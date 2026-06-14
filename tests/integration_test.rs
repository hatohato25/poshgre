use poshgre::config::{
    BastionConfig, BastionSetting, Config, ConnectionConfig, PostgresConfig, Password,
    PoolConfigPartial, SslMode,
};
use poshgre::connection::ConnectionManager;
use poshgre::query;
use std::process::Command;
use std::time::Duration;
use testcontainers::clients::Cli;
use testcontainers_modules::postgres::Postgres;

/// 統合テスト用のPostgreSQL接続設定を作成
fn create_test_postgres_config(host: &str, port: u16) -> ConnectionConfig {
    ConnectionConfig {
        name: "integration-test".to_string(),
        bastion: None,
        postgres: PostgresConfig {
            host: host.to_string(),
            port,
            database: "postgres".to_string(),
            user: "postgres".to_string(),
            password: Password::from("postgres"),
            timeout: 10,
            ssl_mode: SslMode::Disable, // テストコンテナではSSL無効
            pool: PoolConfigPartial::default(),
        },
        readonly: false,
    }
}

/// testcontainers が利用する docker CLI / daemon が使えるかを確認
fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Docker が使えない場合にテストをスキップする
fn skip_if_docker_unavailable(test_name: &str) -> bool {
    if docker_available() {
        false
    } else {
        eprintln!(
            "Warning: docker CLI または daemon が利用できないため、{} をスキップします",
            test_name
        );
        true
    }
}

#[tokio::test]
async fn test_postgres_connection_with_testcontainers() {
    if skip_if_docker_unavailable("test_postgres_connection_with_testcontainers") {
        return;
    }

    // テストコンテナを起動（docker CLI が必要）
    let docker = Cli::default();

    // PostgreSQLコンテナを起動
    let postgres_container = docker.run(Postgres::default());
    let host = "127.0.0.1";
    let port = postgres_container.get_host_port_ipv4(5432);

    // 接続設定を作成
    let config = create_test_postgres_config(host, port);

    // 接続確立
    let manager = ConnectionManager::connect(config, false)
        .await
        .expect("testcontainers で起動した PostgreSQL へ接続できるはず");

    // 基本的な接続確認
    let pool = manager.pool();
    let result = sqlx::query("SELECT 1 as value").fetch_one(pool).await;

    assert!(result.is_ok(), "基本的なSELECTクエリが実行できる");

    // 接続をクローズ
    manager.close().await;
}

#[tokio::test]
async fn test_connection_timeout() {
    // 到達不可能なホストでタイムアウトをテスト
    let mut config = create_test_postgres_config("192.0.2.1", 5432); // TEST-NET-1
    config.postgres.timeout = 1; // 1秒でタイムアウト

    let result = ConnectionManager::connect(config, false).await;
    assert!(result.is_err(), "到達不可能なホストへの接続はエラーになる");
}

#[tokio::test]
async fn test_connection_with_invalid_credentials() {
    // 無効な認証情報でのテスト（testcontainersなしでも実行可能）
    let config = create_test_postgres_config("localhost", 5432);

    let result = ConnectionManager::connect(config, false).await;
    // PostgreSQLサーバーが存在しない、または認証が失敗することを期待
    assert!(result.is_err(), "無効な認証情報での接続は失敗する");
}

#[tokio::test]
async fn test_ssl_mode_configuration() {
    // SSL/TLS設定のバリエーションテスト
    let mut config = create_test_postgres_config("localhost", 5432);

    // Require
    config.postgres.ssl_mode = SslMode::Require;
    let result = ConnectionManager::connect(config.clone(), false).await;
    // PostgreSQLサーバーがない場合はエラーが期待される
    assert!(result.is_err());

    // Prefer
    config.postgres.ssl_mode = SslMode::Prefer;
    let result = ConnectionManager::connect(config.clone(), false).await;
    assert!(result.is_err());

    // Disable
    config.postgres.ssl_mode = SslMode::Disable;
    let result = ConnectionManager::connect(config, false).await;
    assert!(result.is_err());
}

