use thiserror::Error;

use crate::i18n::{get_lang, ErrorMsg};

/// poshgreアプリケーションのエラー型
///
/// #[error(...)] の文字列はログ/デバッグ用の英語表記。
/// ユーザー向けには user_message() を使う。
#[derive(Debug, Error)]
pub enum Error {
    /// 設定関連エラー
    #[error("Config error: {0}")]
    Config(String),

    /// 設定ファイルの読み込みエラー
    #[error("Failed to load config: {0}")]
    ConfigLoad(String),

    /// 設定ファイルの権限エラー
    #[error("Invalid config file permission (recommended: 600): {0}")]
    ConfigPermission(String),

    /// 接続関連エラー
    #[error("Connection error: {0}")]
    Connection(String),

    /// データベース接続失敗
    #[error("Database connection failed: {0}")]
    DatabaseConnection(#[source] sqlx::Error),

    /// クエリ実行エラー
    #[error("SQL execution failed: {0}")]
    Query(String),

    /// クエリ実行失敗
    #[error("SQL query execution failed: {0}")]
    QueryExecution(#[source] sqlx::Error),

    /// クエリタイムアウト
    #[error("Query execution timed out")]
    QueryTimeout,

    /// TUI関連エラー
    #[error("TUI error: {0}")]
    Tui(String),

    /// 入力/出力エラー
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Claude API 呼び出しエラー
    #[error("Claude API error: {0}")]
    ClaudeApi(String),

    /// その他のエラー
    #[error("{0}")]
    Other(String),
}

/// Result型のエイリアス
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// ユーザー向けのローカライズされたエラーメッセージを返す
    ///
    /// to_string() はログ用英語メッセージを返す。
    /// UIに表示する場合はこのメソッドを使う。
    pub fn user_message(&self) -> String {
        let lang = get_lang();
        match self {
            Error::Config(detail) => ErrorMsg::Config { detail }.translate(lang),
            Error::ConfigLoad(detail) => ErrorMsg::ConfigLoad { detail }.translate(lang),
            Error::ConfigPermission(path) => ErrorMsg::ConfigPermission { path }.translate(lang),
            Error::Connection(detail) => ErrorMsg::Connection { detail }.translate(lang),
            Error::DatabaseConnection(err) => {
                let detail = err.to_string();
                ErrorMsg::DatabaseConnection { detail: &detail }.translate(lang)
            }
            Error::Query(detail) => ErrorMsg::Query { detail }.translate(lang),
            Error::QueryExecution(err) => {
                let detail = err.to_string();
                ErrorMsg::QueryExecution { detail: &detail }.translate(lang)
            }
            Error::QueryTimeout => ErrorMsg::QueryTimeout.translate(lang),
            Error::Tui(detail) => ErrorMsg::Tui { detail }.translate(lang),
            Error::Io(err) => {
                let detail = err.to_string();
                ErrorMsg::Io { detail: &detail }.translate(lang)
            }
            Error::ClaudeApi(detail) => ErrorMsg::ClaudeApi { detail }.translate(lang),
            Error::Other(detail) => ErrorMsg::Other { detail }.translate(lang),
        }
    }

    /// 接続エラーを生成（簡易版）
    pub fn connection<T: std::fmt::Display>(msg: T) -> Self {
        Error::Connection(msg.to_string())
    }

    /// 接続エラーを生成（コンテキスト付き）
    pub fn connection_context<T: std::fmt::Display>(operation: &str, error: T) -> Self {
        Error::Connection(format!("{}: {}", operation, error))
    }

    /// 設定エラーを生成（簡易版）
    pub fn config<T: std::fmt::Display>(msg: T) -> Self {
        Error::Config(msg.to_string())
    }

    /// データベース接続エラーの詳細メッセージを生成
    pub fn database_connection_detail(err: sqlx::Error) -> Self {
        // エラーの種類に応じて詳細なログを出力
        match &err {
            sqlx::Error::Configuration(_) => {
                tracing::error!(
                    "接続設定が不正です。ホスト名、ポート番号、認証情報を確認してください。"
                );
            }
            sqlx::Error::Tls(_) => {
                tracing::error!(
                    "SSL/TLS接続に失敗しました。証明書の検証に失敗したか、サーバーがSSL/TLSをサポートしていない可能性があります。\n\
                    ssl_mode設定を「preferred」または「disabled」に変更することで回避できますが、セキュリティリスクがあります。"
                );
            }
            sqlx::Error::Io(_) => {
                tracing::error!(
                    "ネットワーク接続に失敗しました。ホスト名、ポート番号、ネットワーク設定を確認してください。"
                );
            }
            sqlx::Error::PoolTimedOut => {
                tracing::error!(
                    "接続タイムアウトしました。データベースサーバーが応答していないか、ネットワークが遅延しています。"
                );
            }
            _ => {
                tracing::error!("予期しないエラーが発生しました。ログを確認してください。");
            }
        };

        tracing::error!("Database connection error details: {:?}", err);
        Error::DatabaseConnection(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_config() {
        let err = Error::Config("invalid config".to_string());
        // #[error(...)] はログ用英語メッセージ
        assert_eq!(err.to_string(), "Config error: invalid config");
    }

    #[test]
    fn test_error_display_config_load() {
        let err = Error::ConfigLoad("file not found".to_string());
        assert_eq!(err.to_string(), "Failed to load config: file not found");
    }

    #[test]
    fn test_error_display_config_permission() {
        let err = Error::ConfigPermission("644".to_string());
        assert_eq!(
            err.to_string(),
            "Invalid config file permission (recommended: 600): 644"
        );
    }

    #[test]
    fn test_error_display_connection() {
        let err = Error::Connection("connection failed".to_string());
        assert_eq!(err.to_string(), "Connection error: connection failed");
    }

    #[test]
    fn test_error_display_query() {
        let err = Error::Query("syntax error".to_string());
        assert_eq!(err.to_string(), "SQL execution failed: syntax error");
    }

    #[test]
    fn test_error_display_query_timeout() {
        let err = Error::QueryTimeout;
        assert_eq!(err.to_string(), "Query execution timed out");
    }

    #[test]
    fn test_error_display_tui() {
        let err = Error::Tui("render failed".to_string());
        assert_eq!(err.to_string(), "TUI error: render failed");
    }

    #[test]
    fn test_error_display_other() {
        let err = Error::Other("unknown error".to_string());
        assert_eq!(err.to_string(), "unknown error");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(err.to_string().contains("I/O error"));
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_result_type_alias() {
        // Result型のエイリアスが正しく動作することを確認
        fn returns_result() -> Result<i32> {
            Ok(42)
        }

        fn returns_error() -> Result<i32> {
            Err(Error::Other("test error".to_string()))
        }

        assert_eq!(returns_result().unwrap(), 42);
        assert!(returns_error().is_err());
    }

    #[test]
    fn test_error_debug_format() {
        let err = Error::Config("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Config"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_connection_helper() {
        let err = Error::connection("test error");
        assert_eq!(err.to_string(), "Connection error: test error");
    }

    #[test]
    fn test_connection_context_helper() {
        let err = Error::connection_context("SSH", "timeout");
        assert_eq!(err.to_string(), "Connection error: SSH: timeout");
    }

    #[test]
    fn test_config_helper() {
        let err = Error::config("invalid value");
        assert_eq!(err.to_string(), "Config error: invalid value");
    }

    #[test]
    fn test_error_display_claude_api() {
        let err = Error::ClaudeApi("request failed".to_string());
        assert_eq!(err.to_string(), "Claude API error: request failed");
    }

    #[test]
    fn test_user_message_returns_localized() {
        use crate::i18n::{ErrorMsg, Lang};
        // user_message()はErrorMsg経由で翻訳されることを確認（デフォルト英語）
        let msg = ErrorMsg::QueryTimeout.translate(Lang::En);
        assert_eq!(msg, "Query execution timed out");
        let msg_ja = ErrorMsg::QueryTimeout.translate(Lang::Ja);
        assert_eq!(msg_ja, "クエリの実行がタイムアウトしました");
    }
}
