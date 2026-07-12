use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroize;

use crate::error::{Error, Result};
use crate::i18n::ConfigMsg;
use crate::t;

/// アプリケーション全体の設定（言語設定など）
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct AppSettings {
    /// 表示言語（"en" / "ja"）。未指定時はデフォルト（"en"）
    pub language: Option<String>,

    /// Anthropic API キー（Claude API 使用時に必要）
    /// 未指定時は環境変数 ANTHROPIC_API_KEY から取得する
    #[serde(default)]
    pub anthropic_api_key: Option<Password>,

    /// 使用する Claude モデルID（未指定時は claude-3-5-haiku-20241022 を使用）
    /// 未指定時は環境変数 CLAUDE_MODEL から取得する
    #[serde(default)]
    pub claude_model: Option<String>,

    /// TUIレイアウト設定（入力エリアの高さ、プレビューペインの位置・サイズなど）
    /// [settings.layout] セクションが存在しない場合は LayoutSettings::default() が使われる
    #[serde(default)]
    pub layout: LayoutSettings,
}

/// TUIレイアウト設定
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct LayoutSettings {
    /// SQL入力エリアの高さ（行数、デフォルト: 5）
    #[serde(default = "default_input_area_height")]
    pub sql_input_height: u16,

    /// Shell入力エリアの高さ（行数、デフォルト: 5）
    #[serde(default = "default_input_area_height")]
    pub shell_input_height: u16,

    /// AI Prompt入力エリアの高さ（行数、デフォルト: 5）
    /// APIキー未設定時は参照されない
    #[serde(default = "default_input_area_height")]
    pub prompt_input_height: u16,

    /// 結果ビューアのプレビューペインのサイズ（%、デフォルト: 30）
    #[serde(default = "default_result_preview_width")]
    pub result_preview_width: u16,

    /// 結果ビューアのプレビューペインの位置（デフォルト: Right）
    #[serde(default = "default_result_preview_position")]
    pub result_preview_position: PreviewPosition,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            sql_input_height: default_input_area_height(),
            shell_input_height: default_input_area_height(),
            prompt_input_height: default_input_area_height(),
            result_preview_width: default_result_preview_width(),
            result_preview_position: default_result_preview_position(),
        }
    }
}

/// 結果ビューアのプレビューペインの位置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewPosition {
    Up,
    Down,
    Left,
    Right,
}

impl PreviewPosition {
    pub fn as_str(&self) -> &'static str {
        match self {
            PreviewPosition::Up => "up",
            PreviewPosition::Down => "down",
            PreviewPosition::Left => "left",
            PreviewPosition::Right => "right",
        }
    }
}

/// アプリケーション設定
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// デフォルトのbastion設定（個別の接続設定で上書き可能）
    pub default_bastion: Option<BastionConfig>,

    /// デフォルトのPostgreSQL接続プール設定（個別の接続設定で上書き可能、フィールド単位で指定可能）
    pub default_postgres_pool: Option<PoolConfigPartial>,

    pub connections: Vec<ConnectionConfig>,

    /// アプリケーション設定（言語など）
    /// [settings] セクションが存在しない場合は AppSettings::default() が使われる
    #[serde(default)]
    pub settings: AppSettings,
}

/// bastion設定の指定方法
///
/// TOML上で `bastion = false`（bool）と `[connections.bastion]`（テーブル）の
/// 両方を受け付けるため、untagged enum で deserialize する。
///
/// TOML上の3パターン:
/// - 省略: default_bastion が適用される（後方互換）
/// - `bastion = false`: bastion を使わない（直接接続）
/// - `bastion = true`: 明示的に default_bastion を使う（省略と同等）
/// - `[connections.bastion]` テーブル: 個別のbastion設定を使用
///
/// untagged の deserialize 順序: Config を先に試行することで、
/// BastionConfig の各フィールドが必須のため bool 値が誤ってパースされることはない
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum BastionSetting {
    /// `[connections.bastion]` テーブル — 個別のbastion設定
    Config(BastionConfig),
    /// `bastion = true` または `bastion = false`
    Toggle(bool),
}

/// 接続先設定
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionConfig {
    pub name: String,

    /// bastion（踏み台）サーバー設定
    ///
    /// None: 省略（default_bastionを適用）
    /// Some(BastionSetting::Toggle(false)): 直接接続（default_bastionをスキップ）
    /// Some(BastionSetting::Toggle(true)): 明示的にdefault_bastionを使用
    /// Some(BastionSetting::Config(...)): 個別のbastion設定を使用
    pub bastion: Option<BastionSetting>,

    /// PostgreSQL設定
    pub postgres: PostgresConfig,

    /// readonlyモード（デフォルト: false）
    /// trueの場合、接続後に SET default_transaction_read_only = ON を実行する
    #[serde(default)]
    pub readonly: bool,
}

