use sqlx::postgres::PgValueFormat;
use sqlx::{Column, Executor, Pool, Postgres, Row as SqlxRow, TypeInfo, ValueRef};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

/// PostgreSQL識別子をダブルクォートで安全に囲む
///
/// 識別子内のダブルクォート（"）を""にエスケープする。
/// テーブル名・カラム名を SQL に埋め込む際に使用し、SQL インジェクションを防ぐ。
/// カラム取得など $1 パラメータバインディングが使える箇所では本関数は不要。
pub fn escape_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// SQLの値を文字列に変換
///
/// NULL値は"NULL"文字列として返す
/// 各種SQL型（INT, VARCHAR, TIMESTAMP等）を適切に文字列化
pub(crate) fn convert_value_to_string(
    row: &sqlx::postgres::PgRow,
    index: usize,
    col: &sqlx::postgres::PgColumn,
) -> String {
    let value = row.try_get_raw(index).ok();

    if let Some(raw_value) = value {
        if raw_value.is_null() {
            return String::from("NULL");
        }

        let type_info = col.type_info();
        let type_name = type_info.name();

        // 型名に応じて適切に変換し、制御文字をサニタイズして返す
        // skimは1行=1アイテムのため、改行等が含まれるとUI崩れの原因になる
        // sqlx の TypeInfo::name() は大文字を返す（"INT4", "VARCHAR" 等）
        // ただし custom type は小文字になる場合があるため to_uppercase() で正規化する
        let raw = match type_name.to_uppercase().as_str() {
            "INT2" | "SMALLINT" => row
                .try_get::<i16, _>(index)
                .map(|v| v.to_string())
                .unwrap_or_else(|_| String::from("NULL")),
            "INT4" | "INT" | "INTEGER" | "SERIAL" => {
                // SERIAL は INT4 として格納される
                row.try_get::<i32, _>(index)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|_| String::from("NULL"))
            }
            "INT8" | "BIGINT" | "BIGSERIAL" => {
                // BIGSERIAL は INT8 として格納される
                row.try_get::<i64, _>(index)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|_| String::from("NULL"))
            }
            "FLOAT4" | "REAL" => row
                .try_get::<f32, _>(index)
                .map(|v| format_numeric(v as f64))
                .unwrap_or_else(|_| String::from("NULL")),
            "FLOAT8" | "DOUBLE PRECISION" => row
                .try_get::<f64, _>(index)
                .map(format_numeric)
                .unwrap_or_else(|_| String::from("NULL")),
            "NUMERIC" | "DECIMAL" => {
                // sqlx の f64::Decode は FLOAT8 専用のため numeric に直接使えない。
                // テキスト・バイナリ両プロトコルに対応した専用関数でデコードする。
                decode_pg_numeric(row, index)
            }
            "VARCHAR" | "BPCHAR" | "CHAR" | "TEXT" | "NAME" => row
                .try_get::<String, _>(index)
                .unwrap_or_else(|_| String::from("NULL")),
            "DATE" => decode_pg_date(row, index),
            "TIMESTAMP" => decode_pg_timestamp(row, index),
            "TIMESTAMPTZ" => decode_pg_timestamptz(row, index),
            "TIME" | "TIMETZ" => {
                // TIME はテキストプロトコルで String デコード可能
                row.try_get::<String, _>(index)
                    .unwrap_or_else(|_| String::from("NULL"))
            }
            "BOOL" | "BOOLEAN" => row
                .try_get::<bool, _>(index)
                .map(|v| v.to_string())
                .unwrap_or_else(|_| String::from("NULL")),
            "BYTEA" => String::from("[BYTEA]"),
            _ => {
                // 未知の型は文字列として取得を試みる
                row.try_get::<String, _>(index)
                    .unwrap_or_else(|_| format!("[{}]", type_name))
            }
        };

        // 改行・タブ等の制御文字を置換（skim表示のUI崩れ防止）
        sanitize_for_display(&raw)
    } else {
        String::from("NULL")
    }
}

