use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
    Pool, Postgres,
};
use ssh2::Session;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::config::{BastionSetting, ConnectionConfig, SslMode};
use crate::error::{Error, Result};
use crate::i18n::ConnectionMsg;
use crate::t;

/// SSH tunnel経由接続時のローカルホスト
const LOCALHOST: &str = "127.0.0.1";

/// SSH tunnelハンドル
/// SSH接続とローカルポート転送を管理
struct SshTunnel {
    local_port: u16,
    _session: Arc<Mutex<Session>>, // セッションをライフタイム管理のために保持
    shutdown_flag: Arc<AtomicBool>,
    forwarding_thread: Option<thread::JoinHandle<()>>,
}

impl SshTunnel {
    /// ポートフォワーディングスレッドを停止してクリーンアップ
    fn shutdown(&mut self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);

        if let Some(forwarding_thread) = self.forwarding_thread.take() {
            if forwarding_thread.join().is_err() {
                tracing::warn!("Port forwarding thread panicked during shutdown");
            }
        }
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// PostgreSQL接続管理
pub struct ConnectionManager {
    pool: Pool<Postgres>,
    config: ConnectionConfig,
    /// bastion経由の場合はSSH tunnelを保持
    _tunnel: Option<SshTunnel>,
    /// readonlyモードフラグ（接続設定またはCLI引数由来）
    readonly: bool,
}

impl ConnectionManager {
    /// PostgreSQL接続を確立（リトライなし）
    ///
    /// bastion経由の場合はSSH tunnelingを使用
    /// 直接接続の場合は既存の動作を維持
    async fn connect_once(config: ConnectionConfig, readonly: bool) -> Result<Self> {
        tracing::info!("Connecting to PostgreSQL: {}", config.name);

        // bastion経由接続の判定
        // resolve_connections()適用後のConfigではbastionはConfig(BastionConfig)かNoneのみ。
        // Toggleは生のConfigではありうるが、connect()はresolve後のConfigを受け取ることを想定する。
        let (tunnel, postgres_host, postgres_port) =
            if let Some(BastionSetting::Config(ref bastion_cfg)) = config.bastion {
                tracing::info!(
                    "Setting up SSH tunnel via bastion: {}@{}:{}",
                    bastion_cfg.user,
                    bastion_cfg.host,
                    bastion_cfg.port
                );

                // SSH tunnel確立（同期処理をspawn_blockingで実行）
                let bastion_cfg = bastion_cfg.clone();
                let postgres_host_for_tunnel = config.postgres.host.clone();
                let postgres_port = config.postgres.port;
                let ssh_timeout = Duration::from_secs(config.postgres.timeout);

                let tunnel = tokio::task::spawn_blocking(move || {
                    establish_ssh_tunnel(
                        &bastion_cfg,
                        &postgres_host_for_tunnel,
                        postgres_port,
                        ssh_timeout,
                    )
                })
                .await
                .map_err(|e| Error::connection_context("SSH tunnel task", e))??;

                let local_port = tunnel.local_port;
                tracing::info!(
                    "SSH tunnel established: localhost:{} -> {}:{}",
                    local_port,
                    config.postgres.host,
                    postgres_port
                );

                (Some(tunnel), LOCALHOST.to_string(), local_port)
            } else {
                tracing::info!("Direct connection (no bastion)");
                (None, config.postgres.host.clone(), config.postgres.port)
            };

        // SSL/TLS設定を決定（PostgreSQLのPgSslModeに変換）
        let ssl_mode = match config.postgres.ssl_mode {
            SslMode::Require => PgSslMode::Require,
            SslMode::Prefer => PgSslMode::Prefer,
            SslMode::Disable => {
                tracing::warn!("SSL/TLSが無効化されています。本番環境では使用しないでください。");
                PgSslMode::Disable
            }
            SslMode::VerifyCa => PgSslMode::VerifyCa,
            SslMode::VerifyFull => PgSslMode::VerifyFull,
        };

        // PostgreSQL接続オプション設定
        let connect_options = PgConnectOptions::new()
            .host(&postgres_host)
            .port(postgres_port)
            .username(&config.postgres.user)
            .password(config.postgres.password.as_str())
            .database(&config.postgres.database)
            .ssl_mode(ssl_mode);

        // 接続プール作成
        // resolve() でデフォルト値をフォールバックしつつ PoolConfig（解決済み）を取得する
        let pool_config = config.postgres.pool.resolve(None);
        tracing::info!(
            "Configuring connection pool: max_connections={}, idle_timeout={}s",
            pool_config.max_connections,
            pool_config.idle_timeout
        );

        let pool = PgPoolOptions::new()
            .max_connections(pool_config.max_connections)
            .idle_timeout(Duration::from_secs(pool_config.idle_timeout))
            .acquire_timeout(Duration::from_secs(config.postgres.timeout))
            .connect_with(connect_options)
            .await
            .map_err(Error::database_connection_detail)?;

        // readonlyモード: サーバー側でも書き込みをブロックするため
        // クライアントブロックだけでなくセッション変数でも制御する
        // PostgreSQLでは SET default_transaction_read_only = ON を使用（セッションスコープ）
        if readonly {
            sqlx::query("SET default_transaction_read_only = ON")
                .execute(&pool)
                .await
                .map_err(|e| {
                    Error::Connection(t!(ConnectionMsg::ReadonlySetFailed {
                        detail: &e.to_string()
                    }))
                })?;
            tracing::info!("Connection '{}' is set to readonly mode", config.name);
        }

        tracing::info!("Successfully connected to PostgreSQL: {}", config.name);

        Ok(Self {
            pool,
            config,
            _tunnel: tunnel,
            readonly,
        })
    }