/// bastion（踏み台）サーバー設定
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BastionConfig {
    /// ホスト名またはIPアドレス
    pub host: String,

    /// SSHポート（デフォルト: 22）
    #[serde(default = "default_ssh_port")]
    pub port: u16,

    /// SSHユーザー名
    pub user: String,

    /// SSH秘密鍵パス（オプション）
    pub key_path: Option<String>,
}

/// PostgreSQL設定
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PostgresConfig {
    /// ホスト名またはIPアドレス
    pub host: String,

    /// ポート（デフォルト: 5432）
    #[serde(default = "default_postgres_port")]
    pub port: u16,

    /// データベース名
    pub database: String,

    /// ユーザー名
    pub user: String,

    /// パスワード
    /// セキュリティ: 使用後はzeroizeでメモリクリア
    pub password: Password,

    /// タイムアウト設定（秒）
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// SSL/TLS接続モード（デフォルト: require）
    /// 値: "require", "prefer", "disable", "verify-ca", "verify-full"
    #[serde(default = "default_ssl_mode")]
    pub ssl_mode: SslMode,

    /// 接続プール設定（オプショナル、フィールド単位で指定可能）
    #[serde(default)]
    pub pool: PoolConfigPartial,
}

/// 接続プール設定（解決済み・デフォルト値適用済み）
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolConfig {
    /// 最大接続数（デフォルト: 10）
    pub max_connections: u32,

    /// アイドルタイムアウト（秒）（デフォルト: 300秒）
    pub idle_timeout: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: default_max_connections(),
            idle_timeout: default_idle_timeout(),
        }
    }
}

/// 接続プール部分設定（フィールド単位で上書き可能）
///
/// TOML設定ファイルでの指定に使用する。未指定フィールドはデフォルト値またはdefault_postgres_poolの値が使われる。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct PoolConfigPartial {
    /// 最大接続数（未指定時はdefault_postgres_poolまたはデフォルト値を使用）
    pub max_connections: Option<u32>,

    /// アイドルタイムアウト（秒）（未指定時はdefault_postgres_poolまたはデフォルト値を使用）
    pub idle_timeout: Option<u64>,
}

impl PoolConfigPartial {
    /// デフォルト値とマージして解決済み PoolConfig を生成する
    ///
    /// 優先順位: 自身の値 > default_pool の値 > PoolConfig::default() の値
    pub fn resolve(&self, default_pool: Option<&PoolConfigPartial>) -> PoolConfig {
        let default = PoolConfig::default();
        PoolConfig {
            max_connections: self
                .max_connections
                .or_else(|| default_pool.and_then(|d| d.max_connections))
                .unwrap_or(default.max_connections),
            idle_timeout: self
                .idle_timeout
                .or_else(|| default_pool.and_then(|d| d.idle_timeout))
                .unwrap_or(default.idle_timeout),
        }
    }
}

/// PostgreSQL SSL/TLS接続モード
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum SslMode {
    /// SSL/TLS必須（推奨）
    #[serde(rename = "require")]
    Require,
    /// SSL/TLS優先（可能ならSSL、不可なら平文）
    #[serde(rename = "prefer")]
    Prefer,
    /// SSL/TLS無効（非推奨、開発環境のみ）
    #[serde(rename = "disable")]
    Disable,
    /// CA証明書で検証（サーバー証明書のCA確認）
    #[serde(rename = "verify-ca")]
    VerifyCa,
    /// CA証明書とホスト名で完全検証（最も安全）
    #[serde(rename = "verify-full")]
    VerifyFull,
}

/// パスワード型（メモリクリア対応）
#[derive(Clone, Deserialize, Serialize, Zeroize)]
#[zeroize(drop)]
pub struct Password(String);