/// エンドツーエンドテスト: 接続 → クエリ実行 → 結果取得
#[tokio::test]
async fn test_end_to_end_query_execution() {
    if skip_if_docker_unavailable("test_end_to_end_query_execution") {
        return;
    }

    let docker = Cli::default();
    let postgres_container = docker.run(Postgres::default());
    let host = "127.0.0.1";
    let port = postgres_container.get_host_port_ipv4(5432);

    let config = create_test_postgres_config(host, port);

    // 接続確立
    let manager = ConnectionManager::connect(config, false)
        .await
        .expect("testcontainers で起動した PostgreSQL へ接続できるはず");
    let pool = manager.pool();

    // テーブル作成
    let create_table_sql = r"
        CREATE TABLE IF NOT EXISTS test_users (
            id SERIAL PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            email VARCHAR(100),
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
    ";
    let result = sqlx::query(create_table_sql).execute(pool).await;
    assert!(result.is_ok(), "テーブル作成が成功する");

    // データ挿入
    let insert_sql = "INSERT INTO test_users (name, email) VALUES ($1, $2)";
    for i in 1..=5 {
        let result = sqlx::query(insert_sql)
            .bind(format!("User{}", i))
            .bind(format!("user{}@example.com", i))
            .execute(pool)
            .await;
        assert!(result.is_ok(), "データ挿入が成功する");
    }

    // SELECTクエリ実行（query.rsを使用）
    let select_sql = "SELECT id, name, email FROM test_users ORDER BY id";
    let query_result = query::execute_query(pool, select_sql, None).await;
    assert!(query_result.is_ok(), "SELECTクエリが成功する");

    let query_result = query_result.unwrap();
    assert_eq!(query_result.columns.len(), 3, "カラム数は3");
    assert_eq!(query_result.columns[0], "id");
    assert_eq!(query_result.columns[1], "name");
    assert_eq!(query_result.columns[2], "email");
    assert_eq!(query_result.row_count(), 5, "5行のデータが取得できる");
    assert!(
        query_result.execution_time > Duration::from_secs(0),
        "実行時間が記録される"
    );

    // 最初の行の検証
    assert_eq!(query_result.rows[0][0], "1", "最初の行のIDは1");
    assert_eq!(query_result.rows[0][1], "User1", "最初の行のnameはUser1");
    assert_eq!(
        query_result.rows[0][2], "user1@example.com",
        "最初の行のemailはuser1@example.com"
    );

    manager.close().await;
}

/// 設定ファイル読み込みの統合テスト
#[tokio::test]
async fn test_config_file_loading() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // 一時ファイルに設定を書き込み
    let mut temp_file = NamedTempFile::new().expect("一時ファイル作成");
    let config_content = r#"
[[connections]]
name = "test-connection"

[connections.postgres]
host = "localhost"
port = 5432
database = "testdb"
user = "testuser"
password = "testpass"
timeout = 15
ssl_mode = "disable"
"#;
    write!(temp_file, "{}", config_content).expect("設定書き込み");
    let path = temp_file.path().to_str().unwrap();

    // 設定読み込み
    let config = Config::load(path);
    assert!(config.is_ok(), "設定ファイルの読み込みが成功する");

    let config = config.unwrap();
    assert_eq!(config.connections.len(), 1, "接続設定が1つ存在する");
    assert_eq!(config.connections[0].name, "test-connection");
    assert_eq!(config.connections[0].postgres.host, "localhost");
    assert_eq!(config.connections[0].postgres.port, 5432);
    assert_eq!(config.connections[0].postgres.database, "testdb");
    assert_eq!(config.connections[0].postgres.user, "testuser");
    assert_eq!(config.connections[0].postgres.timeout, 15);
    assert_eq!(config.connections[0].postgres.ssl_mode, SslMode::Disable);
}

