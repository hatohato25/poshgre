use skim::prelude::*;
use std::borrow::Cow;
use std::sync::Arc;

use crate::config::ConnectionConfig;
use crate::error::{Error, Result};
use crate::i18n::TuiMsg;
use crate::t;

/// skim の選択肢に元の接続インデックスを保持する
struct ConnectionItem {
    index: usize,
    display: String,
}

impl SkimItem for ConnectionItem {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.display)
    }
}

fn format_connection_display(conn: &ConnectionConfig) -> String {
    conn.name.clone()
}

fn connection_from_selected_item(
    selected: &Arc<dyn SkimItem>,
    connections: &[ConnectionConfig],
) -> Result<ConnectionConfig> {
    let selected_item = selected
        .as_ref()
        .as_any()
        .downcast_ref::<ConnectionItem>()
        .ok_or_else(|| Error::Other("選択された接続先の情報を復元できません".to_string()))?;

    connections
        .get(selected_item.index)
        .cloned()
        .ok_or_else(|| Error::Other("選択された接続先が見つかりません".to_string()))
}

/// 接続先を選択
///
/// skimを使用してfzf風の絞り込み選択UIを提供
pub fn select_connection(connections: &[ConnectionConfig]) -> Result<ConnectionConfig> {
    if connections.is_empty() {
        return Err(Error::config("接続先が定義されていません"));
    }

    // 接続先リストを文字列化してSkimItemに変換
    let items: Vec<Arc<dyn SkimItem>> = connections
        .iter()
        .enumerate()
        .map(|(index, conn)| {
            Arc::new(ConnectionItem {
                index,
                display: format_connection_display(conn),
            }) as Arc<dyn SkimItem>
        })
        .collect();

    // skimオプション設定
    // SkimOptions が &str を要求するため、変数に束縛してからプロンプトに渡す
    let prompt = t!(TuiMsg::SelectConnectionPrompt);
    let options = SkimOptionsBuilder::default()
        .height(Some("100%"))
        .multi(false)
        .reverse(true)
        .prompt(Some(&prompt))
        .no_mouse(true)
        .build()
        .map_err(|e| Error::Other(format!("{}: {:?}", t!(TuiMsg::SkimInitError), e)))?;

    // チャネル作成
    let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
    for item in items {
        let _ = tx.send(item);
    }
    drop(tx);

    // skim実行
    let skim_output = Skim::run_with(&options, Some(rx))
        .ok_or_else(|| Error::Other("接続先の選択がキャンセルされました".to_string()))?;

    if skim_output.is_abort {
        return Err(Error::Other(
            "接続先の選択がキャンセルされました".to_string(),
        ));
    }

    // 選択されたアイテムのインデックスを取得
    let selected = skim_output
        .selected_items
        .first()
        .ok_or_else(|| Error::Other("接続先が選択されていません".to_string()))?;

    connection_from_selected_item(selected, connections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BastionConfig, BastionSetting, Password, PoolConfigPartial, PostgresConfig, SslMode,
    };

    fn create_test_connection(name: &str) -> ConnectionConfig {
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

    fn create_test_connection_with_bastion(name: &str) -> ConnectionConfig {
        ConnectionConfig {
            name: name.to_string(),
            bastion: Some(BastionSetting::Config(BastionConfig {
                host: "bastion.example.com".to_string(),
                port: 22,
                user: "bastionuser".to_string(),
                key_path: Some("~/.ssh/id_rsa".to_string()),
            })),
            postgres: PostgresConfig {
                host: "postgres.internal".to_string(),
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
    fn test_select_connection_empty() {
        let connections = vec![];
        let result = select_connection(&connections);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("接続先が定義されていません"));
    }

    #[test]
    fn test_connection_display_format_direct() {
        let conn = create_test_connection("test-connection");
        let display = format_connection_display(&conn);

        assert_eq!(display, "test-connection");
    }

    #[test]
    fn test_connection_display_format_with_bastion() {
        let conn = create_test_connection_with_bastion("prod-connection");
        let display = format_connection_display(&conn);

        assert_eq!(display, "prod-connection");
    }

    #[test]
    fn test_skim_item_conversion() {
        let connections = [
            create_test_connection("conn1"),
            create_test_connection("conn2"),
            create_test_connection("conn3"),
        ];

        // SkimItem変換のロジック確認
        let items: Vec<String> = connections.iter().map(format_connection_display).collect();

        assert_eq!(items.len(), 3);
        assert!(items[0].starts_with("conn1"));
        assert!(items[1].starts_with("conn2"));
        assert!(items[2].starts_with("conn3"));
    }

    #[test]
    fn test_connection_item_downcast_uses_index() {
        let connections = [
            create_test_connection("conn1"),
            create_test_connection("conn2"),
        ];

        let selected: Arc<dyn SkimItem> = Arc::new(ConnectionItem {
            index: 1,
            display: format_connection_display(&connections[1]),
        });

        let found = connection_from_selected_item(&selected, &connections);

        assert!(found.is_ok());
        assert_eq!(found.unwrap().name, "conn2");
    }

    #[test]
    fn test_connection_selection_handles_prefix_names() {
        let connections = [
            create_test_connection("prod"),
            create_test_connection("prod-readonly"),
        ];

        let selected: Arc<dyn SkimItem> = Arc::new(ConnectionItem {
            index: 1,
            display: format_connection_display(&connections[1]),
        });

        let found = connection_from_selected_item(&selected, &connections);

        assert!(found.is_ok());
        assert_eq!(found.unwrap().name, "prod-readonly");
    }

    // Note: 実際のskim UIテストは統合テストで実施
    // ここではエラーケースと表示フォーマットのテストのみ
}
