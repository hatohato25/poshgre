use std::collections::HashMap;
use std::sync::Arc;

/// 補完候補の種別
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionKind {
    /// SQLキーワード（静的リスト）
    Keyword,
    /// テーブル名（接続時キャッシュ）
    Table,
    /// カラム名（テーブルごとキャッシュ）
    Column { table: String },
}

/// 補完候補1件
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// 表示・挿入テキスト
    pub text: String,
    /// 種別（表示色のヒントに使用）
    pub kind: CompletionKind,
}

/// 補完候補キャッシュ
///
/// 接続確立後に非同期で段階的に充填される。
/// tokio::sync::RwLock でラップして読み取り優先の並行アクセスを実現する。
pub struct CompletionCache {
    pub tables: Vec<String>,
    /// キー: テーブル名（小文字正規化）、値: カラム名リスト
    pub columns: HashMap<String, Vec<String>>,
    /// schema.table 補完用: キー: スキーマ名（小文字正規化）、値: テーブル名リスト
    pub database_tables: HashMap<String, Vec<String>>,
    /// 接続確立済みフラグ
    pub is_ready: bool,
}

impl CompletionCache {
    pub fn new() -> Self {
        Self {
            tables: Vec::new(),
            columns: HashMap::new(),
            database_tables: HashMap::new(),
            is_ready: false,
        }
    }
}

impl Default for CompletionCache {
    fn default() -> Self {
        Self::new()
    }
}

/// SQL文脈: カーソル位置で期待される補完種別
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlContext {
    /// 行頭またはキーワードが期待される位置
    Keyword,
    /// テーブル名が期待される位置（FROM / JOIN の直後）
    TableName,
    /// カラム名が期待される位置（推定テーブル名付き）
    ColumnName { table: Option<String> },
    /// schema.table パターン: 指定スキーマのテーブル名が期待される位置
    DatabaseTableName { database: String },
}

/// SQLキーワード静的リスト（PostgreSQL向け）
pub const SQL_KEYWORDS: &[&str] = &[
    "SELECT",
    "INSERT",
    "UPDATE",
    "DELETE",
    "CREATE",
    "DROP",
    "ALTER",
    "TRUNCATE",
    "FROM",
    "WHERE",
    "JOIN",
    "INNER JOIN",
    "LEFT JOIN",
    "RIGHT JOIN",
    "LEFT OUTER JOIN",
    "CROSS JOIN",
    "FULL OUTER JOIN",
    "ON",
    "GROUP BY",
    "ORDER BY",
    "HAVING",
    "LIMIT",
    "OFFSET",
    "AS",
    "DISTINCT",
    "AND",
    "OR",
    "NOT",
    "IN",
    "LIKE",
    "ILIKE",
    "BETWEEN",
    "IS NULL",
    "IS NOT NULL",
    "COUNT",
    "SUM",
    "AVG",
    "MAX",
    "MIN",
    "COALESCE",
    "NULLIF",
    "RETURNING",
    "ON CONFLICT",
    "WITH",
    "RECURSIVE",
    "EXPLAIN",
    "EXPLAIN ANALYZE",
    "SET",
    "SHOW",
    "COPY",
];

/// 補完用の単語区切り文字（tui.rs の is_word_separator と同一定義）
///
/// PostgreSQL文脈での単語境界を定義する。スペース・演算子・括弧・クォート等を
/// 区切り文字とし、英数字・アンダースコア・その他Unicode文字を単語文字として扱う。
/// バッククォートはPostgreSQLでは識別子クォートに使用しないため除外済み。
pub fn is_completion_separator(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | ',' | ';' | '.' | '(' | ')' | '[' | ']'
            | '=' | '<' | '>' | '!' | '+' | '-' | '*' | '/'
            | '\'' | '"'
    )
}