impl Password {
    pub fn new(s: String) -> Self {
        Password(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Password(***)")
    }
}

impl From<&str> for Password {
    fn from(s: &str) -> Self {
        Password(s.to_string())
    }
}

impl From<String> for Password {
    fn from(s: String) -> Self {
        Password(s)
    }
}

impl Config {
    /// 設定ファイルを読み込む
    pub fn load(path: &str) -> Result<Self> {
        let path = Path::new(path);

        if !path.exists() {
            return Err(Error::ConfigLoad(t!(ConfigMsg::NotFound {
                path: &path.display().to_string()
            })));
        }

        // ファイル権限チェック（Unix系のみ）
        #[cfg(unix)]
        check_file_permissions(path)?;

        let content = std::fs::read_to_string(path).map_err(|e| {
            Error::ConfigLoad(t!(ConfigMsg::FileReadFailed {
                detail: &e.to_string()
            }))
        })?;

        // TOML解析
        let config: Config = toml::from_str(&content).map_err(|e| {
            Error::Config(t!(ConfigMsg::ParseFailed {
                detail: &e.to_string()
            }))
        })?;

        // バリデーション
        config.validate()?;

        // 環境変数フォールバック:
        // anthropic_api_key が未設定なら ANTHROPIC_API_KEY 環境変数から取得する
        // APIキーはログに出力しないようにする（セキュリティ要件）
        let mut config = config;
        if config.settings.anthropic_api_key.is_none() {
            if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
                config.settings.anthropic_api_key = Some(Password(key));
            }
        }
        // claude_model が未設定なら CLAUDE_MODEL 環境変数から取得する
        if config.settings.claude_model.is_none() {
            if let Ok(model) = std::env::var("CLAUDE_MODEL") {
                config.settings.claude_model = Some(model);
            }
        }

        Ok(config)
    }

    /// デフォルトbastion設定とデフォルトpool設定を適用した接続設定リストを取得
    ///
    /// - bastion = true の場合のみ default_bastion を適用（省略時は直接接続）
    /// - pool設定はフィールド単位でマージする: 個別設定 > default_postgres_pool > PoolConfig::default()
    pub fn resolve_connections(&self) -> Vec<ConnectionConfig> {
        let default_pool = self.default_postgres_pool.as_ref();

        self.connections
            .iter()
            .map(|conn| {
                // bastion設定の解決:
                // - Some(Config): 個別のbastion設定をそのまま使用
                // - Some(Toggle(true)): default_bastionを適用
                // - Some(Toggle(false)) / None: bastion不使用（直接接続）
                let bastion = match &conn.bastion {
                    Some(BastionSetting::Config(config)) => {
                        Some(BastionSetting::Config(config.clone()))
                    }
                    Some(BastionSetting::Toggle(true)) => {
                        self.default_bastion.clone().map(BastionSetting::Config)
                    }
                    Some(BastionSetting::Toggle(false)) | None => None,
                };

                // フィールド単位マージ: 個別設定のNoneフィールドはdefault_poolで補完
                // PoolConfigPartialのフィールド単位マージにより、一部フィールドだけdefault_postgres_poolから取る設定が可能
                let mut postgres = conn.postgres.clone();
                postgres.pool = PoolConfigPartial {
                    max_connections: conn
                        .postgres
                        .pool
                        .max_connections
                        .or_else(|| default_pool.and_then(|d| d.max_connections)),
                    idle_timeout: conn
                        .postgres
                        .pool
                        .idle_timeout
                        .or_else(|| default_pool.and_then(|d| d.idle_timeout)),
                };

                ConnectionConfig {
                    name: conn.name.clone(),
                    bastion,
                    postgres,
                    readonly: conn.readonly,
                }
            })
            .collect()
    }

    /// 設定のバリデーション
    fn validate(&self) -> Result<()> {
        if self.connections.is_empty() {
            return Err(Error::Config(t!(ConfigMsg::NoConnections)));
        }

        // デフォルトbastion設定のバリデーション
        if let Some(ref bastion) = self.default_bastion {
            bastion.validate()?;
        }

        // default_postgres_poolは特にバリデーション不要（PoolConfigのデフォルト値は常に有効）

        for conn in &self.connections {
            conn.validate()?;
        }

        Ok(())
    }
}

/// 文字列が空でないことを検証
fn validate_not_empty(value: &str, field_name: &str) -> Result<()> {
    if value.is_empty() {
        return Err(Error::config(t!(ConfigMsg::FieldEmpty {
            field: field_name
        })));
    }
    Ok(())
}

/// ポート番号が0でないことを検証
fn validate_port(port: u16, field_name: &str) -> Result<()> {
    if port == 0 {
        return Err(Error::config(t!(ConfigMsg::InvalidPort {
            field: field_name
        })));
    }
    Ok(())
}