/// PostgreSQL の numeric 型を文字列にデコードする
///
/// テキストプロトコル: PostgreSQL が送る文字列をそのまま返す（例: "160.808000"）
/// バイナリプロトコル: base-10000 のバイナリ表現を f64 経由で文字列化する
fn decode_pg_numeric(row: &sqlx::postgres::PgRow, index: usize) -> String {
    let raw = match row.try_get_raw(index) {
        Ok(v) => v,
        Err(_) => return String::from("NULL"),
    };

    match raw.format() {
        PgValueFormat::Text => {
            // テキストプロトコル: バイト列は UTF-8 の数値文字列
            match raw.as_str() {
                Ok(s) => s.to_string(),
                Err(_) => String::from("NULL"),
            }
        }
        PgValueFormat::Binary => {
            // バイナリプロトコル: PgNumeric のバイナリ形式を手動デコードする
            // 構造: num_digits(u16) + weight(i16) + sign(u16) + dscale(u16) + digits([i16])
            // sign: 0x0000=正, 0x4000=負, 0xC000=NaN
            let bytes = match raw.as_bytes() {
                Ok(b) => b,
                Err(_) => return String::from("NULL"),
            };
            if bytes.len() < 8 {
                return String::from("NULL");
            }
            let num_digits = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
            let weight = i16::from_be_bytes([bytes[2], bytes[3]]);
            let sign = u16::from_be_bytes([bytes[4], bytes[5]]);
            let dscale = u16::from_be_bytes([bytes[6], bytes[7]]) as usize;

            if sign == 0xC000 {
                return String::from("NaN");
            }
            if bytes.len() < 8 + num_digits * 2 {
                return String::from("NULL");
            }

            // base-10000 の桁列を f64 に変換する
            let mut value: f64 = 0.0;
            for i in 0..num_digits {
                let digit = i16::from_be_bytes([bytes[8 + i * 2], bytes[8 + i * 2 + 1]]) as f64;
                // digits[i] は 10000^(weight - i) の位
                let exp = (weight as i32 - i as i32) as f64;
                value += digit * 10000f64.powf(exp);
            }
            if sign == 0x4000 {
                value = -value;
            }

            // dscale（小数点以下の桁数）に合わせてフォーマットする
            format!("{:.prec$}", value, prec = dscale)
        }
    }
}

/// DATE 型を文字列にデコードする
///
/// バイナリプロトコルでは i32（Julian date から 2000-01-01 を基点とした日数）として送られる。
/// chrono の NaiveDate で正確にデコードする。
fn decode_pg_date(row: &sqlx::postgres::PgRow, index: usize) -> String {
    use chrono::NaiveDate;
    match row.try_get::<NaiveDate, _>(index) {
        Ok(d) => d.format("%Y-%m-%d").to_string(),
        Err(_) => String::from("NULL"),
    }
}

/// TIMESTAMP 型（タイムゾーンなし）を文字列にデコードする
///
/// バイナリプロトコルでは i64（2000-01-01 00:00:00 からのマイクロ秒）として送られる。
/// chrono の NaiveDateTime で正確にデコードする。
fn decode_pg_timestamp(row: &sqlx::postgres::PgRow, index: usize) -> String {
    use chrono::NaiveDateTime;
    match row.try_get::<NaiveDateTime, _>(index) {
        Ok(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        Err(_) => String::from("NULL"),
    }
}

/// TIMESTAMPTZ 型（タイムゾーンあり）を文字列にデコードする
///
/// バイナリプロトコルでは i64（UTC 2000-01-01 からのマイクロ秒）として送られる。
/// chrono の DateTime<Utc> で正確にデコードする。
fn decode_pg_timestamptz(row: &sqlx::postgres::PgRow, index: usize) -> String {
    use chrono::{DateTime, Utc};
    match row.try_get::<DateTime<Utc>, _>(index) {
        Ok(dt) => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        Err(_) => String::from("NULL"),
    }
}

/// NUMERIC/DECIMAL 値を表示用文字列に変換する
///
/// f64 の `to_string()` は末尾ゼロを落とす（10.50 → "10.5"）一方、
/// `format!("{:.10}")` は不要なゼロが並ぶ。
/// ここでは有効桁数10桁で丸めてから末尾ゼロをトリムする。
/// 例: 10.50000000001 → "10.5"、155.00 → "155"、3.14159 → "3.14159"
fn format_numeric(v: f64) -> String {
    // 有効桁数10桁で丸める（f64 の精度ノイズを除去）
    let s = format!("{:.10}", v);
    // 小数点を含む場合のみ末尾ゼロをトリム
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    } else {
        s
    }
}

/// 制御文字を表示用に置換する
///
/// 改行→⏎、タブ→→、その他制御文字→空白に変換
fn sanitize_for_display(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\n' => result.push('⏎'),
            '\r' => {} // CRは無視
            '\t' => result.push('→'),
            c if c.is_control() => result.push(' '),
            c => result.push(c),
        }
    }
    result
}