/// 空の結果セット処理のテスト
#[tokio::test]
async fn test_empty_result_set() {
    if skip_if_docker_unavailable("test_empty_result_set") {
        return;
    }

    let docker = Cli::default();
    let postgres_container = docker.run(Postgres::default());
    let host = "127.0.0.1";
    let port = postgres_container.get_host_port_ipv4(5432);

    let config = create_test_postgres_config(host, port);
    let manager = ConnectionManager::connect(config, false)
        .await
        .expect("testcontainers で起動した PostgreSQL へ接続できるはず");
    let pool = manager.pool();

    // 空のテーブルを作成
    let create_sql = "CREATE TABLE IF NOT EXISTS empty_table (id INT PRIMARY KEY)";
    sqlx::query(create_sql).execute(pool).await.unwrap();

    // 空の結果セットをSELECT
    let select_sql = "SELECT * FROM empty_table";
    let query_result = query::execute_query(pool, select_sql, None).await;
    assert!(query_result.is_ok(), "空の結果セットでもエラーにならない");

    let query_result = query_result.unwrap();
    assert_eq!(
        query_result.columns,
        vec!["id".to_string()],
        "空結果でも列情報は保持される"
    );
    assert_eq!(query_result.row_count(), 0, "行数は0");
    assert_eq!(query_result.rows.len(), 0, "rowsは空");

    manager.close().await;
}

/// クエリタイムアウトのテスト
#[tokio::test]
async fn test_query_timeout() {
    if skip_if_docker_unavailable("test_query_timeout") {
        return;
    }

    let docker = Cli::default();
    let postgres_container = docker.run(Postgres::default());
    let host = "127.0.0.1";
    let port = postgres_container.get_host_port_ipv4(5432);

    let config = create_test_postgres_config(host, port);
    let manager = ConnectionManager::connect(config, false)
        .await
        .expect("testcontainers で起動した PostgreSQL へ接続できるはず");
    let pool = manager.pool();

    // 短いタイムアウトでSLEEPクエリを実行（タイムアウトを期待）
    let sleep_sql = "SELECT pg_sleep(10)"; // 10秒スリープ（PostgreSQL形式）
    let timeout = Duration::from_secs(1); // 1秒でタイムアウト
    let query_result = query::execute_query_with_timeout(pool, sleep_sql, timeout, None).await;

    assert!(
        query_result.is_err(),
        "タイムアウト時間を超えたクエリはエラーになる"
    );

    manager.close().await;
}