impl ConnectionConfig {
    /// bastion設定を取得（個別設定 > デフォルト設定の優先順位）
    ///
    /// resolve_connections() 適用後のインスタンスで使用する場合は、
    /// bastion フィールドは Config か None のみになっているため default は参照されない。
    /// TOML からの生の ConnectionConfig に対して使用する場合は、
    /// Toggle(true) / Toggle(false) / None を適切に処理する。
    pub fn get_bastion<'a>(
        &'a self,
        default: &'a Option<BastionConfig>,
    ) -> Option<&'a BastionConfig> {
        match &self.bastion {
            Some(BastionSetting::Config(config)) => Some(config),
            Some(BastionSetting::Toggle(true)) => default.as_ref(),
            Some(BastionSetting::Toggle(false)) | None => None,
        }
    }

    /// 接続設定のバリデーション
    fn validate(&self) -> Result<()> {
        validate_not_empty(&self.name, "接続先の名前")?;

        self.postgres.validate()?;

        if let Some(BastionSetting::Config(ref bastion)) = self.bastion {
            bastion.validate()?;
        }

        Ok(())
    }
}

impl BastionConfig {
    /// bastion設定のバリデーション
    fn validate(&self) -> Result<()> {
        validate_not_empty(&self.host, "bastionホスト")?;
        validate_not_empty(&self.user, "bastionユーザー名")?;
        validate_port(self.port, "bastionポート番号")?;

        Ok(())
    }
}

impl PostgresConfig {
    /// PostgreSQL設定のバリデーション
    fn validate(&self) -> Result<()> {
        validate_not_empty(&self.host, "PostgreSQLホスト")?;
        validate_not_empty(&self.database, "データベース名")?;
        validate_not_empty(&self.user, "PostgreSQLユーザー名")?;
        validate_port(self.port, "PostgreSQLポート番号")?;

        Ok(())
    }
}

/// Unix系システムでファイル権限をチェック
#[cfg(unix)]
fn check_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(path)
        .map_err(|e| Error::ConfigLoad(format!("ファイル情報の取得に失敗しました: {}", e)))?;

    let permissions = metadata.permissions();
    let mode = permissions.mode();

    // 600 (owner read/write only) が推奨
    let owner_only = mode & 0o777;
    if owner_only != 0o600 {
        tracing::warn!(
            "{}",
            t!(ConfigMsg::PermissionWarning {
                mode: owner_only,
                path: &path.display().to_string()
            })
        );
        // Phase 1では警告のみ（エラーにしない）
    }

    Ok(())
}

fn default_ssh_port() -> u16 {
    22
}

fn default_postgres_port() -> u16 {
    5432
}

fn default_timeout() -> u64 {
    // デフォルト5秒: 設定誤りによる長時間ブロックを防ぐため短く設定する
    5
}

fn default_ssl_mode() -> SslMode {
    // PostgreSQLではRequireをデフォルトとし、本番環境での平文通信を防ぐ
    SslMode::Require
}

fn default_max_connections() -> u32 {
    10
}

fn default_idle_timeout() -> u64 {
    300
}

fn default_input_area_height() -> u16 {
    5
}

fn default_result_preview_width() -> u16 {
    30
}