/// クエリ実行結果
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// カラム名リスト
    pub columns: Vec<String>,

    /// データ行（各行は文字列のベクタ）
    pub rows: Vec<Vec<String>>,

    /// 実行時間
    pub execution_time: Duration,

    /// 結果を表示すべきかどうか（USE, SETなどのコマンドはfalse）
    pub should_display: bool,
}

impl QueryResult {
    /// 行数を返す
    ///
    /// rows.len()の別名。row_countフィールドとrows.lenが乖離するバグを防ぐためメソッドで提供する。
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

impl QueryResult {
    /// メモリ使用量の概算を計算（バイト単位）
    ///
    /// Phase 3: メモリ最適化のためのプロファイリング情報
    pub fn estimate_memory_usage(&self) -> usize {
        let mut total = 0;

        // カラム名のメモリ使用量
        total += self.columns.iter().map(|s| s.capacity()).sum::<usize>();

        // データ行のメモリ使用量
        for row in &self.rows {
            total += row.iter().map(|s| s.capacity()).sum::<usize>();
        }

        // ベクタ自体のオーバーヘッド
        total += std::mem::size_of::<Vec<String>>() * (1 + self.rows.len());

        total
    }

    /// メモリ使用量を人間が読みやすい形式で返す
    pub fn format_memory_usage(&self) -> String {
        let bytes = self.estimate_memory_usage();
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.2} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }
}

/// SQLクエリを実行（ストリーミング版）
///
/// Phase 3: ストリーミング処理で大量データに対応
/// メモリに全データを読み込まず、順次処理することでメモリ使用量を削減
///
/// PostgreSQLでは接続単位でデータベースが固定されるため、
/// current_database パラメータは不要であり、USE文の先行実行も行わない。
pub async fn execute_query(
    pool: &Pool<Postgres>,
    sql: &str,
    _current_database: Option<&str>,
) -> Result<QueryResult> {
    tracing::debug!("Executing query: {}", sql);
    let start = Instant::now();

    // SETなどプリペアドステートメントをサポートしないコマンドを検出
    // これらは結果を返さないため、execute()で実行
    let sql_trimmed = sql.trim().to_uppercase();
    let is_non_prepared_command = sql_trimmed.starts_with("SET ");

    // プリペアドステートメント非対応のコマンドは execute() で実行
    if is_non_prepared_command {
        pool.execute(sql).await.map_err(Error::QueryExecution)?;

        let execution_time = start.elapsed();
        tracing::info!("Command executed successfully in {:?}", execution_time);

        // SET コマンドは結果を表示しない
        return Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            execution_time,
            should_display: false,
        });
    }

    // ストリーム取得
    // PostgreSQLでは接続ごとにDBが固定されるため、USE先行実行は不要
    let mut stream: std::pin::Pin<
        Box<
            dyn futures::Stream<Item = std::result::Result<sqlx::postgres::PgRow, sqlx::Error>>
                + Send,
        >,
    > = Box::pin(sqlx::query(sql).fetch(pool));

    use futures::StreamExt;

    let mut columns = Vec::new();
    let mut data_rows = Vec::new();

    while let Some(row_result) = stream.next().await {
        let row = row_result.map_err(Error::QueryExecution)?;

        // 最初の行からカラム名を取得
        if data_rows.is_empty() {
            columns = row
                .columns()
                .iter()
                .map(|col| col.name().to_string())
                .collect();
        }

        // データ行を変換
        let data_row: Vec<String> = row
            .columns()
            .iter()
            .enumerate()
            .map(|(i, col)| convert_value_to_string(&row, i, col))
            .collect();

        data_rows.push(data_row);

        // メモリ使用量のログ（10万行ごと）
        if data_rows.len() % 100_000 == 0 {
            tracing::info!("Fetched {} rows so far...", data_rows.len());
        }
    }

    // 0件結果でも列ヘッダーを表示できるよう、必要時のみメタデータを補完する
    if data_rows.is_empty() && columns.is_empty() {
        match pool.describe(sql).await {
            Ok(describe) => {
                columns = describe
                    .columns()
                    .iter()
                    .map(|col| col.name().to_string())
                    .collect();
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to describe query result columns for empty result: {}",
                    e
                );
            }
        }
    }

    let execution_time = start.elapsed();

    let result = QueryResult {
        columns,
        rows: data_rows,
        execution_time,
        should_display: true,
    };

    // Phase 3: メモリ使用量のログ出力
    let memory_usage = result.format_memory_usage();
    tracing::info!(
        "Query executed successfully: {} rows in {:?}, estimated memory: {}",
        result.rows.len(),
        execution_time,
        memory_usage
    );

    Ok(result)
}