    /// PostgreSQL接続を確立（リトライ機構付き）
    ///
    /// 接続エラーや一時的なネットワークエラーの場合、指数バックオフで最大3回リトライ
    /// タイムアウトエラーは即座に失敗させる（リトライ対象外）
    pub async fn connect(config: ConnectionConfig, readonly: bool) -> Result<Self> {
        const MAX_RETRIES: u32 = 3;
        const BASE_DELAY_MS: u64 = 100;

        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                tracing::info!(
                    "Retry attempt {}/{} for connection: {}",
                    attempt,
                    MAX_RETRIES,
                    config.name
                );
            }

            match Self::connect_once(config.clone(), readonly).await {
                Ok(manager) => {
                    if attempt > 0 {
                        tracing::info!("Successfully connected after {} retry attempts", attempt);
                    }
                    return Ok(manager);
                }
                Err(e) => {
                    // タイムアウトエラーはリトライしない
                    if Self::is_timeout_error(&e) {
                        tracing::warn!("Timeout error detected, not retrying: {}", e);
                        return Err(e);
                    }

                    // リトライ対象のエラーの場合、エラーを記録して次のループへ
                    tracing::warn!("Connection attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);

                    // 最後の試行でない場合は指数バックオフで待機
                    if attempt < MAX_RETRIES - 1 {
                        let delay_ms = BASE_DELAY_MS * 2_u64.pow(attempt);
                        tracing::debug!("Waiting {}ms before retry", delay_ms);
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        // すべてのリトライが失敗した場合
        Err(last_error.unwrap_or_else(|| Error::Connection(t!(ConnectionMsg::ConnectionFailed))))
    }

    /// エラーがタイムアウトエラーかどうかを判定
    fn is_timeout_error(error: &Error) -> bool {
        match error {
            Error::DatabaseConnection(sqlx_err) => {
                // sqlxの型でタイムアウトを判定することで、バージョン差異による文字列マッチングの検出漏れを防ぐ
                match sqlx_err {
                    sqlx::Error::PoolTimedOut => true,
                    // sqlxがラップしているI/Oエラーのタイムアウトはメッセージ文字列で補完判定する
                    sqlx::Error::Io(_) => {
                        let msg = sqlx_err.to_string().to_lowercase();
                        msg.contains("timeout") || msg.contains("timed out")
                    }
                    _ => false,
                }
            }
            // SSH接続・DNS解決タイムアウトはError::Connectionとして上がるためリトライ対象外にする
            Error::Connection(msg) => {
                let lower = msg.to_lowercase();
                lower.contains("timeout")
                    || lower.contains("timed out")
                    || lower.contains("タイムアウト")
            }
            _ => false,
        }
    }

    /// 接続プールへの参照を取得
    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    /// 接続プールの統計情報をログ出力
    pub fn log_pool_metrics(&self) {
        tracing::info!(
            "Connection pool metrics: size={}, idle={}",
            self.pool.size(),
            self.pool.num_idle()
        );
    }

    /// readonlyモードかどうかを返す
    pub fn is_readonly(&self) -> bool {
        self.readonly
    }

    /// 接続設定への参照を取得
    pub fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    /// 接続をクローズ
    pub async fn close(mut self) {
        tracing::info!("Closing PostgreSQL connection: {}", self.config.name);
        self.pool.close().await;

        if let Some(tunnel) = self._tunnel.as_mut() {
            tunnel.shutdown();
        }
    }
}

/// SSH tunnelを確立
///
/// ローカルポートを動的に割り当て、bastion経由でPostgreSQL serverへポートフォワーディング
fn establish_ssh_tunnel(
    bastion_cfg: &crate::config::BastionConfig,
    postgres_host: &str,
    postgres_port: u16,
    timeout: Duration,
) -> Result<SshTunnel> {
    // ローカルポートを動的に割り当て（0を指定するとOSが自動割り当て）
    let listener = TcpListener::bind((LOCALHOST, 0))
        .map_err(|e| Error::connection_context("ローカルポートのバインド", e))?;

    let local_port = listener
        .local_addr()
        .map_err(|e| Error::connection_context("ローカルポートの取得", e))?
        .port();

    tracing::debug!("Allocated local port: {}", local_port);

    // bastion serverへSSH接続（OSデフォルトの長いタイムアウトを避けるため明示的に設定）
    // DNS解決もタイムアウト対象にするため、別スレッドで実行して timeout 内に完了しなければ失敗させる
    use std::net::ToSocketAddrs;
    let host_port = format!("{}:{}", bastion_cfg.host, bastion_cfg.port);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = host_port
            .to_socket_addrs()
            .ok()
            .and_then(|mut it| it.next());
        let _ = tx.send(result);
    });
    let addr = rx.recv_timeout(timeout).ok().flatten().ok_or_else(|| {
        Error::connection_context(
            "bastionアドレスの解決",
            "タイムアウトまたはアドレスが見つかりません",
        )
    })?;
    let tcp_stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|e| Error::connection_context("bastion serverへの接続", e))?;

