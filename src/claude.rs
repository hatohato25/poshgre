//! Claude API 連携モジュール（tool use エージェント方式）
//!
//! Claude を PostgreSQL SQL 生成エージェントとして動作させる。
//! Claude は `execute_query` ツールを使い、スキーマ確認や
//! サンプルデータ取得などを自律的に行いながら最終的な SQL を生成する。
//!
//! セキュリティ:
//! - `execute_query` ツール経由のクエリは読み取り専用のみ許可する
//! - API キーはログに出力しない

use sqlx::{Column, Pool, Postgres, Row};

use crate::error::{Error, Result};
use crate::query::is_write_sql;

/// Claude API エンドポイント
const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Claude API のバージョン
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// エージェントループの最大ターン数
///
/// 無限ループを防ぐため、ツール呼び出しが続く場合でも最大10ターンで打ち切る。
const MAX_AGENT_TURNS: usize = 10;

/// tool_result として返すクエリ結果の最大行数
///
/// コンテキスト長の節約のため、結果は最大100行に制限する。
const MAX_RESULT_ROWS: usize = 100;

/// generate_sql のシステムプロンプト
const SYSTEM_PROMPT: &str = "\
You are a PostgreSQL SQL generation expert. \
To generate the correct SQL for the user's request, you may use the execute_query tool \
to inspect the database schema or sample data as needed. \
When you have enough information, respond with ONLY the final SQL query, no explanation.";

/// execute_query ツール定義 JSON
///
/// Claude が呼び出せるツールを定義する。
/// 書き込み系クエリは Rust 側でブロックするが、
/// ツール定義では read-only の意図を明示することで Claude の自己制御を促す。
const EXECUTE_QUERY_TOOL: &str = r#"{
  "name": "execute_query",
  "description": "Execute a read-only SQL query against the PostgreSQL database to retrieve schema information or sample data needed to generate the target SQL.",
  "input_schema": {
    "type": "object",
    "properties": {
      "sql": {
        "type": "string",
        "description": "A read-only SELECT or schema inspection query"
      }
    },
    "required": ["sql"]
  }
}"#;

/// Claude API エージェントを実行して SQL を生成する
///
/// Claude との会話を繰り返しながら（最大 MAX_AGENT_TURNS ターン）、
/// execute_query ツールを介してスキーマや現在のデータを調べ、
/// 最終的にユーザーの要求に合った SQL を生成して返す。
///
/// 戻り値: 生成された SQL 文字列
pub async fn run_agent(
    api_key: &str,
    model: &str,
    pool: &Pool<Postgres>,
    user_prompt: &str,
) -> Result<String> {
    // reqwest クライアントをタイムアウト付きで構築する
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Error::ClaudeApi(format!("HTTP クライアントの構築に失敗しました: {}", e)))?;

    let tool_def: serde_json::Value = serde_json::from_str(EXECUTE_QUERY_TOOL)
        .map_err(|e| Error::ClaudeApi(format!("ツール定義の解析に失敗しました: {}", e)))?;

    // 会話履歴（messages 配列）
    let mut messages: Vec<serde_json::Value> = vec![serde_json::json!({
        "role": "user",
        "content": user_prompt
    })];

    for turn in 0..MAX_AGENT_TURNS {
        tracing::debug!("Agent turn {}/{}", turn + 1, MAX_AGENT_TURNS);

        let request_body = serde_json::json!({
            "model": model,
            "max_tokens": 2048,
            "system": SYSTEM_PROMPT,
            "tools": [tool_def],
            "messages": messages
        });

        // Claude API へリクエストを送信する（APIキーはログに出力しない）
        let response = client
            .post(CLAUDE_API_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| Error::ClaudeApi(format!("API リクエストに失敗しました: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::ClaudeApi(format!(
                "API がエラーを返しました (HTTP {}): {}",
                status, body
            )));
        }

        let response_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| Error::ClaudeApi(format!("レスポンスの解析に失敗しました: {}", e)))?;

        // レスポンスの content 配列を処理する
        let content = response_json["content"]
            .as_array()
            .ok_or_else(|| Error::ClaudeApi("レスポンスに content フィールドがありません".to_string()))?;

        // tool_use ブロックが含まれるか確認する
        let has_tool_use = content.iter().any(|b| b["type"].as_str() == Some("tool_use"));

        if has_tool_use {
            // tool_use ブロックを収集してクエリを実行し、次のターンの messages に追加する
            let mut tool_results: Vec<serde_json::Value> = Vec::new();

            for block in content {
                if block["type"].as_str() != Some("tool_use") {
                    continue;
                }

                let tool_id = block["id"].as_str().unwrap_or("").to_string();
                let tool_name = block["name"].as_str().unwrap_or("").to_string();
                let input = &block["input"];

                if tool_name == "execute_query" {
                    let sql = input["sql"].as_str().unwrap_or("").to_string();
                    tracing::debug!("Agent tool call: execute_query({})", sql);

                    let result = execute_query_safe(pool, &sql).await;

                    tool_results.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_id,
                        "content": result
                    }));
                } else {
                    // 未知のツール呼び出しはエラーとして返す
                    tool_results.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_id,
                        "content": format!("Unknown tool: {}", tool_name),
                        "is_error": true
                    }));
                }
            }

            // アシスタントの返答（tool_use を含む）を会話履歴に追加する
            messages.push(serde_json::json!({
                "role": "assistant",
                "content": content.clone()
            }));

            // ツール実行結果を会話履歴に追加する
            messages.push(serde_json::json!({
                "role": "user",
                "content": tool_results
            }));

            // 次のターンへ続く
            continue;
        }

        // tool_use なし: テキストブロックから SQL を抽出して返す
        let text = content
            .iter()
            .find(|b| b["type"].as_str() == Some("text"))
            .and_then(|b| b["text"].as_str())
            .unwrap_or("")
            .to_string();

        let sql = extract_sql_from_text(&text);
        tracing::debug!("Agent generated SQL: {}", sql);
        return Ok(sql);
    }

    // 最大ターン数を超えた場合はエラー
    Err(Error::ClaudeApi(
        "エージェントが最大ターン数を超えました".to_string(),
    ))
}