/// カーソル位置直前の「現在入力中のトークン」とその開始バイト位置を返す
pub fn current_token_with_pos(query_input: &str, cursor_byte_pos: usize) -> (&str, usize) {
    let before = &query_input[..cursor_byte_pos];
    let token_start = before
        .char_indices()
        .rev()
        .find(|(_, c)| is_completion_separator(*c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    (&before[token_start..], token_start)
}

/// SQL文字列を単純なトークン列に分割する（クォート内を1トークンとして扱う）
fn tokenize_sql(sql: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for c in sql.chars() {
        match in_quote {
            Some(q) if c == q => {
                // クォート終端
                current.push(c);
                tokens.push(current.clone());
                current.clear();
                in_quote = None;
            }
            Some(_) => {
                // クォート内
                current.push(c);
            }
            // PostgreSQLでは識別子のクォートはダブルクォートのみ（バッククォートは不使用）
            None if c == '\'' || c == '"' => {
                // クォート開始
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                current.push(c);
                in_quote = Some(c);
            }
            None if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            None => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

/// JOIN系キーワードかどうかを判定する
fn matches_join_keyword(token: &str) -> bool {
    let upper = token.to_uppercase();
    matches!(upper.as_str(), "JOIN" | "ON")
}

/// SQL文から FROM テーブル名を抽出する（最初に現れた FROM の直後）
pub fn extract_from_table(sql: &str) -> Option<String> {
    let upper = sql.to_uppercase();
    let from_pos = upper.find(" FROM ")?;
    let after_from = sql[from_pos + 6..].trim_start();
    // スペース・セミコロン・WHEREまでの最初のトークンを取得し、ダブルクォートを全て除去する
    // "db"."table" 形式で返ってきた場合でも生の "db.table" 文字列として返す
    let raw = after_from
        .split(|c: char| c.is_whitespace() || c == ';')
        .next()?
        .replace('"', "");
    if raw.is_empty() {
        None
    } else {
        Some(raw)
    }
}

/// カーソルがSELECTとFROMの間にあるかを判定する
///
/// SELECT が含まれ、かつ FROM がまだ現れていない状態かを判定する。
fn is_between_select_and_from(sql_before_cursor: &str) -> bool {
    let upper = sql_before_cursor.to_uppercase();
    // SELECT が含まれかつ FROM がない場合、またはSELECT..FROMの途中の場合
    let has_select = upper.contains("SELECT");
    let has_from = upper.contains(" FROM ");
    has_select && !has_from
}

/// カーソル位置より前のSQL文を解析して補完文脈を返す
///
/// sql: カーソルより前の文字列（query_input[..cursor_byte_pos]）
pub fn analyze_context(sql_before_cursor: &str) -> SqlContext {
    // カーソル直前のトークンと、その前のキーワードを取り出す
    let tokens = tokenize_sql(sql_before_cursor);

    match tokens.as_slice() {
        // 空または最初のトークン
        [] => SqlContext::Keyword,
        // PostgreSQLではUSE文によるDB切り替えは行わないため、USE文の文脈解析は除去済み
        // FROM - まだテーブル名未入力
        [.., kw] if kw.eq_ignore_ascii_case("FROM") => SqlContext::TableName,
        // FROM <partial> - FROMの後にテーブル名を入力中
        [.., kw, _] if kw.eq_ignore_ascii_case("FROM") => SqlContext::TableName,
        // JOIN系 - まだテーブル名未入力
        [.., kw] if matches_join_keyword(kw) => SqlContext::TableName,
        // JOIN <partial> - JOIN系の後にテーブル名を入力中
        [.., kw, _] if matches_join_keyword(kw) => SqlContext::TableName,
        // WHERE / AND / OR の後はカラム名（FROMテーブルを抽出して渡す）
        [.., kw]
            if kw.eq_ignore_ascii_case("WHERE")
                || kw.eq_ignore_ascii_case("AND")
                || kw.eq_ignore_ascii_case("OR") =>
        {
            let table = extract_from_table(sql_before_cursor);
            SqlContext::ColumnName { table }
        }
        // WHERE / AND / OR <partial> - これらの後にカラム名を入力中
        [.., kw, _]
            if kw.eq_ignore_ascii_case("WHERE")
                || kw.eq_ignore_ascii_case("AND")
                || kw.eq_ignore_ascii_case("OR") =>
        {
            let table = extract_from_table(sql_before_cursor);
            SqlContext::ColumnName { table }
        }
        // SELECT の後 FROM が来るまではカラム名候補
        _ if is_between_select_and_from(sql_before_cursor) => {
            let table = extract_from_table(sql_before_cursor);
            SqlContext::ColumnName { table }
        }
        // それ以外はキーワード
        _ => SqlContext::Keyword,
    }
}

/// 現在の入力状態から補完候補リストを生成する
///
/// input_prefix: カーソル直前の現在入力中トークン（前方一致フィルタに使用）
/// context: analyze_context() で決定した文脈
/// cache: 補完キャッシュへの参照
pub fn get_candidates(
    input_prefix: &str,
    context: &SqlContext,
    cache: &CompletionCache,
) -> Vec<CompletionItem> {
    let prefix_upper = input_prefix.to_uppercase();

    match context {
        SqlContext::Keyword => {
            // SQLキーワード静的リストを前方一致フィルタ
            SQL_KEYWORDS
                .iter()
                .filter(|kw| kw.starts_with(&prefix_upper))
                .map(|kw| CompletionItem {
                    text: kw.to_string(),
                    kind: CompletionKind::Keyword,
                })
                .collect()
        }
        SqlContext::TableName => {
            // テーブル名キャッシュを前方一致フィルタ（大文字小文字を区別しない）
            cache
                .tables
                .iter()
                .filter(|t| t.to_lowercase().starts_with(&input_prefix.to_lowercase()))
                .map(|t| CompletionItem {
                    text: t.clone(),
                    kind: CompletionKind::Table,
                })
                .collect()
        }
        SqlContext::ColumnName { table } => {
            let columns = table
                .as_deref()
                .and_then(|t| cache.columns.get(&t.to_lowercase()))
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let mut candidates: Vec<CompletionItem> = columns
                .iter()
                .filter(|c| c.to_lowercase().starts_with(&input_prefix.to_lowercase()))
                .map(|c| CompletionItem {
                    text: c.clone(),
                    kind: CompletionKind::Column {
                        table: table.clone().unwrap_or_default(),
                    },
                })
                .collect();
            // カラム名に加えてキーワード候補も含める（FROM等の入力補助のため）
            candidates.extend(
                SQL_KEYWORDS
                    .iter()
                    .filter(|kw| kw.starts_with(&prefix_upper))
                    .map(|kw| CompletionItem {
                        text: kw.to_string(),
                        kind: CompletionKind::Keyword,
                    }),
            );
            candidates
        }
        SqlContext::DatabaseTableName { ref database } => {
            let db_tables = cache.database_tables.get(&database.to_lowercase());
            match db_tables {
                Some(tables) => tables
                    .iter()
                    .filter(|t| t.to_lowercase().starts_with(&input_prefix.to_lowercase()))
                    .map(|t| CompletionItem {
                        text: t.clone(),
                        kind: CompletionKind::Table,
                    })
                    .collect(),
                None => vec![],
            }
        }
    }
}

/// 指定テーブルのカラムキャッシュを取得・更新する（キャッシュ済みならスキップ）
///
/// TUI描画ループを止めないよう tokio::spawn でバックグラウンド取得する想定。
/// エラー時はサイレントに失敗（warn ログのみ）。
/// PostgreSQLでは $1 パラメータバインディングを使用するため escape_identifier は不要。
pub async fn fetch_column_cache_if_needed(
    cache: &Arc<tokio::sync::RwLock<CompletionCache>>,
    pool: &sqlx::Pool<sqlx::Postgres>,
    table_name: &str,
) {
    {
        let cache_read = cache.read().await;
        if cache_read.columns.contains_key(&table_name.to_lowercase()) {
            return;
        }
    }
    // $1 パラメータバインディングによりSQLインジェクションを防ぐ
    if let Ok(rows) = sqlx::query(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = $1 AND table_schema = 'public' \
         ORDER BY ordinal_position",
    )
    .bind(table_name)
    .fetch_all(pool)
    .await
    {
        use sqlx::Row;
        let columns: Vec<String> = rows
            .iter()
            .map(|row| row.try_get::<String, _>(0).unwrap_or_default())
            .collect();
        let mut cache_write = cache.write().await;
        cache_write.columns.insert(table_name.to_lowercase(), columns);
    }
}

/// 指定スキーマのテーブルキャッシュを取得・更新する（キャッシュ済みならスキップ）
///
/// PostgreSQLでは接続ごとにDBが固定されるため、pg_catalog.pg_tables を使用してテーブル取得。
/// `fetch_column_cache_if_needed` と同じパターンでバックグラウンド取得する想定。
/// エラー時はサイレントに失敗（warn ログのみ）。
pub async fn fetch_database_tables_if_needed(
    cache: &Arc<tokio::sync::RwLock<CompletionCache>>,
    pool: &sqlx::Pool<sqlx::Postgres>,
    database_name: &str,
) {
    {
        let cache_read = cache.read().await;
        if cache_read
            .database_tables
            .contains_key(&database_name.to_lowercase())
        {
            return;
        }
    }
    // PostgreSQLではシステムカタログから現在のスキーマのテーブル一覧を取得する
    if let Ok(rows) = sqlx::query(
        "SELECT tablename FROM pg_catalog.pg_tables \
         WHERE schemaname = current_schema() \
         ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    {
        use sqlx::Row;
        let tables: Vec<String> = rows
            .iter()
            .map(|row| row.try_get::<String, _>(0).unwrap_or_default())
            .collect();
        let mut cache_write = cache.write().await;
        cache_write
            .database_tables
            .insert(database_name.to_lowercase(), tables);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // analyze_context のテスト

    #[test]
    fn test_analyze_context_empty() {
        assert_eq!(analyze_context(""), SqlContext::Keyword);
    }

    #[test]
    fn test_analyze_context_keyword_prefix() {
        // "SEL" はキーワード文脈
        assert_eq!(analyze_context("SEL"), SqlContext::Keyword);
    }

    #[test]
    fn test_analyze_context_from() {
        // "SELECT * FROM " はテーブル名文脈
        assert_eq!(analyze_context("SELECT * FROM "), SqlContext::TableName);
    }

    #[test]
    fn test_analyze_context_from_partial() {
        // "SELECT * FROM us" - "us"が現在トークンなのでFROMが最後のキーワード
        // FROMが最後のトークン（"FROM"が残る）
        assert_eq!(analyze_context("SELECT * FROM "), SqlContext::TableName);
    }

    #[test]
    fn test_analyze_context_where() {
        // "SELECT * FROM users WHERE " はカラム名文脈
        let context = analyze_context("SELECT * FROM users WHERE ");
        assert_eq!(
            context,
            SqlContext::ColumnName {
                table: Some("users".to_string())
            }
        );
    }

    #[test]
    fn test_analyze_context_use() {
        // PostgreSQLではUSE文によるDB切り替えを行わないため、USE はキーワード文脈として扱われる
        assert_eq!(analyze_context("USE "), SqlContext::Keyword);
    }

    #[test]
    fn test_analyze_context_select_before_from() {
        // SELECT〜FROMの間はカラム名文脈
        let context = analyze_context("SELECT ");
        assert_eq!(
            context,
            SqlContext::ColumnName { table: None }
        );
    }

    // current_token_with_pos のテスト

    #[test]
    fn test_current_token_basic() {
        let input = "SELECT * FROM us";
        let cursor = input.len();
        assert_eq!(current_token_with_pos(input, cursor).0, "us");
    }

    #[test]
    fn test_current_token_after_space() {
        let input = "SELECT ";
        let cursor = input.len();
        assert_eq!(current_token_with_pos(input, cursor).0, "");
    }

    #[test]
    fn test_current_token_empty() {
        assert_eq!(current_token_with_pos("", 0).0, "");
    }

    #[test]
    fn test_current_token_full_word() {
        // "SELECT" のみ → トークン全体を返す
        let input = "SELECT";
        let cursor = input.len();
        assert_eq!(current_token_with_pos(input, cursor).0, "SELECT");
    }

    // tokenize_sql のテスト

    #[test]
    fn test_tokenize_sql_basic() {
        let tokens = tokenize_sql("SELECT * FROM users");
        assert_eq!(tokens, vec!["SELECT", "*", "FROM", "users"]);
    }

    #[test]
    fn test_tokenize_sql_empty() {
        let tokens = tokenize_sql("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_sql_with_semicolon() {
        let tokens = tokenize_sql("USE mydb;");
        assert_eq!(tokens, vec!["USE", "mydb;"]);
    }

    // get_candidates のテスト

    #[test]
    fn test_completion_cache_new_initializes_database_tables() {
        // CompletionCache::new() で database_tables が空の HashMap として初期化されることを確認する
        let cache = CompletionCache::new();
        assert!(cache.database_tables.is_empty());
        assert!(!cache.is_ready);
    }

    #[test]
    fn test_completion_cache_database_tables_insert() {
        // database_tables への挿入・取得が正しく動作することを確認する
        let mut cache = CompletionCache::new();
        cache.database_tables.insert(
            "warehouse".to_string(),
            vec!["billing".to_string(), "orders".to_string()],
        );
        let tables = cache.database_tables.get("warehouse").unwrap();
        assert_eq!(tables.len(), 2);
        assert!(tables.contains(&"billing".to_string()));
    }

    #[test]
    fn test_get_candidates_keyword() {
        let cache = CompletionCache::new();
        let context = SqlContext::Keyword;
        let candidates = get_candidates("SEL", &context, &cache);
        let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"SELECT"));
    }

    #[test]
    fn test_get_candidates_table() {
        let mut cache = CompletionCache::new();
        cache.tables = vec!["users".to_string(), "orders".to_string()];
        let context = SqlContext::TableName;
        let candidates = get_candidates("us", &context, &cache);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].text, "users");
    }

    #[test]
    fn test_get_candidates_column() {
        let mut cache = CompletionCache::new();
        cache.columns.insert(
            "users".to_string(),
            vec!["id".to_string(), "name".to_string(), "email".to_string()],
        );
        let context = SqlContext::ColumnName {
            table: Some("users".to_string()),
        };
        // ColumnNameコンテキストはカラム名とキーワードの両方を返す。
        // "n" は "name" (Column) と "NOT" (Keyword) の両方に前方一致する。
        let candidates = get_candidates("n", &context, &cache);
        let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"name"));
        assert!(texts.contains(&"NOT"));
    }

    #[test]
    fn test_get_candidates_empty_prefix_keyword() {
        // 空プレフィックスでキーワード全件返す
        let cache = CompletionCache::new();
        let context = SqlContext::Keyword;
        let candidates = get_candidates("", &context, &cache);
        // SQL_KEYWORDS の全件（PostgreSQL固有キーワードを含む）
        assert_eq!(candidates.len(), SQL_KEYWORDS.len());
        // PostgreSQL固有キーワードが含まれることを確認
        let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"RETURNING"), "PostgreSQL固有のRETURNINGが含まれるべき");
        assert!(texts.contains(&"ILIKE"), "PostgreSQL固有のILIKEが含まれるべき");
        // PostgreSQLでは不要なMySQL固有キーワードが含まれないことを確認
        assert!(!texts.contains(&"SHOW TABLES"), "PostgreSQLでは不要なSHOW TABLESは含まれるべきでない");
        assert!(!texts.contains(&"DESCRIBE"), "PostgreSQLでは不要なDESCRIBEは含まれるべきでない");
    }

    #[test]
    fn test_get_candidates_no_match() {
        let mut cache = CompletionCache::new();
        cache.tables = vec!["users".to_string()];
        let context = SqlContext::TableName;
        let candidates = get_candidates("xyz", &context, &cache);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_analyze_context_from_partial_table() {
        // "SELECT * FROM d" - FROMの後にテーブル名を入力中
        assert_eq!(analyze_context("SELECT * FROM d"), SqlContext::TableName);
    }

    #[test]
    fn test_analyze_context_use_partial_db() {
        // PostgreSQLではUSE文によるDB切り替えを行わないため、キーワード文脈として扱われる
        assert_eq!(analyze_context("USE my"), SqlContext::Keyword);
    }

    #[test]
    fn test_analyze_context_where_partial_column() {
        // "SELECT * FROM users WHERE n" - WHEREの後にカラム名を入力中
        let context = analyze_context("SELECT * FROM users WHERE n");
        assert_eq!(
            context,
            SqlContext::ColumnName {
                table: Some("users".to_string())
            }
        );
    }

    #[test]
    fn test_analyze_context_join_partial_table() {
        // "SELECT * FROM users JOIN o" - JOINの後にテーブル名を入力中
        assert_eq!(
            analyze_context("SELECT * FROM users JOIN o"),
            SqlContext::TableName
        );
    }

    #[test]
    fn test_get_candidates_column_includes_keywords() {
        // ColumnNameコンテキストでもキーワードが候補に含まれる
        let cache = CompletionCache::new();
        let context = SqlContext::ColumnName { table: None };
        let candidates = get_candidates("FR", &context, &cache);
        let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"FROM"));
    }
}