/// NULL値を含むクエリ結果のテスト
#[tokio::test]
async fn test_null_values_in_result() {
    if skip_if_docker_unavailable("test_null_values_in_result") {
        return;
    }

    let docker = Cli::default();
    let postgres_container = docker.run(Postgres::default());
    let host = "127.0.0.1";
    let port = postgres_container.get_host_port_ipv4(5432);

    let config = create_test_postgres_config(host, port);
    let manager = ConnectionManager::connect(config, false)
        .await
        .expect("testcontainers で起動した PostgreSQL へ接続できるはず");
    let pool = manager.pool();

    // NULL値を含むテーブルを作成
    let create_sql = r"
        CREATE TABLE IF NOT EXISTS test_nulls (
            id INT PRIMARY KEY,
            nullable_field VARCHAR(50)
        )
    ";
    sqlx::query(create_sql).execute(pool).await.unwrap();

    // NULL値を挿入（PostgreSQLのプレースホルダーは $1, $2 形式）
    let insert_sql = "INSERT INTO test_nulls (id, nullable_field) VALUES ($1, $2)";
    sqlx::query(insert_sql)
        .bind(1_i32)
        .bind(Some("value"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(insert_sql)
        .bind(2_i32)
        .bind(None::<String>)
        .execute(pool)
        .await
        .unwrap();

    // SELECT
    let select_sql = "SELECT id, nullable_field FROM test_nulls ORDER BY id";
    let query_result = query::execute_query(pool, select_sql, None).await.unwrap();

    assert_eq!(query_result.row_count(), 2);
    assert_eq!(query_result.rows[0][1], "value");
    assert_eq!(
        query_result.rows[1][1], "NULL",
        "NULL値は\"NULL\"文字列として表示される"
    );

    manager.close().await;
}

/// bastion経由接続のテスト（SSH tunnel）
///
/// 注: このテストは実際のSSH bastionサーバーが必要なため、
/// 環境変数が設定されていない場合はスキップされる
#[tokio::test]
async fn test_bastion_connection() {
    // 環境変数からbastion設定を取得
    let bastion_host = std::env::var("POSHGRE_TEST_BASTION_HOST");
    let bastion_user = std::env::var("POSHGRE_TEST_BASTION_USER");
    let bastion_key_path = std::env::var("POSHGRE_TEST_BASTION_KEY");
    let postgres_internal_host = std::env::var("POSHGRE_TEST_POSTGRES_INTERNAL_HOST");

    if bastion_host.is_err()
        || bastion_user.is_err()
        || bastion_key_path.is_err()
        || postgres_internal_host.is_err()
    {
        eprintln!("Warning: bastion接続テストに必要な環境変数が設定されていません:");
        eprintln!("  POSHGRE_TEST_BASTION_HOST");
        eprintln!("  POSHGRE_TEST_BASTION_USER");
        eprintln!("  POSHGRE_TEST_BASTION_KEY");
        eprintln!("  POSHGRE_TEST_POSTGRES_INTERNAL_HOST");
        eprintln!("テストをスキップします。");
        return;
    }

    let bastion_host = bastion_host.unwrap();
    let bastion_user = bastion_user.unwrap();
    let bastion_key_path = bastion_key_path.unwrap();
    let postgres_internal_host = postgres_internal_host.unwrap();

    let config = ConnectionConfig {
        name: "bastion-test".to_string(),
        bastion: Some(BastionSetting::Config(BastionConfig {
            host: bastion_host,
            port: 22,
            user: bastion_user,
            key_path: Some(bastion_key_path),
        })),
        postgres: PostgresConfig {
            host: postgres_internal_host,
            port: 5432,
            database: "postgres".to_string(),
            user: "postgres".to_string(),
            password: Password::from(
                std::env::var("POSHGRE_TEST_POSTGRES_PASSWORD")
                    .unwrap_or_else(|_| "postgres".to_string()),
            ),
            timeout: 30,
            ssl_mode: SslMode::Disable, // bastion経由の場合は内部ネットワークでSSL不要な場合が多い
            pool: PoolConfigPartial::default(),
        },
        readonly: false,
    };

    // bastion経由でPostgreSQL接続
    let result = ConnectionManager::connect(config, false).await;
    assert!(result.is_ok(), "bastion経由でPostgreSQL接続が成功する");

    let manager = result.unwrap();
    let pool = manager.pool();

    // 基本的なクエリを実行
    let result = sqlx::query("SELECT 1 as value").fetch_one(pool).await;
    assert!(result.is_ok(), "bastion経由でクエリが実行できる");

    manager.close().await;
}

/// bastion設定が存在する場合の動作確認（モックテスト）
#[tokio::test]
async fn test_bastion_config_presence() {
    let config_with_bastion = ConnectionConfig {
        name: "test-with-bastion".to_string(),
        bastion: Some(BastionSetting::Config(BastionConfig {
            host: "bastion.example.com".to_string(),
            port: 22,
            user: "testuser".to_string(),
            key_path: Some("~/.ssh/id_rsa".to_string()),
        })),
        postgres: PostgresConfig {
            host: "postgres.internal".to_string(),
            port: 5432,
            database: "testdb".to_string(),
            user: "postgres".to_string(),
            password: Password::from("password"),
            timeout: 5,
            ssl_mode: SslMode::Disable,
            pool: PoolConfigPartial::default(),
        },
        readonly: false,
    };

    // bastion設定が存在することを確認
    assert!(config_with_bastion.bastion.is_some());
    match config_with_bastion.bastion.as_ref().unwrap() {
        BastionSetting::Config(bastion) => {
            assert_eq!(bastion.host, "bastion.example.com");
            assert_eq!(bastion.port, 22);
            assert_eq!(bastion.user, "testuser");
            assert_eq!(bastion.key_path, Some("~/.ssh/id_rsa".to_string()));
        }
        _ => panic!("Expected BastionSetting::Config"),
    }

    // 実際の接続は環境がないため試行しない（上記のtest_bastion_connectionで実施）
}