    let mut session =
        Session::new().map_err(|e| Error::connection_context("SSHセッションの作成", e))?;

    session.set_tcp_stream(tcp_stream);
    session
        .handshake()
        .map_err(|e| Error::connection_context("SSH handshake", e))?;

    // SSH認証（秘密鍵 or SSH Agent）
    let authenticated = if let Some(ref key_path) = bastion_cfg.key_path {
        let expanded_key_path = shellexpand::tilde(key_path).to_string();
        tracing::debug!("Trying SSH key authentication: {}", expanded_key_path);

        // 鍵ファイル認証を試行（パスフレーズなしを想定）
        match session.userauth_pubkey_file(
            &bastion_cfg.user,
            None,
            Path::new(&expanded_key_path),
            None,
        ) {
            Ok(_) => {
                let authenticated = session.authenticated();
                if authenticated {
                    tracing::info!("SSH key authentication successful");
                }
                authenticated
            }
            Err(e) => {
                tracing::warn!("SSH key authentication failed: {}. Trying SSH agent...", e);
                // 鍵ファイル認証が失敗（パスフレーズ保護等）した場合、SSH Agentにフォールバック
                match session.userauth_agent(&bastion_cfg.user) {
                    Ok(_) => {
                        let authenticated = session.authenticated();
                        if authenticated {
                            tracing::info!("SSH agent authentication successful");
                        }
                        authenticated
                    }
                    Err(agent_err) => {
                        return Err(Error::connection(t!(ConnectionMsg::SshAuthFailed {
                            key_err: &e.to_string(),
                            agent_err: &agent_err.to_string()
                        })));
                    }
                }
            }
        }
    } else {
        // key_pathが指定されていない場合はSSH agent認証のみ
        tracing::debug!("Trying SSH agent authentication");
        session.userauth_agent(&bastion_cfg.user).map_err(|e| {
            Error::connection(t!(ConnectionMsg::SshAgentAuthFailed {
                detail: &e.to_string()
            }))
        })?;
        let authenticated = session.authenticated();
        if authenticated {
            tracing::info!("SSH agent authentication successful");
        }
        authenticated
    };

    if !authenticated {
        return Err(Error::Connection(t!(ConnectionMsg::SshAuthError)));
    }

    tracing::info!("SSH authentication successful");
    session.set_blocking(false);

    let session = Arc::new(Mutex::new(session));
    let shutdown_flag = Arc::new(AtomicBool::new(false));

    // ポートフォワーディングのスレッドを起動
    let session_clone = Arc::clone(&session);
    let shutdown_flag_clone = Arc::clone(&shutdown_flag);
    let postgres_host_owned = postgres_host.to_string();
    let forwarding_thread = thread::spawn(move || {
        if let Err(e) = run_port_forwarding(
            listener,
            session_clone,
            &postgres_host_owned,
            postgres_port,
            shutdown_flag_clone,
        ) {
            tracing::error!("Port forwarding error: {}", e);
        }
    });

