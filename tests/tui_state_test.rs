use poshgre::config::{ConnectionConfig, Password, PoolConfigPartial, PostgresConfig, SslMode};
/// TUI状態遷移のテスト
///
/// Phase 3.3: テストカバレッジ向上のための状態遷移テスト
use poshgre::tui::AppState;

fn create_test_connection_config(name: &str) -> ConnectionConfig {
    ConnectionConfig {
        name: name.to_string(),
        bastion: None,
        postgres: PostgresConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "test".to_string(),
            user: "testuser".to_string(),
            password: Password::from("testpass"),
            timeout: 10,
            ssl_mode: SslMode::Disable,
            pool: PoolConfigPartial::default(),
        },
        readonly: false,
    }
}

#[test]
fn test_appstate_selecting_structure() {
    // Selecting状態の構造確認
    let connections = vec![
        create_test_connection_config("conn1"),
        create_test_connection_config("conn2"),
    ];

    let state = AppState::Selecting {
        connections: connections.clone(),
        selected_index: 0,
    };

    // パターンマッチで状態確認
    match state {
        AppState::Selecting {
            connections: c,
            selected_index: i,
        } => {
            assert_eq!(c.len(), 2);
            assert_eq!(i, 0);
        }
        _ => panic!("Expected Selecting state"),
    }
}

#[test]
fn test_appstate_error_structure() {
    // Error状態の構造確認
    let connections = vec![create_test_connection_config("conn1")];
    let previous_state = Box::new(AppState::Selecting {
        connections,
        selected_index: 0,
    });

    let error_state = AppState::Error {
        message: "テストエラー".to_string(),
        previous_state,
    };

    match error_state {
        AppState::Error {
            message,
            previous_state,
        } => {
            assert_eq!(message, "テストエラー");
            // previous_stateの検証
            match *previous_state {
                AppState::Selecting { .. } => {
                    // 期待通り
                }
                _ => panic!("Expected Selecting as previous state"),
            }
        }
        _ => panic!("Expected Error state"),
    }
}

#[test]
fn test_config_multiple_connections() {
    // 複数の接続設定を持つConfigのテスト
    let connections = [
        create_test_connection_config("dev"),
        create_test_connection_config("staging"),
        create_test_connection_config("prod"),
    ];

    assert_eq!(connections.len(), 3);
    assert_eq!(connections[0].name, "dev");
    assert_eq!(connections[1].name, "staging");
    assert_eq!(connections[2].name, "prod");
}

#[test]
fn test_state_transition_flow() {
    // 状態遷移フローの確認（概念的なテスト）

    // 1. 最初はSelecting状態
    let connections = vec![create_test_connection_config("conn1")];
    let initial_state = AppState::Selecting {
        connections: connections.clone(),
        selected_index: 0,
    };

    // Selecting状態であることを確認
    assert!(matches!(initial_state, AppState::Selecting { .. }));

    // 2. エラー発生時はError状態に遷移（previous_stateを保持）
    let error_state = AppState::Error {
        message: "接続に失敗しました".to_string(),
        previous_state: Box::new(initial_state),
    };

    assert!(matches!(error_state, AppState::Error { .. }));

    // 3. Error状態からprevious_stateに戻れることを確認
    match error_state {
        AppState::Error { previous_state, .. } => match *previous_state {
            AppState::Selecting { connections: c, .. } => {
                assert_eq!(c.len(), 1);
                assert_eq!(c[0].name, "conn1");
            }
            _ => panic!("Expected Selecting state"),
        },
        _ => panic!("Expected Error state"),
    }
}

#[test]
fn test_selected_index_bounds() {
    // selected_indexの範囲チェック（ロジックの検証）
    let connections = vec![
        create_test_connection_config("conn1"),
        create_test_connection_config("conn2"),
        create_test_connection_config("conn3"),
    ];

    // 有効なインデックス
    for i in 0..connections.len() {
        let state = AppState::Selecting {
            connections: connections.clone(),
            selected_index: i,
        };

        match state {
            AppState::Selecting {
                connections: c,
                selected_index: idx,
            } => {
                assert!(idx < c.len(), "selected_index should be within bounds");
            }
            _ => panic!("Expected Selecting state"),
        }
    }
}

#[test]
fn test_state_enum_variants() {
    // AppStateのすべてのバリアントが正しく構築できることを確認

    // Selecting
    let selecting = AppState::Selecting {
        connections: vec![create_test_connection_config("test")],
        selected_index: 0,
    };
    assert!(matches!(selecting, AppState::Selecting { .. }));

    // Executing
    let executing = AppState::Executing {
        query: "SELECT * FROM users".to_string(),
    };
    assert!(matches!(executing, AppState::Executing { .. }));

    // Error
    let error = AppState::Error {
        message: "エラーメッセージ".to_string(),
        previous_state: Box::new(selecting),
    };
    assert!(matches!(error, AppState::Error { .. }));
}