/// SQLの先頭コメント（`/* ... */` および `-- ...`）を読み飛ばし、最初の意味あるトークンを返す
///
/// skimやTUI表示層ではなく、SQL意味論的な判定に使うため query.rs に置く。
fn first_meaningful_token(sql: &str) -> &str {
    let mut s = sql.trim();
    loop {
        if s.starts_with("/*") {
            if let Some(end) = s.find("*/") {
                s = s[end + 2..].trim_start();
            } else {
                return "";
            }
        } else if s.starts_with("--") {
            if let Some(newline) = s.find('\n') {
                s = s[newline + 1..].trim_start();
            } else {
                return "";
            }
        } else {
            break;
        }
    }
    s.split_whitespace().next().unwrap_or("")
}

/// CTE（WITH句）の後に続くSQL本体が書き込みDMLかどうかを判定する
///
/// WITH句のCTE定義を括弧のネストで追跡し、全CTE定義の終了後の先頭トークンを確認する。
/// 複数CTEのカンマ区切り（`WITH a AS (...), b AS (...)`）にも対応する。
fn cte_contains_write(sql: &str) -> bool {
    let upper = sql.to_uppercase();
    let write_keywords = ["INSERT", "UPDATE", "DELETE"];

    let start = match upper.find("WITH") {
        Some(pos) => pos + 4,
        None => return false,
    };

    let bytes = upper.as_bytes();
    let len = bytes.len();
    let mut depth = 0i32;
    let mut i = start;

    while i < len {
        match bytes[i] {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    // 全CTEの括弧を抜けた後、次のトークンを確認する
                    let remaining = upper[i..].trim_start();
                    if remaining.starts_with(',') {
                        // カンマ区切りの次のCTEがあるので読み進める
                        i += upper[i..].len() - remaining.len() + 1;
                        continue;
                    }
                    let token = remaining.split_whitespace().next().unwrap_or("");
                    return write_keywords.contains(&token);
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    false
}

/// 書き込み系SQLかどうかを判定する
///
/// readonlyモードでブロックすべきSQL文の先頭トークンをチェックする。
/// サーバー側でもブロックされるが、ユーザーへの即時フィードバックのためクライアントでも検査する。
/// コメント（`/* */` ブロック・`--` 行）を読み飛ばし、CTE（WITH句）も正しく判定する。
pub fn is_write_sql(sql: &str) -> bool {
    let first_token = first_meaningful_token(sql).to_uppercase();

    if first_token == "WITH" {
        return cte_contains_write(sql);
    }

    matches!(
        first_token.as_str(),
        "INSERT"
            | "UPDATE"
            | "DELETE"
            | "DROP"
            | "ALTER"
            | "TRUNCATE"
            | "CREATE"
            | "REPLACE"
            | "RENAME"
            | "GRANT"
            | "REVOKE"
    )
}

pub async fn execute_query_with_timeout(
    pool: &Pool<Postgres>,
    sql: &str,
    timeout: Duration,
    current_database: Option<&str>,
) -> Result<QueryResult> {
    tokio::time::timeout(timeout, execute_query(pool, sql, current_database))
        .await
        .map_err(|_| Error::QueryTimeout)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_write_sql_basic() {
        assert!(is_write_sql("INSERT INTO users VALUES (1)"));
        assert!(is_write_sql("DELETE FROM users WHERE id = 1"));
        assert!(is_write_sql("UPDATE users SET name = 'x'"));
        assert!(is_write_sql("DROP TABLE users"));
        assert!(!is_write_sql("SELECT * FROM users"));
        assert!(!is_write_sql("EXPLAIN SELECT 1"));
    }

    #[test]
    fn test_is_write_sql_with_comments() {
        assert!(!is_write_sql("/* comment */ SELECT 1"));
        assert!(is_write_sql("/* comment */ INSERT INTO t VALUES (1)"));
        assert!(!is_write_sql("-- line comment\nSELECT 1"));
        assert!(is_write_sql("-- comment\nDELETE FROM t"));
        assert!(!is_write_sql("/* multi\nline */ SELECT 1"));
    }

    #[test]
    fn test_is_write_sql_cte() {
        assert!(!is_write_sql("WITH cte AS (SELECT 1) SELECT * FROM cte"));
        assert!(is_write_sql(
            "WITH cte AS (SELECT 1) INSERT INTO t SELECT * FROM cte"
        ));
        assert!(is_write_sql(
            "WITH cte AS (SELECT 1) DELETE FROM t WHERE id IN (SELECT * FROM cte)"
        ));
        assert!(!is_write_sql(
            "WITH a AS (SELECT 1), b AS (SELECT 2) SELECT * FROM a, b"
        ));
        assert!(is_write_sql(
            "WITH a AS (SELECT 1), b AS (SELECT 2) UPDATE t SET x = 1"
        ));
    }

    #[test]
    fn test_is_write_sql_cte_with_comments() {
        assert!(!is_write_sql(
            "/* comment */ WITH cte AS (SELECT 1) SELECT * FROM cte"
        ));
        assert!(is_write_sql(
            "-- comment\nWITH cte AS (SELECT 1) INSERT INTO t SELECT * FROM cte"
        ));
    }

    #[test]
    fn test_escape_identifier_plain() {
        // PostgreSQLではダブルクォート形式
        assert_eq!(escape_identifier("users"), "\"users\"");
    }

    #[test]
    fn test_escape_identifier_with_double_quote() {
        // PostgreSQLでは内部のダブルクォートを""にエスケープ
        assert_eq!(escape_identifier("my\"table"), "\"my\"\"table\"");
    }

    #[test]
    fn test_escape_identifier_empty() {
        assert_eq!(escape_identifier(""), "\"\"");
    }

    #[test]
    fn test_query_result_creation() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![
                vec!["1".to_string(), "Alice".to_string()],
                vec!["2".to_string(), "Bob".to_string()],
            ],
            execution_time: Duration::from_millis(100),
            should_display: true,
        };

        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.row_count(), 2);
        assert_eq!(result.rows.len(), 2);
        assert!(result.should_display);
    }

    #[test]
    fn test_query_result_empty() {
        let result = QueryResult {
            columns: vec![],
            rows: vec![],
            execution_time: Duration::from_millis(10),
            should_display: true,
        };

        assert_eq!(result.columns.len(), 0);
        assert_eq!(result.row_count(), 0);
        assert_eq!(result.rows.len(), 0);
    }

    #[test]
    fn test_query_result_with_null_values() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "name".to_string(), "age".to_string()],
            rows: vec![
                vec!["1".to_string(), "Alice".to_string(), "NULL".to_string()],
                vec!["2".to_string(), "NULL".to_string(), "30".to_string()],
            ],
            execution_time: Duration::from_millis(50),
            should_display: true,
        };

        assert_eq!(result.row_count(), 2);
        assert_eq!(result.rows[0][2], "NULL");
        assert_eq!(result.rows[1][1], "NULL");
    }

    #[test]
    fn test_memory_usage_estimation() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![
                vec!["1".to_string(), "Alice".to_string()],
                vec!["2".to_string(), "Bob".to_string()],
            ],
            execution_time: Duration::from_millis(100),
            should_display: true,
        };

        // メモリ使用量が0より大きいことを確認
        let memory_usage = result.estimate_memory_usage();
        assert!(memory_usage > 0);

        // フォーマットされた文字列が取得できることを確認
        let formatted = result.format_memory_usage();
        assert!(!formatted.is_empty());
        assert!(formatted.contains("B") || formatted.contains("KB") || formatted.contains("MB"));
    }

    #[test]
    fn test_memory_usage_large_result() {
        // 大量データでのメモリ使用量計算
        let mut rows = Vec::new();
        for i in 0..10000 {
            rows.push(vec![
                i.to_string(),
                format!("User_{}", i),
                format!("user_{}@example.com", i),
            ]);
        }

        let result = QueryResult {
            columns: vec!["id".to_string(), "name".to_string(), "email".to_string()],
            rows,
            execution_time: Duration::from_millis(500),
            should_display: true,
        };

        let memory_usage = result.estimate_memory_usage();
        // 10000行のデータなので、少なくとも100KB以上は使用しているはず
        assert!(memory_usage > 100_000);

        let formatted = result.format_memory_usage();
        println!("Memory usage for 10000 rows: {}", formatted);
    }
}
