//! 多言語化(i18n)モジュール
//!
//! グローバルな言語状態と各種メッセージカタログを提供する。
//! 外部クレートなしで自前実装し、OnceLockによるスレッドセーフな初期化を行う。

use std::sync::OnceLock;

/// サポートする言語
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// 英語（デフォルト）
    #[default]
    En,
    /// 日本語
    Ja,
}

impl std::str::FromStr for Lang {
    type Err = String;

    /// 文字列から言語を解析する
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "en" => Ok(Lang::En),
            "ja" => Ok(Lang::Ja),
            other => Err(format!("Unsupported language: '{}'. Supported: en, ja", other)),
        }
    }
}

/// グローバル言語状態
/// OnceLockにより一度だけ設定でき、スレッドセーフに参照できる
static LANG: OnceLock<Lang> = OnceLock::new();

/// グローバル言語を設定する
///
/// 既に設定済みの場合は無視する（最初の呼び出しのみ有効）
pub fn set_lang(lang: Lang) {
    let _ = LANG.set(lang);
}

/// 現在のグローバル言語を取得する
///
/// 未設定の場合は Lang::En を返す
pub fn get_lang() -> Lang {
    LANG.get().copied().unwrap_or_default()
}

/// 翻訳マクロ
///
/// `$msg.translate(get_lang())` に展開する。
/// 使用例: `t!(ConfigMsg::NotFound { path: "..." })`
#[macro_export]
macro_rules! t {
    ($msg:expr) => {
        $msg.translate($crate::i18n::get_lang())
    };
}

// ============================================================
// タスク 5.2: ConfigMsg enum
// ============================================================