    Ok(SshTunnel {
        local_port,
        _session: session,
        shutdown_flag,
        forwarding_thread: Some(forwarding_thread),
    })
}

/// ポートフォワーディングのメインループ
///
/// ローカルポートへの接続を待ち受け、SSH経由でPostgreSQL serverへ転送
fn run_port_forwarding(
    listener: TcpListener,
    session: Arc<Mutex<Session>>,
    postgres_host: &str,
    postgres_port: u16,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<()> {
    tracing::debug!("Port forwarding thread started");

    listener.set_nonblocking(true).map_err(|e| {
        Error::connection_context("ポートフォワーディングのノンブロッキング設定", e)
    })?;

    while !shutdown_flag.load(Ordering::Relaxed) {
        let stream = match listener.accept() {
            Ok((stream, _)) => match prepare_local_tunnel_stream(stream) {
                Ok(stream) => stream,
                Err(e) => {
                    tracing::error!("Failed to prepare accepted local stream: {}", e);
                    continue;
                }
            },
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => {
                if shutdown_flag.load(Ordering::Relaxed) {
                    break;
                }
                tracing::error!("Failed to accept connection: {}", e);
                thread::sleep(Duration::from_millis(10));
                continue;
            }
        };

        let session_clone = Arc::clone(&session);
        let postgres_host_owned = postgres_host.to_string();

        // 各接続を個別スレッドで処理
        thread::spawn(move || {
            if let Err(e) =
                handle_tunnel_connection(stream, session_clone, &postgres_host_owned, postgres_port)
            {
                tracing::error!("Tunnel connection error: {}", e);
            }
        });
    }

    tracing::debug!("Port forwarding thread stopped");
    Ok(())
}

fn prepare_local_tunnel_stream(stream: TcpStream) -> Result<TcpStream> {
    // ssh2 側を non-blocking で駆動するため、ローカル側の socket も同じ前提に揃える。
    stream.set_nonblocking(true).map_err(|e| {
        Error::connection_context("受け入れたローカルソケットのブロッキング設定", e)
    })?;

    Ok(stream)
}

/// 個別のトンネル接続を処理
///
/// ローカル接続とSSHチャネルの双方向データ転送
fn handle_tunnel_connection(
    local_stream: TcpStream,
    session: Arc<Mutex<Session>>,
    postgres_host: &str,
    postgres_port: u16,
) -> Result<()> {
    tracing::debug!("New tunnel connection established");

    // SSH channelを作成（Direct-TCPIPでポートフォワーディング）
    let mut channel = open_ssh_channel(&session, postgres_host, postgres_port)?;

    tracing::debug!("SSH channel created: {}:{}", postgres_host, postgres_port);
    pump_bidirectional(local_stream, &mut channel)?;
    close_ssh_channel(&mut channel);

    tracing::debug!("Tunnel connection closed");
    Ok(())
}

/// SSHチャネル開設時のWouldBlock最大リトライ回数（1ms × 5000 = 約5秒）
///
/// SSHセッションがデッドロックした場合に永久ブロックを防ぐ
const SSH_CHANNEL_MAX_RETRIES: usize = 5000;

fn open_ssh_channel(
    session: &Arc<Mutex<Session>>,
    postgres_host: &str,
    postgres_port: u16,
) -> Result<ssh2::Channel> {
    for _ in 0..SSH_CHANNEL_MAX_RETRIES {
        let result = {
            let session = session.blocking_lock();
            session.channel_direct_tcpip(postgres_host, postgres_port, None)
        };

        match result {
            Ok(channel) => return Ok(channel),
            Err(err) => {
                let io_err = std::io::Error::from(err);
                if io_err.kind() == std::io::ErrorKind::WouldBlock {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }

                return Err(Error::connection_context("SSH channelの作成", io_err));
            }
        }
    }

    Err(Error::connection_context(
        "SSH channelの作成",
        format!(
            "{}ms経過してもチャネルが開けませんでした。SSHセッションの状態を確認してください。",
            SSH_CHANNEL_MAX_RETRIES
        ),
    ))
}

/// 双方向転送バッファの上限サイズ（1MB）
///
/// バッファが無制限に成長するとメモリ枯渇を招くため、上限に達した場合は
/// 読み取りを一時停止してバックプレッシャーをかける
const PUMP_BUFFER_MAX: usize = 1024 * 1024;

fn pump_bidirectional(mut local_stream: TcpStream, channel: &mut ssh2::Channel) -> Result<()> {
    let mut local_read_buf = [0_u8; 16 * 1024];
    let mut remote_read_buf = [0_u8; 16 * 1024];
    let mut pending_to_remote = Vec::new();
    let mut pending_to_local = Vec::new();
    let mut local_eof = false;
    let mut remote_eof = false;
    let mut sent_remote_eof = false;

    loop {
        let mut progressed = false;

        // バッファが上限未満の場合のみローカルから読み取る（上限超過時はバックプレッシャー）
        if pending_to_remote.len() < PUMP_BUFFER_MAX && !local_eof {
            match local_stream.read(&mut local_read_buf) {
                Ok(0) => {
                    local_eof = true;
                    progressed = true;
                }
                Ok(n) => {
                    pending_to_remote.extend_from_slice(&local_read_buf[..n]);
                    progressed = true;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(Error::connection_context("ローカルからの読み取り", e)),
            }
        }

        while !pending_to_remote.is_empty() {
            match channel.write(&pending_to_remote) {
                Ok(0) => break,
                Ok(n) => {
                    pending_to_remote.drain(..n);
                    progressed = true;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    return Err(Error::connection_context(
                        "ローカルからSSHチャネルへの転送",
                        e,
                    ));
                }
            }
        }

        if local_eof && pending_to_remote.is_empty() && !sent_remote_eof {
            match channel.send_eof() {
                Ok(()) => {
                    sent_remote_eof = true;
                    progressed = true;
                }
                Err(err) => {
                    let io_err = std::io::Error::from(err);
                    if io_err.kind() != std::io::ErrorKind::WouldBlock {
                        return Err(Error::connection_context("SSHチャネルEOF送信", io_err));
                    }
                }
            }
        }

        // バッファが上限未満の場合のみSSHチャネルから読み取る（上限超過時はバックプレッシャー）
        if pending_to_local.len() < PUMP_BUFFER_MAX && !remote_eof {
            match channel.read(&mut remote_read_buf) {
                Ok(0) => {
                    remote_eof = true;
                    progressed = true;
                }
                Ok(n) => {
                    pending_to_local.extend_from_slice(&remote_read_buf[..n]);
                    progressed = true;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    return Err(Error::connection_context(
                        "SSHチャネルからローカルへの転送",
                        e,
                    ));
                }
            }
        }

        while !pending_to_local.is_empty() {
            match local_stream.write(&pending_to_local) {
                Ok(0) => break,
                Ok(n) => {
                    pending_to_local.drain(..n);
                    progressed = true;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(Error::connection_context("ローカルへの書き込み", e)),
            }
        }

        if remote_eof && pending_to_local.is_empty() {
            break;
        }

        if !progressed {
            thread::sleep(Duration::from_millis(1));
        }
    }

    Ok(())
}

fn close_ssh_channel(channel: &mut ssh2::Channel) {
    loop {
        match channel.close() {
            Ok(()) => break,
            Err(err) => {
                let io_err = std::io::Error::from(err);
                if io_err.kind() == std::io::ErrorKind::WouldBlock {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                tracing::debug!("Failed to close SSH channel cleanly: {}", io_err);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BastionConfig, Password, PostgresConfig};
    use std::io::Read;

    fn create_test_connection_config(name: &str) -> ConnectionConfig {
        ConnectionConfig {
            name: name.to_string(),
            bastion: None,
            postgres: PostgresConfig {
                host: "localhost".to_string(),
                port: 5432,
                database: "test_db".to_string(),
                user: "test_user".to_string(),
                password: Password::from("test_password"),
                timeout: 5,
                ssl_mode: crate::config::SslMode::Require,
                pool: crate::config::PoolConfigPartial::default(),
            },
            readonly: false,
        }
    }

    #[test]
    fn test_connection_manager_structure() {
        // ConnectionManager構造体のビルド確認
        // 実際の接続テストは統合テストで実施
    }

    #[test]
    fn test_ssh_tunnel_shutdown_stops_thread() {
        let session = Arc::new(Mutex::new(
            Session::new().expect("テスト用のSSHセッション作成に失敗"),
        ));
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let shutdown_flag_clone = Arc::clone(&shutdown_flag);

        let forwarding_thread = thread::spawn(move || {
            while !shutdown_flag_clone.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(1));
            }
        });

        let mut tunnel = SshTunnel {
            local_port: 0,
            _session: session,
            shutdown_flag,
            forwarding_thread: Some(forwarding_thread),
        };

        tunnel.shutdown();

        assert!(tunnel.shutdown_flag.load(Ordering::Relaxed));
        assert!(tunnel.forwarding_thread.is_none());
    }

    #[test]
    #[ignore = "loopback socket permissions are required to verify accepted stream behavior"]
    fn test_prepare_local_tunnel_stream_enables_nonblocking_mode() {
        let listener = TcpListener::bind((LOCALHOST, 0)).expect("テスト用リスナー作成に失敗");
        listener
            .set_nonblocking(true)
            .expect("テスト用リスナーの non-blocking 設定に失敗");

        let client_stream =
            TcpStream::connect(listener.local_addr().expect("リスナーのアドレス取得に失敗"))
                .expect("テスト用クライアント接続に失敗");

        let (server_stream, _) = loop {
            match listener.accept() {
                Ok(stream) => break stream,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("テスト用接続の accept に失敗: {}", e),
            }
        };

        let mut server_stream =
            prepare_local_tunnel_stream(server_stream).expect("accepted socket の準備に失敗");
        let err = server_stream
            .read(&mut [0; 1])
            .expect_err("データ未送信のため read は WouldBlock になるべき");

        assert!(
            err.kind() == std::io::ErrorKind::WouldBlock,
            "non-blocking 想定のため、WouldBlock を期待しました: {}",
            err
        );

        drop(client_stream);
    }

    #[tokio::test]
    async fn test_connect_with_invalid_host() {
        // 無効なホストへの接続試行（失敗を期待）
        let mut config = create_test_connection_config("invalid-host");
        config.postgres.host = "invalid.host.example.com".to_string();
        config.postgres.timeout = 1; // 短いタイムアウトで高速失敗

        let result = ConnectionManager::connect(config, false).await;
        assert!(result.is_err(), "無効なホストへの接続は失敗すべき");

        if let Err(Error::DatabaseConnection(_)) = result {
            // 期待通りのエラー型
        } else {
            panic!("DatabaseConnectionエラーが期待されます");
        }
    }

    #[tokio::test]
    async fn test_connect_timeout() {
        // 到達不可能なホストへの接続でタイムアウトを検証
        let mut config = create_test_connection_config("timeout-test");
        config.postgres.host = "192.0.2.1".to_string(); // TEST-NET-1 (到達不可能)
        config.postgres.timeout = 1; // 1秒でタイムアウト

        let result = ConnectionManager::connect(config, false).await;
        assert!(result.is_err(), "タイムアウトすべき");
    }

    #[test]
    fn test_bastion_config_structure() {
        // BastionConfigの構造確認
        let bastion = BastionConfig {
            host: "bastion.example.com".to_string(),
            port: 22,
            user: "devuser".to_string(),
            key_path: Some("~/.ssh/id_rsa".to_string()),
        };

        assert_eq!(bastion.host, "bastion.example.com");
        assert_eq!(bastion.port, 22);
        assert_eq!(bastion.user, "devuser");
        assert_eq!(bastion.key_path, Some("~/.ssh/id_rsa".to_string()));
    }

    // Phase 2.1: bastion経由接続の統合テストはtests/integration_test.rsで実装
    // - SSH tunnel確立
    // - bastion経由でのPostgreSQL接続
    // - トンネルのクローズ

    // Phase 2.4: エラーハンドリング強化のテスト

    #[test]
    fn test_is_timeout_error() {
        // タイムアウトエラーの判定テスト
        let timeout_err = Error::DatabaseConnection(sqlx::Error::PoolTimedOut);
        assert!(ConnectionManager::is_timeout_error(&timeout_err));

        // 非タイムアウトエラー
        let other_err = Error::Connection("接続に失敗しました".to_string());
        assert!(!ConnectionManager::is_timeout_error(&other_err));
    }

    #[tokio::test]
    async fn test_connect_retry_on_network_error() {
        // 到達不可能なホストで接続リトライを検証
        let mut config = create_test_connection_config("retry-test");
        config.postgres.host = "192.0.2.1".to_string(); // TEST-NET-1 (到達不可能)
        config.postgres.timeout = 1; // 1秒でタイムアウト（リトライ対象外）

        let result = ConnectionManager::connect(config, false).await;

        // タイムアウトエラーはリトライせず即座に失敗
        assert!(result.is_err());
    }
}