fn default_result_preview_position() -> PreviewPosition {
    // 既存のプレビューウィンドウ挙動（right:30%）を踏襲する
    PreviewPosition::Right
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// テスト用: Unix環境でファイル権限を600に設定
    #[cfg(unix)]
    fn set_test_file_permissions_600(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn test_password_debug() {
        let password = Password("secret".to_string());
        let debug_str = format!("{:?}", password);
        assert!(debug_str.contains("***"));
        assert!(!debug_str.contains("secret"));
    }

    #[test]
    fn test_default_ports() {
        assert_eq!(default_ssh_port(), 22);
        assert_eq!(default_postgres_port(), 5432);
    }

    #[test]
    fn test_default_timeout() {
        assert_eq!(default_timeout(), 5);
    }

    #[test]
    fn test_config_load_valid() {
        let toml_content = r#"
[[connections]]
name = "test-connection"

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
timeout = 30
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        // Unix系のみパーミッション設定
        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap());
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!(config.connections.len(), 1);
        assert_eq!(config.connections[0].name, "test-connection");
        assert_eq!(config.connections[0].postgres.host, "localhost");
        assert_eq!(config.connections[0].postgres.port, 5432);
    }

    #[test]
    fn test_config_load_with_bastion() {
        let toml_content = r#"
[[connections]]
name = "bastion-connection"

[connections.bastion]
host = "bastion.example.com"
port = 22
user = "devuser"

[connections.postgres]
host = "postgres.internal.example.com"
port = 5432
database = "proddb"
user = "app_user"
password = "secure_password"
timeout = 60
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap());
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!(config.connections.len(), 1);
        assert!(config.connections[0].bastion.is_some());

        // BastionSetting::Config から BastionConfig を取り出して検証
        let bastion = match config.connections[0].bastion.as_ref().unwrap() {
            BastionSetting::Config(b) => b,
            _ => panic!("BastionSetting::Configが期待されます"),
        };
        assert_eq!(bastion.host, "bastion.example.com");
        assert_eq!(bastion.port, 22);
        assert_eq!(bastion.user, "devuser");
    }

    #[test]
    fn test_config_load_file_not_found() {
        let result = Config::load("/nonexistent/path/config.toml");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::ConfigLoad(_)));
    }

    #[test]
    fn test_config_validation_empty_connections() {
        let config = Config {
            default_bastion: None,
            default_postgres_pool: None,
            connections: vec![],
            settings: AppSettings::default(),
        };
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_empty_name() {
        let config = Config {
            default_bastion: None,
            default_postgres_pool: None,
            connections: vec![ConnectionConfig {
                name: "".to_string(),
                bastion: None,
                postgres: PostgresConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "test".to_string(),
                    user: "postgres".to_string(),
                    password: Password("pass".to_string()),
                    timeout: 30,
                    ssl_mode: SslMode::Require,
                    pool: PoolConfigPartial::default(),
                },
                readonly: false,
            }],
            settings: AppSettings::default(),
        };
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_postgres_config_validation_empty_host() {
        let postgres_config = PostgresConfig {
            host: "".to_string(),
            port: 5432,
            database: "test".to_string(),
            user: "postgres".to_string(),
            password: Password("pass".to_string()),
            timeout: 30,
            ssl_mode: SslMode::Require,
            pool: PoolConfigPartial::default(),
        };
        let result = postgres_config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_postgres_config_validation_empty_database() {
        let postgres_config = PostgresConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "".to_string(),
            user: "postgres".to_string(),
            password: Password("pass".to_string()),
            timeout: 30,
            ssl_mode: SslMode::Require,
            pool: PoolConfigPartial::default(),
        };
        let result = postgres_config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_postgres_config_validation_empty_user() {
        let postgres_config = PostgresConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "test".to_string(),
            user: "".to_string(),
            password: Password("pass".to_string()),
            timeout: 30,
            ssl_mode: SslMode::Require,
            pool: PoolConfigPartial::default(),
        };
        let result = postgres_config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_postgres_config_validation_invalid_port() {
        let postgres_config = PostgresConfig {
            host: "localhost".to_string(),
            port: 0,
            database: "test".to_string(),
            user: "postgres".to_string(),
            password: Password("pass".to_string()),
            timeout: 30,
            ssl_mode: SslMode::Require,
            pool: PoolConfigPartial::default(),
        };
        let result = postgres_config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_bastion_config_validation_empty_host() {
        let bastion_config = BastionConfig {
            host: "".to_string(),
            port: 22,
            user: "user".to_string(),
            key_path: None,
        };
        let result = bastion_config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_bastion_config_validation_empty_user() {
        let bastion_config = BastionConfig {
            host: "bastion.example.com".to_string(),
            port: 22,
            user: "".to_string(),
            key_path: None,
        };
        let result = bastion_config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_bastion_config_validation_invalid_port() {
        let bastion_config = BastionConfig {
            host: "bastion.example.com".to_string(),
            port: 0,
            user: "user".to_string(),
            key_path: None,
        };
        let result = bastion_config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_ssl_mode_default() {
        // PostgreSQLではRequireをデフォルトとする
        assert_eq!(default_ssl_mode(), SslMode::Require);
    }

    #[test]
    fn test_ssl_mode_deserialization() {
        // 構造体経由でSslModeをテスト
        #[derive(Deserialize)]
        struct TestConfig {
            ssl_mode: SslMode,
        }

        let toml_require = r#"ssl_mode = "require""#;
        let config: TestConfig = toml::from_str(toml_require).unwrap();
        assert_eq!(config.ssl_mode, SslMode::Require);

        let toml_prefer = r#"ssl_mode = "prefer""#;
        let config: TestConfig = toml::from_str(toml_prefer).unwrap();
        assert_eq!(config.ssl_mode, SslMode::Prefer);

        let toml_disable = r#"ssl_mode = "disable""#;
        let config: TestConfig = toml::from_str(toml_disable).unwrap();
        assert_eq!(config.ssl_mode, SslMode::Disable);

        let toml_verify_ca = r#"ssl_mode = "verify-ca""#;
        let config: TestConfig = toml::from_str(toml_verify_ca).unwrap();
        assert_eq!(config.ssl_mode, SslMode::VerifyCa);

        let toml_verify_full = r#"ssl_mode = "verify-full""#;
        let config: TestConfig = toml::from_str(toml_verify_full).unwrap();
        assert_eq!(config.ssl_mode, SslMode::VerifyFull);
    }

    #[test]
    fn test_pool_config_default() {
        let pool_config = PoolConfig::default();
        assert_eq!(pool_config.max_connections, 10);
        assert_eq!(pool_config.idle_timeout, 300);
    }

    #[test]
    fn test_pool_config_deserialization() {
        let toml_content = r#"
[[connections]]
name = "test-pool"

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"

[connections.postgres.pool]
max_connections = 20
idle_timeout = 600
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();
        // PoolConfigPartialはOption型なので、設定された値はSomeとして保持される
        assert_eq!(
            config.connections[0].postgres.pool.max_connections,
            Some(20)
        );
        assert_eq!(config.connections[0].postgres.pool.idle_timeout, Some(600));
    }

    #[test]
    fn test_pool_config_default_when_missing() {
        let toml_content = r#"
[[connections]]
name = "test-no-pool"

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();
        // pool設定が無い場合はすべてNone（resolve()で初めてデフォルト値が適用される）
        assert_eq!(config.connections[0].postgres.pool.max_connections, None);
        assert_eq!(config.connections[0].postgres.pool.idle_timeout, None);
        // resolve()でデフォルト値が適用される
        let resolved = config.connections[0].postgres.pool.resolve(None);
        assert_eq!(resolved.max_connections, 10);
        assert_eq!(resolved.idle_timeout, 300);
    }

    #[test]
    fn test_default_bastion_config() {
        let toml_content = r#"
[default_bastion]
host = "shared-bastion.example.com"
port = 22
user = "shared_user"
key_path = "/path/to/shared/key"

[[connections]]
name = "connection-using-default"
bastion = true

[connections.postgres]
host = "postgres1.internal"
port = 5432
database = "db1"
user = "user1"
password = "pass1"

[[connections]]
name = "connection-with-override"

[connections.bastion]
host = "custom-bastion.example.com"
port = 2222
user = "custom_user"

[connections.postgres]
host = "postgres2.internal"
port = 5432
database = "db2"
user = "user2"
password = "pass2"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();

        // default_bastionが設定されている
        assert!(config.default_bastion.is_some());
        let default_bastion = config.default_bastion.as_ref().unwrap();
        assert_eq!(default_bastion.host, "shared-bastion.example.com");
        assert_eq!(default_bastion.user, "shared_user");

        // 1つ目の接続: デフォルトbastion を使用
        let conn1 = &config.connections[0];
        assert_eq!(conn1.name, "connection-using-default");
        assert!(matches!(conn1.bastion, Some(BastionSetting::Toggle(true))));
        let bastion1 = conn1.get_bastion(&config.default_bastion).unwrap();
        assert_eq!(bastion1.host, "shared-bastion.example.com");

        // 2つ目の接続: 個別bastion設定で上書き
        let conn2 = &config.connections[1];
        assert_eq!(conn2.name, "connection-with-override");
        assert!(conn2.bastion.is_some());
        let bastion2 = conn2.get_bastion(&config.default_bastion).unwrap();
        assert_eq!(bastion2.host, "custom-bastion.example.com");
        assert_eq!(bastion2.port, 2222);
    }

    #[test]
    fn test_default_bastion_validation() {
        let toml_content = r#"
[default_bastion]
host = ""
port = 22
user = "user"

[[connections]]
name = "test"

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        // default_bastionのホストが空なのでバリデーションエラー
        let result = Config::load(temp_file.path().to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn test_default_postgres_pool() {
        let toml_content = r#"
[default_postgres_pool]
max_connections = 20
idle_timeout = 600

[[connections]]
name = "conn-with-default-pool"

[connections.postgres]
host = "localhost"
port = 5432
database = "db1"
user = "user1"
password = "pass1"

[[connections]]
name = "conn-with-custom-pool"

[connections.postgres]
host = "localhost"
port = 5432
database = "db2"
user = "user2"
password = "pass2"

[connections.postgres.pool]
max_connections = 5
idle_timeout = 300
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();

        // default_postgres_poolが設定されている
        assert!(config.default_postgres_pool.is_some());
        let default_pool = config.default_postgres_pool.as_ref().unwrap();
        assert_eq!(default_pool.max_connections, Some(20));
        assert_eq!(default_pool.idle_timeout, Some(600));

        // 1つ目の接続: pool設定が省略されているのですべてNone
        let conn1 = &config.connections[0];
        assert_eq!(conn1.postgres.pool, PoolConfigPartial::default());

        // resolve_connections()でdefault_postgres_poolがフィールド単位でマージされる
        let resolved = config.resolve_connections();
        // 1つ目の接続: 個別設定が省略のため、default_postgres_poolの値がマージされてSomeになる
        assert_eq!(resolved[0].postgres.pool.max_connections, Some(20));
        assert_eq!(resolved[0].postgres.pool.idle_timeout, Some(600));
        // resolve()でPoolConfigに変換
        let resolved_pool = resolved[0].postgres.pool.resolve(None);
        assert_eq!(resolved_pool.max_connections, 20);
        assert_eq!(resolved_pool.idle_timeout, 600);

        // 2つ目の接続: 個別pool設定がそのまま使用される
        assert_eq!(resolved[1].postgres.pool.max_connections, Some(5));
        assert_eq!(resolved[1].postgres.pool.idle_timeout, Some(300));
    }

    #[test]
    fn test_default_bastion_and_pool_combined() {
        let toml_content = r#"
[default_bastion]
host = "bastion.example.com"
port = 22
user = "bastionuser"

[default_postgres_pool]
max_connections = 15
idle_timeout = 500

[[connections]]
name = "conn-all-defaults"
bastion = true

[connections.postgres]
host = "postgres.internal"
port = 5432
database = "testdb"
user = "testuser"
password = "testpass"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();
        let resolved = config.resolve_connections();

        // bastion = true なので default_bastion が適用される
        let conn = &resolved[0];
        assert!(conn.bastion.is_some());
        // resolve後のbastionはBastionSetting::Configのみ
        let bastion_host = match conn.bastion.as_ref().unwrap() {
            BastionSetting::Config(b) => &b.host,
            _ => panic!("resolve後はBastionSetting::Configが期待されます"),
        };
        assert_eq!(bastion_host, "bastion.example.com");
        // PoolConfigPartialのフィールド単位マージにより、default_postgres_poolの値がSomeになっている
        assert_eq!(conn.postgres.pool.max_connections, Some(15));
        assert_eq!(conn.postgres.pool.idle_timeout, Some(500));
    }

    #[test]
    fn test_readonly_field_default_is_false() {
        // readonlyフィールドを省略した場合はfalseになることを確認（後方互換性）
        let toml_content = r#"
[[connections]]
name = "no-readonly"

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();
        assert!(
            !config.connections[0].readonly,
            "readonlyフィールド省略時はfalseになるべき"
        );
    }

    #[test]
    fn test_readonly_field_explicit_true() {
        // readonlyフィールドをtrueで指定した場合はtrueになることを確認
        let toml_content = r#"
[[connections]]
name = "readonly-conn"
readonly = true

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();
        assert!(
            config.connections[0].readonly,
            "readonlyフィールドtrueはtrueになるべき"
        );
    }

    #[test]
    fn test_readonly_field_preserved_in_resolve_connections() {
        // resolve_connections()でreadonlyフィールドが引き継がれることを確認
        let toml_content = r#"
[default_bastion]
host = "bastion.example.com"
port = 22
user = "user"

[[connections]]
name = "readonly-with-default-bastion"
readonly = true
bastion = true

[connections.postgres]
host = "postgres.internal"
port = 5432
database = "testdb"
user = "testuser"
password = "testpass"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();
        let resolved = config.resolve_connections();

        // bastion = true + default_bastion 適用後もreadonlyが維持される
        assert!(
            resolved[0].readonly,
            "resolve_connections後もreadonly=trueが維持されるべき"
        );
        assert!(
            resolved[0].bastion.is_some(),
            "bastion=trueでデフォルトbastionが適用されるべき"
        );
    }

    #[test]
    fn test_bastion_false_skips_default() {
        // `bastion = false` の接続は default_bastion が適用されないことを確認
        let toml_content = r#"
[default_bastion]
host = "bastion.example.com"
port = 22
user = "bastionuser"

[[connections]]
name = "direct-connection"
bastion = false

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();

        // 生のConfigでは bastion = Toggle(false) として保持される
        assert!(matches!(
            &config.connections[0].bastion,
            Some(BastionSetting::Toggle(false))
        ));

        // get_bastion()でNoneが返ること（直接接続）
        let bastion = config.connections[0].get_bastion(&config.default_bastion);
        assert!(
            bastion.is_none(),
            "bastion = false のとき direct 接続になるべき"
        );

        // resolve_connections()でもbastionはNoneになること
        let resolved = config.resolve_connections();
        assert!(
            resolved[0].bastion.is_none(),
            "resolve後、bastion = false の接続はbastionがNoneになるべき"
        );
    }

    #[test]
    fn test_bastion_true_applies_default() {
        // `bastion = true` の接続は default_bastion が適用されることを確認
        let toml_content = r#"
[default_bastion]
host = "bastion.example.com"
port = 22
user = "bastionuser"

[[connections]]
name = "explicit-bastion-true"
bastion = true

[connections.postgres]
host = "postgres.internal"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();

        // 生のConfigでは bastion = Toggle(true) として保持される
        assert!(matches!(
            &config.connections[0].bastion,
            Some(BastionSetting::Toggle(true))
        ));

        // get_bastion()でdefault_bastionが返ること
        let bastion = config.connections[0]
            .get_bastion(&config.default_bastion)
            .unwrap();
        assert_eq!(bastion.host, "bastion.example.com");

        // resolve_connections()でdefault_bastionが適用されること
        let resolved = config.resolve_connections();
        assert!(
            resolved[0].bastion.is_some(),
            "resolve後、bastion = true の接続はdefault_bastionが適用されるべき"
        );
        let resolved_host = match resolved[0].bastion.as_ref().unwrap() {
            BastionSetting::Config(b) => &b.host,
            _ => panic!("resolve後はBastionSetting::Configが期待されます"),
        };
        assert_eq!(resolved_host, "bastion.example.com");
    }

    #[test]
    fn test_bastion_false_deserialization() {
        // TOMLから `bastion = false` が正しくデシリアライズされることを確認
        let toml_content = r#"
[[connections]]
name = "local-test"
bastion = false

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();

        // bastion = false がToggle(false)として読み込まれること
        assert!(matches!(
            &config.connections[0].bastion,
            Some(BastionSetting::Toggle(false))
        ));
    }

    #[test]
    fn test_layout_settings_default_when_section_missing() {
        // [settings.layout] セクションを省略した場合、全フィールドがデフォルト値になること
        let toml_content = r#"
[[connections]]
name = "test-layout-default"

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();
        let layout = config.settings.layout;
        assert_eq!(layout.sql_input_height, 5);
        assert_eq!(layout.shell_input_height, 5);
        assert_eq!(layout.prompt_input_height, 5);
        assert_eq!(layout.result_preview_width, 30);
        assert_eq!(layout.result_preview_position, PreviewPosition::Right);
    }

    #[test]
    fn test_layout_settings_all_fields_specified() {
        // 全フィールドを指定した場合、指定値が反映されること
        let toml_content = r#"
[settings.layout]
sql_input_height = 8
shell_input_height = 6
prompt_input_height = 4
result_preview_width = 50
result_preview_position = "left"

[[connections]]
name = "test-layout-full"

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();
        let layout = config.settings.layout;
        assert_eq!(layout.sql_input_height, 8);
        assert_eq!(layout.shell_input_height, 6);
        assert_eq!(layout.prompt_input_height, 4);
        assert_eq!(layout.result_preview_width, 50);
        assert_eq!(layout.result_preview_position, PreviewPosition::Left);
    }

    #[test]
    fn test_layout_settings_partial_fields_specified() {
        // 一部フィールドのみ指定した場合、残りはデフォルトになること
        let toml_content = r#"
[settings.layout]
sql_input_height = 10

[[connections]]
name = "test-layout-partial"

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let config = Config::load(temp_file.path().to_str().unwrap()).unwrap();
        let layout = config.settings.layout;
        assert_eq!(layout.sql_input_height, 10);
        assert_eq!(layout.shell_input_height, 5);
        assert_eq!(layout.prompt_input_height, 5);
        assert_eq!(layout.result_preview_width, 30);
        assert_eq!(layout.result_preview_position, PreviewPosition::Right);
    }

    #[test]
    fn test_layout_settings_invalid_preview_position() {
        // result_preview_position に不正な値を指定した場合、Config::loadがErrを返すこと
        let toml_content = r#"
[settings.layout]
result_preview_position = "top"

[[connections]]
name = "test-layout-invalid"

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "postgres"
password = "password"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        #[cfg(unix)]
        set_test_file_permissions_600(temp_file.path());

        let result = Config::load(temp_file.path().to_str().unwrap());
        assert!(result.is_err());
    }
}