/// 設定関連メッセージカタログ
pub enum ConfigMsg<'a> {
    /// 設定ファイルが見つからない
    NotFound { path: &'a str },
    /// TOML解析失敗
    ParseFailed { detail: &'a str },
    /// 接続先未定義
    NoConnections,
    /// フィールドが空
    FieldEmpty { field: &'a str },
    /// ポートが0
    InvalidPort { field: &'a str },
    /// ファイル読み込み失敗
    FileReadFailed { detail: &'a str },
    /// 権限警告
    PermissionWarning { mode: u32, path: &'a str },
}

impl<'a> ConfigMsg<'a> {
    /// 英語メッセージを返す
    pub fn en(&self) -> String {
        match self {
            ConfigMsg::NotFound { path } => format!(
                "Config file not found: {}\n\n\
                 Create the file:\n\
                 mkdir -p ~/.config/poshgre\n\
                 vi ~/.config/poshgre/config.toml\n\n\
                 Example:\n\
                 [[connections]]\n\
                 name = \"local\"\n\n\
                 [connections.postgres]\n\
                 host = \"127.0.0.1\"\n\
                 port = 5432\n\
                 database = \"mydb\"\n\
                 user = \"postgres\"\n\
                 password = \"postgres\"\n\
                 ssl_mode = \"disabled\"",
                path
            ),
            ConfigMsg::ParseFailed { detail } => {
                format!("Failed to parse config file: {}", detail)
            }
            ConfigMsg::NoConnections => "No connections defined in config".to_string(),
            ConfigMsg::FieldEmpty { field } => format!("{} is empty", field),
            ConfigMsg::InvalidPort { field } => {
                format!("{} is 0. Please specify a valid port number.", field)
            }
            ConfigMsg::FileReadFailed { detail } => format!("Failed to read file: {}", detail),
            ConfigMsg::PermissionWarning { mode, path } => format!(
                "Config file permission is {:o}. chmod 600 is recommended for security: {}",
                mode, path
            ),
        }
    }

    /// 日本語メッセージを返す
    pub fn ja(&self) -> String {
        match self {
            ConfigMsg::NotFound { path } => format!(
                "設定ファイルが見つかりません: {}\n\n\
                 以下のようにファイルを作成してください:\n\
                 mkdir -p ~/.config/poshgre\n\
                 vi ~/.config/poshgre/config.toml\n\n\
                 設定例:\n\
                 [[connections]]\n\
                 name = \"local\"\n\n\
                 [connections.postgres]\n\
                 host = \"127.0.0.1\"\n\
                 port = 5432\n\
                 database = \"mydb\"\n\
                 user = \"postgres\"\n\
                 password = \"postgres\"\n\
                 ssl_mode = \"disabled\"",
                path
            ),
            ConfigMsg::ParseFailed { detail } => {
                format!("設定ファイルの解析に失敗しました: {}", detail)
            }
            ConfigMsg::NoConnections => "接続先が1つも定義されていません".to_string(),
            ConfigMsg::FieldEmpty { field } => format!("{}が空です", field),
            ConfigMsg::InvalidPort { field } => {
                format!("{}が0です。正しいポート番号を指定してください。", field)
            }
            ConfigMsg::FileReadFailed { detail } => {
                format!("ファイルの読み込みに失敗しました: {}", detail)
            }
            ConfigMsg::PermissionWarning { mode, path } => format!(
                "設定ファイルのパーミッションが{:o}です。認証情報を含むため chmod 600 を推奨します: {}",
                mode, path
            ),
        }
    }

    /// 現在の言語でメッセージを返す
    pub fn translate(&self, lang: Lang) -> String {
        match lang {
            Lang::En => self.en(),
            Lang::Ja => self.ja(),
        }
    }
}

// ============================================================
// タスク 5.3: ErrorMsg enum
// ============================================================

/// エラー関連メッセージカタログ
pub enum ErrorMsg<'a> {
    Config { detail: &'a str },
    ConfigLoad { detail: &'a str },
    ConfigPermission { path: &'a str },
    Connection { detail: &'a str },
    DatabaseConnection { detail: &'a str },
    Query { detail: &'a str },
    QueryExecution { detail: &'a str },
    QueryTimeout,
    Tui { detail: &'a str },
    Io { detail: &'a str },
    ClaudeApi { detail: &'a str },
    Other { detail: &'a str },
}

impl<'a> ErrorMsg<'a> {
    /// 英語メッセージを返す
    pub fn en(&self) -> String {
        match self {
            ErrorMsg::Config { detail } => format!("Config error: {}", detail),
            ErrorMsg::ConfigLoad { detail } => format!("Failed to load config: {}", detail),
            ErrorMsg::ConfigPermission { path } => {
                format!("Invalid config file permission (recommended: 600): {}", path)
            }
            ErrorMsg::Connection { detail } => format!("Connection error: {}", detail),
            ErrorMsg::DatabaseConnection { detail } => {
                format!("Database connection failed: {}", detail)
            }
            ErrorMsg::Query { detail } => format!("SQL execution failed: {}", detail),
            ErrorMsg::QueryExecution { detail } => {
                format!("SQL query execution failed: {}", detail)
            }
            ErrorMsg::QueryTimeout => "Query execution timed out".to_string(),
            ErrorMsg::Tui { detail } => format!("TUI error: {}", detail),
            ErrorMsg::Io { detail } => format!("I/O error: {}", detail),
            ErrorMsg::ClaudeApi { detail } => format!("Claude API error: {}", detail),
            ErrorMsg::Other { detail } => detail.to_string(),
        }
    }

    /// 日本語メッセージを返す
    pub fn ja(&self) -> String {
        match self {
            ErrorMsg::Config { detail } => format!("設定エラー: {}", detail),
            ErrorMsg::ConfigLoad { detail } => {
                format!("設定ファイルの読み込みに失敗しました: {}", detail)
            }
            ErrorMsg::ConfigPermission { path } => {
                format!("設定ファイルの権限が不正です（推奨: 600）: {}", path)
            }
            ErrorMsg::Connection { detail } => format!("接続エラー: {}", detail),
            ErrorMsg::DatabaseConnection { detail } => {
                format!("データベースへの接続に失敗しました: {}", detail)
            }
            ErrorMsg::Query { detail } => format!("SQLの実行に失敗しました: {}", detail),
            ErrorMsg::QueryExecution { detail } => {
                format!("SQLクエリの実行に失敗しました: {}", detail)
            }
            ErrorMsg::QueryTimeout => "クエリの実行がタイムアウトしました".to_string(),
            ErrorMsg::Tui { detail } => format!("TUIエラー: {}", detail),
            ErrorMsg::Io { detail } => format!("I/Oエラー: {}", detail),
            ErrorMsg::ClaudeApi { detail } => format!("Claude API エラー: {}", detail),
            ErrorMsg::Other { detail } => detail.to_string(),
        }
    }

    /// 現在の言語でメッセージを返す
    pub fn translate(&self, lang: Lang) -> String {
        match lang {
            Lang::En => self.en(),
            Lang::Ja => self.ja(),
        }
    }
}

// ============================================================
// タスク 5.4: TuiMsg enum
// ============================================================

/// TUI表示テキストメッセージカタログ
pub enum TuiMsg<'a> {
    // 接続先選択画面
    SelectingTitle,
    SelectingHelp,
    // SQL入力画面
    SqlInputTitle,
    SqlInputReadonlyLabel,
    SqlInputTitleSuffix,
    SqlInputHint,
    // 接続情報
    ConnectionInfo,
    ConnectionTarget,
    Host,
    Database,
    SelectedDatabase,
    // ヘルプ
    ConnectedHelp,
    QueryHelp,
    // 接続中
    ConnectingTitle,
    ConnectingMessage { connection_name: &'a str },
    // クエリ実行中
    ExecutingQueryTitle,
    StatusTitle,
    ExecutingMessage,
    ColumnSelecting,
    // 選択レコード
    SelectedRecordTitle,
    // エラー画面
    ErrorTitle,
    ErrorHelp,
    // skim
    QueryResultPrompt,
    SkimInitError,
    // selector.rs 関連
    SelectConnectionPrompt,
    // カラム選択関連
    ColumnSelectPrompt { table: &'a str },
    NoColumnsFound { table: &'a str },
    // Shell入力エリア
    ShellInputTitleFocused,
    // PROMPT入力エリア（Claude AI連携）
    PromptInputTitle,
    PromptInputTitleFocused,
    PromptProcessing,
    /// スピナーと組み合わせて使うベースメッセージ（スピナー文字は render 側で付与）
    PromptProcessingBase,
    PromptApiKeyNotSet,
    // 動的メッセージ（引数付き）
    QueryFailed { detail: &'a str },
    QueryCancelled { query: &'a str },
    QueryTaskFailed { detail: &'a str },
    ReadonlyBlocked,
}

impl<'a> TuiMsg<'a> {
    /// 英語メッセージを返す
    pub fn en(&self) -> String {
        match self {
            TuiMsg::SelectingTitle => "Select Connection".to_string(),
            TuiMsg::SelectingHelp => {
                "Enter: select | q: quit".to_string()
            }
            TuiMsg::SqlInputTitle => "SQL Input".to_string(),
            TuiMsg::SqlInputReadonlyLabel => "[READONLY]".to_string(),
            TuiMsg::SqlInputTitleSuffix => {
                "(Enter: run, Ctrl+J: newline, Ctrl+T: tables)".to_string()
            }
            TuiMsg::SqlInputHint => "Enter SQL and press Enter to execute.".to_string(),
            TuiMsg::ConnectionInfo => "Connection Info".to_string(),
            TuiMsg::ConnectionTarget => "Target".to_string(),
            TuiMsg::Host => "Host".to_string(),
            TuiMsg::Database => "Database".to_string(),
            TuiMsg::SelectedDatabase => "Selected Database".to_string(),
            TuiMsg::ConnectedHelp => {
                "Tab: switch focus | Enter: run | Ctrl+J: newline | Ctrl+T: tables | Ctrl+S: select columns | q: quit | ss/st/sc: alias"
                    .to_string()
            }
            TuiMsg::QueryHelp => "Enter: run query | q: quit".to_string(),
            TuiMsg::ConnectingTitle => "Connecting".to_string(),
            TuiMsg::ConnectingMessage { connection_name } => {
                format!("Connecting to '{}'...", connection_name)
            }
            TuiMsg::ExecutingQueryTitle => "Executing Query".to_string(),
            TuiMsg::StatusTitle => "Status".to_string(),
            TuiMsg::ExecutingMessage => "Executing query...".to_string(),
            TuiMsg::ColumnSelecting => "Selecting columns...".to_string(),
            TuiMsg::SelectedRecordTitle => "Selected Record".to_string(),
            TuiMsg::ErrorTitle => "Error".to_string(),
            TuiMsg::ErrorHelp => "Enter/ESC/q: close".to_string(),
            TuiMsg::QueryResultPrompt => "Result > ".to_string(),
            TuiMsg::SkimInitError => "skim initialization error".to_string(),
            TuiMsg::QueryFailed { detail } => {
                format!("Query execution failed: {}", detail)
            }
            TuiMsg::QueryCancelled { query } => {
                format!("Query execution was cancelled: {}", query)
            }
            TuiMsg::QueryTaskFailed { detail } => {
                format!("Query execution task crashed: {}", detail)
            }
            TuiMsg::SelectConnectionPrompt => "Select connection > ".to_string(),
            TuiMsg::ColumnSelectPrompt { table } => {
                format!(
                    "{}: select columns (Tab: multi-select, Enter: confirm) > ",
                    table
                )
            }
            TuiMsg::NoColumnsFound { table } => {
                format!("No columns found for table '{}'", table)
            }
            TuiMsg::ReadonlyBlocked => {
                "Write operations are not allowed in readonly mode.".to_string()
            }
            TuiMsg::ShellInputTitleFocused => "Shell Input (Enter: run, Ctrl+J: newline)".to_string(),
            TuiMsg::PromptInputTitle => "AI Prompt".to_string(),
            TuiMsg::PromptInputTitleFocused => "AI Prompt (focused)".to_string(),
            TuiMsg::PromptProcessing => "Generating SQL...".to_string(),
            TuiMsg::PromptProcessingBase => "Generating SQL".to_string(),
            TuiMsg::PromptApiKeyNotSet => "Set ANTHROPIC_API_KEY to use AI prompt".to_string(),
        }
    }

    /// 日本語メッセージを返す
    pub fn ja(&self) -> String {
        match self {
            TuiMsg::SelectingTitle => "接続先選択".to_string(),
            TuiMsg::SelectingHelp => "Enter: 選択 | q: 終了".to_string(),
            TuiMsg::SqlInputTitle => "SQL入力".to_string(),
            TuiMsg::SqlInputReadonlyLabel => "[READONLY]".to_string(),
            TuiMsg::SqlInputTitleSuffix => {
                "(Enter: 実行, Ctrl+J: 改行, Ctrl+T: テーブル一覧)".to_string()
            }
            TuiMsg::SqlInputHint => "SQLを入力してEnterで実行してください。".to_string(),
            TuiMsg::ConnectionInfo => "接続情報".to_string(),
            TuiMsg::ConnectionTarget => "接続先".to_string(),
            TuiMsg::Host => "ホスト".to_string(),
            TuiMsg::Database => "データベース".to_string(),
            TuiMsg::SelectedDatabase => "選択データベース".to_string(),
            TuiMsg::ConnectedHelp => {
                "Tab: フォーカス切替 | Enter: 実行 | Ctrl+J: 改行 | Ctrl+T: テーブル一覧 | Ctrl+S: カラム選択 | q: 終了 | ss/st/sc: エイリアス"
                    .to_string()
            }
            TuiMsg::QueryHelp => "Enter: クエリ実行 | q: 終了".to_string(),
            TuiMsg::ConnectingTitle => "接続中".to_string(),
            TuiMsg::ConnectingMessage { connection_name } => {
                format!("'{}' に接続しています...", connection_name)
            }
            TuiMsg::ExecutingQueryTitle => "実行中のクエリ".to_string(),
            TuiMsg::StatusTitle => "ステータス".to_string(),
            TuiMsg::ExecutingMessage => "クエリを実行しています...".to_string(),
            TuiMsg::ColumnSelecting => "カラム選択中...".to_string(),
            TuiMsg::SelectedRecordTitle => "選択レコード".to_string(),
            TuiMsg::ErrorTitle => "エラー".to_string(),
            TuiMsg::ErrorHelp => "Enter/ESC/q: 閉じる".to_string(),
            TuiMsg::QueryResultPrompt => "結果 > ".to_string(),
            TuiMsg::SkimInitError => "skim初期化エラー".to_string(),
            TuiMsg::QueryFailed { detail } => {
                format!("クエリ実行に失敗しました: {}", detail)
            }
            TuiMsg::QueryCancelled { query } => {
                format!("クエリ実行がキャンセルされました: {}", query)
            }
            TuiMsg::QueryTaskFailed { detail } => {
                format!("クエリ実行タスクが異常終了しました: {}", detail)
            }
            TuiMsg::SelectConnectionPrompt => "接続先を選択してください > ".to_string(),
            TuiMsg::ColumnSelectPrompt { table } => {
                format!("{} のカラムを選択 (Tab: 複数選択, Enter: 確定) > ", table)
            }
            TuiMsg::NoColumnsFound { table } => {
                format!("テーブル '{}' のカラムが見つかりません", table)
            }
            TuiMsg::ReadonlyBlocked => {
                "readonlyモードのため、書き込み操作は実行できません。".to_string()
            }
            TuiMsg::ShellInputTitleFocused => "SHELL入力 (Enter: 実行, Ctrl+J: 改行)".to_string(),
            TuiMsg::PromptInputTitle => "AIプロンプト".to_string(),
            TuiMsg::PromptInputTitleFocused => "AIプロンプト (入力中)".to_string(),
            TuiMsg::PromptProcessing => "SQL生成中...".to_string(),
            TuiMsg::PromptProcessingBase => "SQL生成中".to_string(),
            TuiMsg::PromptApiKeyNotSet => "ANTHROPIC_API_KEY を設定するとAIプロンプトが使えます".to_string(),
        }
    }

    /// 現在の言語でメッセージを返す
    pub fn translate(&self, lang: Lang) -> String {
        match lang {
            Lang::En => self.en(),
            Lang::Ja => self.ja(),
        }
    }
}

// ============================================================
// タスク 5.5: ConnectionMsg enum
// ============================================================

/// 接続関連メッセージカタログ
pub enum ConnectionMsg<'a> {
    /// readonlyモード設定失敗
    ReadonlySetFailed { detail: &'a str },
    /// 接続失敗（原因不明）
    ConnectionFailed,
    /// SSH鍵/agentの両認証失敗
    SshAuthFailed { key_err: &'a str, agent_err: &'a str },
    /// SSH agent認証失敗
    SshAgentAuthFailed { detail: &'a str },
    /// SSH認証エラー（一般）
    SshAuthError,
}

impl<'a> ConnectionMsg<'a> {
    /// 英語メッセージを返す
    pub fn en(&self) -> String {
        match self {
            ConnectionMsg::ReadonlySetFailed { detail } => {
                format!("Failed to set readonly mode: {}", detail)
            }
            ConnectionMsg::ConnectionFailed => "Connection failed (unknown cause)".to_string(),
            ConnectionMsg::SshAuthFailed { key_err, agent_err } => format!(
                "SSH authentication failed. Key authentication error: {}. SSH agent authentication error: {}",
                key_err, agent_err
            ),
            ConnectionMsg::SshAgentAuthFailed { detail } => format!(
                "SSH agent authentication failed: {}. Please set key_path or register your key in the SSH agent (e.g., 1Password).",
                detail
            ),
            ConnectionMsg::SshAuthError => {
                "SSH authentication failed. Please check your credentials.".to_string()
            }
        }
    }

    /// 日本語メッセージを返す
    pub fn ja(&self) -> String {
        match self {
            ConnectionMsg::ReadonlySetFailed { detail } => {
                format!("readonlyモード設定に失敗: {}", detail)
            }
            ConnectionMsg::ConnectionFailed => "接続に失敗しました（原因不明）".to_string(),
            ConnectionMsg::SshAuthFailed { key_err, agent_err } => format!(
                "SSH認証に失敗しました。鍵ファイル認証エラー: {}。SSH agent認証エラー: {}",
                key_err, agent_err
            ),
            ConnectionMsg::SshAgentAuthFailed { detail } => format!(
                "SSH agent認証に失敗しました: {}。key_pathを設定するか、利用中のSSH agent（例: 1Password）に鍵を登録してください。",
                detail
            ),
            ConnectionMsg::SshAuthError => {
                "SSH認証に失敗しました。認証情報を確認してください。".to_string()
            }
        }
    }

    /// 現在の言語でメッセージを返す
    pub fn translate(&self, lang: Lang) -> String {
        match lang {
            Lang::En => self.en(),
            Lang::Ja => self.ja(),
        }
    }
}

// ============================================================
// タスク 5.12: ユニットテスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    // FromStr テスト
    #[test]
    fn test_lang_from_str_en() {
        assert_eq!("en".parse::<Lang>(), Ok(Lang::En));
    }

    #[test]
    fn test_lang_from_str_ja() {
        assert_eq!("ja".parse::<Lang>(), Ok(Lang::Ja));
    }

    #[test]
    fn test_lang_from_str_unknown() {
        assert!("fr".parse::<Lang>().is_err());
        assert!("".parse::<Lang>().is_err());
        assert!("EN".parse::<Lang>().is_err());
    }

    #[test]
    fn test_lang_default_is_en() {
        assert_eq!(Lang::default(), Lang::En);
    }

    // get_lang() 未設定テスト
    // OnceLockはプロセス内でリセットできないため、未設定のデフォルト動作はデフォルト値から確認する
    #[test]
    fn test_get_lang_default_is_en() {
        // 未設定の場合はデフォルトのEnが返ることをLang::defaultから確認する
        // (OnceLockは一度設定するとリセットできないため、直接get_lang()を呼ぶとテスト順序依存になる)
        assert_eq!(Lang::default(), Lang::En);
    }

    // ConfigMsg テスト
    #[test]
    fn test_config_msg_not_found_en() {
        let msg = ConfigMsg::NotFound { path: "/path/to/config.toml" };
        let text = msg.translate(Lang::En);
        assert!(text.contains("Config file not found"));
        assert!(text.contains("/path/to/config.toml"));
    }

    #[test]
    fn test_config_msg_not_found_ja() {
        let msg = ConfigMsg::NotFound { path: "/path/to/config.toml" };
        let text = msg.translate(Lang::Ja);
        assert!(text.contains("設定ファイルが見つかりません"));
        assert!(text.contains("/path/to/config.toml"));
    }

    #[test]
    fn test_config_msg_parse_failed_en() {
        let msg = ConfigMsg::ParseFailed { detail: "invalid toml" };
        let text = msg.translate(Lang::En);
        assert!(text.contains("Failed to parse config file"));
        assert!(text.contains("invalid toml"));
    }

    #[test]
    fn test_config_msg_parse_failed_ja() {
        let msg = ConfigMsg::ParseFailed { detail: "invalid toml" };
        let text = msg.translate(Lang::Ja);
        assert!(text.contains("設定ファイルの解析に失敗しました"));
    }

    #[test]
    fn test_config_msg_no_connections_en() {
        let msg = ConfigMsg::NoConnections;
        assert_eq!(msg.translate(Lang::En), "No connections defined in config");
    }

    #[test]
    fn test_config_msg_no_connections_ja() {
        let msg = ConfigMsg::NoConnections;
        assert_eq!(msg.translate(Lang::Ja), "接続先が1つも定義されていません");
    }

    #[test]
    fn test_config_msg_field_empty_en() {
        let msg = ConfigMsg::FieldEmpty { field: "host" };
        let text = msg.translate(Lang::En);
        assert!(text.contains("host"));
        assert!(text.contains("is empty"));
    }

    #[test]
    fn test_config_msg_field_empty_ja() {
        let msg = ConfigMsg::FieldEmpty { field: "ホスト" };
        let text = msg.translate(Lang::Ja);
        assert!(text.contains("ホスト"));
        assert!(text.contains("が空です"));
    }

    #[test]
    fn test_config_msg_invalid_port_en() {
        let msg = ConfigMsg::InvalidPort { field: "port" };
        let text = msg.translate(Lang::En);
        assert!(text.contains("port"));
        assert!(text.contains("is 0"));
    }

    #[test]
    fn test_config_msg_invalid_port_ja() {
        let msg = ConfigMsg::InvalidPort { field: "ポート" };
        let text = msg.translate(Lang::Ja);
        assert!(text.contains("ポート"));
        assert!(text.contains("が0です"));
    }

    #[test]
    fn test_config_msg_permission_warning_en() {
        let msg = ConfigMsg::PermissionWarning { mode: 0o644, path: "/path/config.toml" };
        let text = msg.translate(Lang::En);
        assert!(text.contains("644"));
        assert!(text.contains("chmod 600"));
        assert!(text.contains("/path/config.toml"));
    }

    #[test]
    fn test_config_msg_permission_warning_ja() {
        let msg = ConfigMsg::PermissionWarning { mode: 0o644, path: "/path/config.toml" };
        let text = msg.translate(Lang::Ja);
        assert!(text.contains("644"));
        assert!(text.contains("chmod 600"));
    }

    // ErrorMsg テスト
    #[test]
    fn test_error_msg_config_en() {
        let msg = ErrorMsg::Config { detail: "invalid" };
        assert_eq!(msg.translate(Lang::En), "Config error: invalid");
    }

    #[test]
    fn test_error_msg_config_ja() {
        let msg = ErrorMsg::Config { detail: "invalid" };
        assert_eq!(msg.translate(Lang::Ja), "設定エラー: invalid");
    }

    #[test]
    fn test_error_msg_query_timeout_en() {
        let msg = ErrorMsg::QueryTimeout;
        assert_eq!(msg.translate(Lang::En), "Query execution timed out");
    }

    #[test]
    fn test_error_msg_query_timeout_ja() {
        let msg = ErrorMsg::QueryTimeout;
        assert_eq!(msg.translate(Lang::Ja), "クエリの実行がタイムアウトしました");
    }

    // TuiMsg テスト
    #[test]
    fn test_tui_msg_selecting_title_en() {
        let msg = TuiMsg::SelectingTitle;
        let text = msg.translate(Lang::En);
        assert!(text.contains("Select Connection"));
    }

    #[test]
    fn test_tui_msg_selecting_title_ja() {
        let msg = TuiMsg::SelectingTitle;
        let text = msg.translate(Lang::Ja);
        assert!(text.contains("接続先選択"));
    }

    #[test]
    fn test_tui_msg_error_title_en() {
        let msg = TuiMsg::ErrorTitle;
        assert_eq!(msg.translate(Lang::En), "Error");
    }

    #[test]
    fn test_tui_msg_error_title_ja() {
        let msg = TuiMsg::ErrorTitle;
        assert_eq!(msg.translate(Lang::Ja), "エラー");
    }

    #[test]
    fn test_tui_msg_query_failed_en() {
        let msg = TuiMsg::QueryFailed { detail: "syntax error" };
        let text = msg.translate(Lang::En);
        assert!(text.contains("Query execution failed"));
        assert!(text.contains("syntax error"));
    }

    #[test]
    fn test_tui_msg_query_failed_ja() {
        let msg = TuiMsg::QueryFailed { detail: "syntax error" };
        let text = msg.translate(Lang::Ja);
        assert!(text.contains("クエリ実行に失敗しました"));
        assert!(text.contains("syntax error"));
    }

    #[test]
    fn test_tui_msg_readonly_blocked_en() {
        let msg = TuiMsg::ReadonlyBlocked;
        let text = msg.translate(Lang::En);
        assert!(text.contains("readonly mode"));
    }

    #[test]
    fn test_tui_msg_readonly_blocked_ja() {
        let msg = TuiMsg::ReadonlyBlocked;
        let text = msg.translate(Lang::Ja);
        assert!(text.contains("readonlyモード"));
    }

    // ConnectionMsg テスト
    #[test]
    fn test_connection_msg_readonly_set_failed_en() {
        let msg = ConnectionMsg::ReadonlySetFailed { detail: "permission denied" };
        let text = msg.translate(Lang::En);
        assert!(text.contains("Failed to set readonly mode"));
        assert!(text.contains("permission denied"));
    }

    #[test]
    fn test_connection_msg_readonly_set_failed_ja() {
        let msg = ConnectionMsg::ReadonlySetFailed { detail: "permission denied" };
        let text = msg.translate(Lang::Ja);
        assert!(text.contains("readonlyモード設定に失敗"));
    }

    #[test]
    fn test_connection_msg_connection_failed_en() {
        let msg = ConnectionMsg::ConnectionFailed;
        assert_eq!(msg.translate(Lang::En), "Connection failed (unknown cause)");
    }

    #[test]
    fn test_connection_msg_connection_failed_ja() {
        let msg = ConnectionMsg::ConnectionFailed;
        assert_eq!(msg.translate(Lang::Ja), "接続に失敗しました（原因不明）");
    }

    #[test]
    fn test_connection_msg_ssh_auth_error_en() {
        let msg = ConnectionMsg::SshAuthError;
        let text = msg.translate(Lang::En);
        assert!(text.contains("SSH authentication failed"));
    }

    #[test]
    fn test_connection_msg_ssh_auth_error_ja() {
        let msg = ConnectionMsg::SshAuthError;
        let text = msg.translate(Lang::Ja);
        assert!(text.contains("SSH認証に失敗しました"));
    }
}