/// ツール経由で安全に読み取りクエリを実行する
///
/// 書き込み系 SQL は `is_write_sql()` で検出してブロックし、
/// 読み取り系 SQL のみ実行して結果を TSV 形式の文字列で返す。
/// 結果は最大 MAX_RESULT_ROWS 行に制限してコンテキスト長を節約する。
async fn execute_query_safe(pool: &Pool<Postgres>, sql: &str) -> String {
    // 書き込み系 SQL はクライアント側でブロックする
    if is_write_sql(sql) {
        return "Error: Write operations are not allowed in the agent context. \
               Only SELECT and schema inspection queries are permitted."
            .to_string();
    }

    match sqlx::query(sql).fetch_all(pool).await {
        Ok(rows) => {
            if rows.is_empty() {
                return "(empty result)".to_string();
            }

            let mut output = String::new();

            // ヘッダー行: カラム名を TSV で出力する
            let columns: Vec<String> = rows[0]
                .columns()
                .iter()
                .map(|c| c.name().to_string())
                .collect();
            output.push_str(&columns.join("\t"));
            output.push('\n');

            // データ行: 最大 MAX_RESULT_ROWS 行まで出力する
            let row_count = rows.len().min(MAX_RESULT_ROWS);
            for row in &rows[..row_count] {
                let values: Vec<String> = (0..columns.len())
                    .map(|i| {
                        row.try_get::<String, _>(i)
                            .or_else(|_| row.try_get::<i64, _>(i).map(|v| v.to_string()))
                            .or_else(|_| row.try_get::<f64, _>(i).map(|v| v.to_string()))
                            .or_else(|_| row.try_get::<bool, _>(i).map(|v| v.to_string()))
                            .unwrap_or_else(|_| "NULL".to_string())
                    })
                    .collect();
                output.push_str(&values.join("\t"));
                output.push('\n');
            }

            // 切り詰め通知
            if rows.len() > MAX_RESULT_ROWS {
                output.push_str(&format!(
                    "... ({} rows total, showing first {})",
                    rows.len(),
                    MAX_RESULT_ROWS
                ));
            }

            output
        }
        Err(e) => format!("Query error: {}", e),
    }
}

/// テキストから SQL 文を抽出する
///
/// コードブロック（```sql ... ``` または ``` ... ```）が含まれている場合は
/// その中身を取り出す。コードブロックがない場合はテキスト全体をトリムして返す。
fn extract_sql_from_text(text: &str) -> String {
    let text = text.trim();

    // ```sql ... ``` または ``` ... ``` のコードブロックを検索する
    if let Some(start) = text.find("```") {
        let after_backticks = &text[start + 3..];
        // 言語識別子（sql, SQL 等）をスキップする
        let content_start = if let Some(newline_pos) = after_backticks.find('\n') {
            newline_pos + 1
        } else {
            0
        };
        let content = &after_backticks[content_start..];
        if let Some(end) = content.find("```") {
            return content[..end].trim().to_string();
        }
    }

    // コードブロックがない場合はテキスト全体を返す
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_sql_from_code_block() {
        let text = "Here is the SQL:\n```sql\nSELECT * FROM users;\n```";
        let sql = extract_sql_from_text(text);
        assert_eq!(sql, "SELECT * FROM users;");
    }

    #[test]
    fn test_extract_sql_from_plain_code_block() {
        let text = "```\nSELECT id FROM orders\n```";
        let sql = extract_sql_from_text(text);
        assert_eq!(sql, "SELECT id FROM orders");
    }

    #[test]
    fn test_extract_sql_no_code_block() {
        let text = "SELECT * FROM users WHERE id = 1";
        let sql = extract_sql_from_text(text);
        assert_eq!(sql, "SELECT * FROM users WHERE id = 1");
    }

    #[test]
    fn test_extract_sql_trims_whitespace() {
        let text = "   SELECT 1   ";
        let sql = extract_sql_from_text(text);
        assert_eq!(sql, "SELECT 1");
    }

    #[test]
    fn test_write_sql_blocked_in_agent_context() {
        // is_write_sql のロジックを経由してブロックされることを確認する
        assert!(is_write_sql("INSERT INTO t VALUES (1)"));
        assert!(is_write_sql("DELETE FROM t"));
        assert!(is_write_sql("DROP TABLE t"));
        assert!(!is_write_sql("SELECT * FROM t"));
        assert!(!is_write_sql("SELECT schema_name FROM information_schema.schemata"));
    }
}
