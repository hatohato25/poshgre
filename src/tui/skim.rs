use ::skim::prelude::{
    unbounded, Skim, SkimItem, SkimItemReceiver, SkimItemSender, SkimOptionsBuilder,
};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::i18n::TuiMsg;
use crate::query::QueryResult;
use crate::t;

use super::{
    append_preview_to_chunk, build_preview_cmd, build_result_skim_options, calculate_column_widths,
    cleanup_preview_dir, determine_skim_action, flush_preview_chunk, format_row_display,
    preview_dir, read_row_from_chunk, App, AppState, ResultRowItem, SimpleSkimItem, SkimAction,
};

impl App {
    /// 表示系クエリをストリーミングモードに遷移させる
    ///
    /// Connected / ShowingResult 状態から manager を取り出して StreamingQuery 状態に遷移する。
    /// TUIループがこの状態を検出してから実際のストリーミング処理を開始する。
    pub(super) fn transition_to_streaming(&mut self) -> Result<()> {
        let manager = match std::mem::replace(
            &mut self.state,
            AppState::Executing {
                query: String::new(),
            },
        ) {
            AppState::Connected { manager } => manager,
            AppState::ShowingResult {
                manager: Some(manager),
                ..
            } => manager,
            other => {
                self.state = other;
                return Err(Error::Other("接続がありません".to_string()));
            }
        };

        // 次の show_result_with_skim でテーブル名を抽出できるよう保存する
        self.sql.last_sql = self.sql.text.clone();

        // タイムアウト値はmanagerのconfig経由で取得する（PostgreSQL設定を参照）
        let timeout_secs = manager.config().postgres.timeout;

        self.state = AppState::StreamingQuery {
            manager,
            sql: self.sql.text.clone(),
            timeout_secs,
        };

        Ok(())
    }

    /// カラム選択モードに遷移する
    ///
    /// Connected 状態から manager を取り出して SelectingColumns 状態に遷移する。
    /// TUIループがこの状態を検出してから実際のカラム選択処理を開始する。
    pub(super) fn transition_to_column_select(&mut self) -> Result<()> {
        let manager = match std::mem::replace(
            &mut self.state,
            AppState::Selecting {
                connections: Vec::new(),
                selected_index: 0,
            },
        ) {
            AppState::Connected { manager } => manager,
            other => {
                self.state = other;
                return Err(Error::Other("接続がありません".to_string()));
            }
        };

        let timeout_secs = manager.config().postgres.timeout;

        self.state = AppState::SelectingColumns {
            manager,
            timeout_secs,
        };

        Ok(())
    }

    /// skimベースのインタラクティブカラム選択
    ///
    /// 1. pg_tablesからテーブル一覧取得 → skimで1つ選択
    /// 2. information_schema.columnsからカラム一覧取得 → skimでTab複数選択
    /// 3. SELECT カラム FROM テーブル名 を返す
    ///
    /// この関数は TUI 一時停止中（AlternateScreen 離脱後）に呼ばれる想定。
    /// skim はブロッキングなので、show_result_streaming と同様に
    /// std::thread::spawn + block_on パターンで DB クエリを実行する。
    pub(super) fn select_columns_interactive(
        &self,
        pool: &sqlx::Pool<sqlx::Postgres>,
        _timeout: std::time::Duration,
        _current_database: Option<&str>,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> Result<Option<String>> {
        let rt_handle = tokio::runtime::Handle::current();
        let pool = pool.clone();

        // Step 1: pg_catalog.pg_tables でテーブル一覧取得
        // PostgreSQL では接続ごとにDBが固定されるため、スキーマ名でフィルタして取得する
        // skimはブロッキングのため、tokioランタイムのハンドルを用いて別スレッドで非同期クエリを実行する
        let tables: Vec<String> = {
            let pool_clone = pool.clone();
            let rt_handle_clone = rt_handle.clone();
            let handle = std::thread::spawn(move || {
                rt_handle_clone.block_on(async {
                    use sqlx::Row;
                    let rows = sqlx::query(
                        "SELECT tablename FROM pg_catalog.pg_tables \
                         WHERE schemaname = current_schema() \
                         ORDER BY tablename",
                    )
                    .fetch_all(&pool_clone)
                    .await
                    .map_err(Error::QueryExecution)?;
                    let tables: Vec<String> = rows
                        .iter()
                        .map(|row| row.try_get::<String, _>(0).unwrap_or_default())
                        .collect();
                    Ok::<_, Error>(tables)
                })
            });
            match handle.join() {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    return Err(Error::Other(
                        "テーブル一覧取得スレッドがパニックしました".to_string(),
                    ))
                }
            }
        };

        if tables.is_empty() {
            return Err(Error::Other("テーブルが見つかりません".to_string()));
        }

        // テーブル一覧取得完了後・skim起動直前にTUIを離脱してちらつきを防ぐ
        crossterm::terminal::disable_raw_mode()
            .map_err(|e| Error::Tui(format!("ターミナル復元失敗: {}", e)))?;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        )
        .map_err(|e| Error::Tui(format!("ターミナル復元失敗: {}", e)))?;

        // Step 2: skimでテーブル選択（single select）
        let table_name = {
            let table_select_prompt = t!(TuiMsg::QueryResultPrompt);
            let options = SkimOptionsBuilder::default()
                .height(Some("100%"))
                .multi(false)
                .reverse(true)
                .prompt(Some(&table_select_prompt))
                .no_mouse(true)
                .build()
                .map_err(|e| Error::Other(format!("{}: {:?}", t!(TuiMsg::SkimInitError), e)))?;

            let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
            for table in &tables {
                let _ = tx.send(Arc::new(SimpleSkimItem(table.clone())) as Arc<dyn SkimItem>);
            }
            drop(tx);

            let output = Skim::run_with(&options, Some(rx));
            let Some(output) = output else {
                return Ok(None);
            };
            if output.is_abort {
                return Ok(None);
            }
            let Some(selected) = output.selected_items.first() else {
                return Ok(None);
            };
            selected.output().to_string()
        };

        // Step 3: information_schema.columns でカラム一覧取得
        // PostgreSQL では接続ごとにDBが固定されているため、table_name でフィルタするだけでよい
        let columns: Vec<String> = {
            let pool_clone = pool.clone();
            let table_name_clone = table_name.clone();
            let rt_handle_clone = rt_handle.clone();
            let handle = std::thread::spawn(move || {
                rt_handle_clone.block_on(async {
                    use sqlx::Row;
                    let rows = sqlx::query(
                        "SELECT column_name FROM information_schema.columns \
                         WHERE table_name = $1 ORDER BY ordinal_position",
                    )
                    .bind(&table_name_clone)
                    .fetch_all(&pool_clone)
                    .await
                    .map_err(Error::QueryExecution)?;
                    let columns: Vec<String> = rows
                        .iter()
                        .map(|row| row.try_get::<String, _>(0).unwrap_or_default())
                        .collect();
                    Ok::<_, Error>(columns)
                })
            });
            match handle.join() {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    return Err(Error::Other(
                        "カラム一覧取得スレッドがパニックしました".to_string(),
                    ))
                }
            }
        };

        if columns.is_empty() {
            return Err(Error::Other(t!(TuiMsg::NoColumnsFound {
                table: &table_name
            })));
        }

        // Step 4: skimでカラム複数選択（multi select、Tab で複数選択）
        let selected_columns = {
            let prompt = t!(TuiMsg::ColumnSelectPrompt { table: &table_name });
            let options = SkimOptionsBuilder::default()
                .height(Some("100%"))
                .multi(true)
                .reverse(true)
                .prompt(Some(&prompt))
                .no_mouse(true)
                .build()
                .map_err(|e| Error::Other(format!("{}: {:?}", t!(TuiMsg::SkimInitError), e)))?;

            let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
            for col in &columns {
                let _ = tx.send(Arc::new(SimpleSkimItem(col.clone())) as Arc<dyn SkimItem>);
            }
            drop(tx);

            let output = Skim::run_with(&options, Some(rx));
            let Some(output) = output else {
                return Ok(None);
            };
            if output.is_abort {
                return Ok(None);
            }
            if output.selected_items.is_empty() {
                return Ok(None);
            }

            output
                .selected_items
                .iter()
                .map(|item| item.output().to_string())
                .collect::<Vec<String>>()
        };

        // Step 5: SELECT文を生成
        // PostgreSQL では接続ごとにDBが固定されるため、スキーマを省略してテーブル名だけで参照する
        let col_list = selected_columns
            .iter()
            .map(|c| crate::query::escape_identifier(c))
            .collect::<Vec<_>>()
            .join(", ");
        let qualified_table = crate::query::escape_identifier(&table_name);
        let sql = format!("SELECT {} FROM {}", col_list, qualified_table);

        Ok(Some(sql))
    }

    // PostgreSQLでは接続ごとにDBが固定されるため、USE によるDB切り替えは不要。
    // current_database は接続確立時に設定され、以後変更されない。

    /// skimを使って結果を表示
    ///
    /// ratatuiのAlternateScreenを一時的に離れてskimを起動し、
    /// ユーザーが行を選択した場合は対応するSQLを構築して返す。
    /// ESC/Ctrl-Cの場合はNoneを返す。
    pub(super) fn show_result_with_skim(
        &mut self,
        result: &QueryResult,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> Result<Option<SkimAction>> {
        // 多カラム表示で見切れる問題に対処するため、プレビューペインに選択行の全カラムを縦表示する。
        // {n} はskimのフィルタ後0ベースインデックスに置換されるため、元データとのマッピングには
        // 行インデックスプレフィックスを付けて一時ファイル参照で対応する。

        // プレビュー用チャンクディレクトリを作成
        let pdir = preview_dir();
        if let Err(e) = std::fs::create_dir_all(&pdir) {
            // ディレクトリ作成失敗時はプレビューが表示されないだけで致命的ではないためwarnログに留める
            tracing::warn!("プレビュー用ディレクトリの作成に失敗しました: {}", e);
        }

        // 各行のプレビューデータをチャンクファイルに書き出す
        let mut chunk_buf = String::new();
        for (row_index, row) in result.rows.iter().enumerate() {
            append_preview_to_chunk(&pdir, row_index, &result.columns, row, &mut chunk_buf);
        }
        flush_preview_chunk(&pdir, result.rows.len().saturating_sub(1), &chunk_buf);

        // カラム幅を計算
        let col_widths = calculate_column_widths(&result.columns, &result.rows);

        // 各行をSkimItemに変換
        let items: Vec<Arc<dyn SkimItem>> = result
            .rows
            .iter()
            .enumerate()
            .map(|(row_index, row)| {
                let display = format_row_display(row, &col_widths);
                Arc::new(ResultRowItem { row_index, display }) as Arc<dyn SkimItem>
            })
            .collect();

        // ヘッダー行を作成
        let header_line = format_row_display(&result.columns, &col_widths);

        // previewコマンド: チャンクファイルから該当行を高速検索
        // self.sql.last_sql からテーブル名を抽出してプレビューのヘッダーに表示する
        let table_name = crate::completion::extract_from_table(&self.sql.last_sql);
        // パンくずリスト用にテーブル名を更新する
        self.current_table = table_name.clone();
        let preview_cmd = build_preview_cmd(&pdir, table_name.as_deref());
        let prompt_str = t!(TuiMsg::QueryResultPrompt);

        let options = build_result_skim_options(&header_line, &preview_cmd, &prompt_str)?;

        let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
        for item in items {
            let _ = tx.send(item);
        }
        drop(tx);

        // データ準備完了後・skim起動直前にTUIを離脱することでちらつきを防ぐ
        crossterm::terminal::disable_raw_mode()
            .map_err(|e| crate::error::Error::Tui(format!("ターミナル復元失敗: {}", e)))?;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        )
        .map_err(|e| crate::error::Error::Tui(format!("ターミナル復元失敗: {}", e)))?;

        let skim_output = Skim::run_with(&options, Some(rx));

        // skim終了後にプレビューディレクトリを削除
        cleanup_preview_dir(&pdir);

        let Some(output) = skim_output else {
            return Ok(None);
        };

        if output.is_abort {
            return Ok(None);
        }

        let Some(selected_item) = output.selected_items.first() else {
            return Ok(None);
        };

        // ResultRowItemにダウンキャスト
        let result_item = selected_item
            .as_ref()
            .as_any()
            .downcast_ref::<ResultRowItem>()
            .ok_or_else(|| Error::Other("選択された行の情報を復元できません".to_string()))?;

        // 選択された行のデータを取得
        let row_data = result
            .rows
            .get(result_item.row_index)
            .ok_or_else(|| Error::Other("選択された行が見つかりません".to_string()))?;

        if row_data.is_empty() {
            return Ok(None);
        }

        let first_value = &row_data[0];

        // NULLまたは空の値の場合は何もしない
        if first_value == "NULL" || first_value.trim().is_empty() {
            return Ok(None);
        }

        let first_column = result.columns.first().map(|s| s.as_str()).unwrap_or("");
        let action = determine_skim_action(
            first_column,
            first_value,
            &result.columns,
            row_data,
            &self.sql.last_sql,
        );

        Ok(Some(action))
    }

    /// DBからストリーミングしながらskimに行を送り続け、ユーザーの選択を返す
    ///
    /// 全件取得を待たずに最初の行が取れた時点でskimが起動するため、
    /// 数百万行のテーブルでも数秒以内に操作を開始できる。
    /// skimが閉じられた（ESC/Ctrl-C）タイミングでDBフェッチも中断する。
    pub(super) fn show_result_streaming(
        &mut self,
        pool: sqlx::Pool<sqlx::Postgres>,
        sql: &str,
        query_timeout: std::time::Duration,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> Result<Option<SkimAction>> {
        use std::sync::mpsc;

        let (skim_tx, skim_rx): (SkimItemSender, SkimItemReceiver) = unbounded();

        // バックグラウンドスレッドからカラム情報とカラム幅、または初期フェッチエラーを受け取るチャネル
        // 最初の100行をサンプリングしてカラム幅を決定してから送信する
        // エラー発生時はErrを送信し、skim起動前にエラーを検出できるようにする
        let (col_tx, col_rx) =
            mpsc::channel::<std::result::Result<(Vec<String>, Vec<usize>), String>>();

        // プレビュー用チャンクディレクトリ
        let pdir = preview_dir();
        if let Err(e) = std::fs::create_dir_all(&pdir) {
            // ディレクトリ作成失敗時はプレビューが表示されないだけで致命的ではないためwarnログに留める
            tracing::warn!("プレビュー用ディレクトリの作成に失敗しました: {}", e);
        }
        let pdir_clone = pdir.clone();

        let sql_owned = sql.to_string();

        // バックグラウンドスレッドでDBストリーミング + skimチャネル送信
        // sqlxの接続プールは元のtokioランタイムに紐づいているため、
        // 同じランタイムのハンドルを使ってasync処理を実行する
        // PostgreSQL では接続ごとにDBが固定されるため、USE 先行実行は不要
        let rt_handle = tokio::runtime::Handle::current();
        let handle = std::thread::spawn(move || {
            rt_handle.block_on(async {
                use futures::StreamExt;
                use sqlx::{Column as SqlxColumn, Row as SqlxRow};

                // プールから直接ストリーミング（PostgreSQL はセッション状態に依存しない）
                let mut stream = sqlx::query(&sql_owned).fetch(&pool);
                let mut columns: Vec<String> = Vec::new();
                let mut row_index: usize = 0;
                let mut chunk_buf = String::new();

                // Phase 1: 最初の100行をバッファしてカラム幅を決定
                // タイムアウトは最初の行が来るまでの接続確認として適用する
                // データが流れ始めた後はユーザーがESCで中断できるため不要
                let sample_size = 100;
                let mut sample_rows: Vec<Vec<String>> = Vec::new();

                loop {
                    // タイムアウト付きで次の行を取得する
                    // タイムアウト経過はサーバー無応答またはクエリ自体のエラーを示す
                    let timeout_result = tokio::time::timeout(query_timeout, stream.next()).await;

                    let row_option = match timeout_result {
                        Err(_elapsed) => {
                            // タイムアウト：サーバーからの応答が設定時間内に来なかった
                            let _ = col_tx.send(Err(
                                "クエリのタイムアウト：サーバーからの応答がありませんでした"
                                    .to_string(),
                            ));
                            return Vec::new();
                        }
                        Ok(opt) => opt,
                    };

                    let row_result = match row_option {
                        None => break, // ストリーム終了（結果が空またはsample_size未満）
                        Some(r) => r,
                    };

                    let row = match row_result {
                        Ok(r) => r,
                        Err(e) => {
                            // SQLエラー（構文エラー・権限エラー等）をメインスレッドに通知
                            tracing::error!("Streaming query error: {}", e);
                            let _ = col_tx.send(Err(format!("クエリ実行エラー: {}", e)));
                            return Vec::new();
                        }
                    };

                    if columns.is_empty() {
                        columns = row.columns().iter().map(|c| c.name().to_string()).collect();
                    }

                    let data: Vec<String> = row
                        .columns()
                        .iter()
                        .enumerate()
                        .map(|(i, col)| crate::query::convert_value_to_string(&row, i, col))
                        .collect();

                    sample_rows.push(data);

                    if sample_rows.len() >= sample_size {
                        break;
                    }
                }

                // 空結果でも列ヘッダーを表示するためにメタデータを補完する
                // execute_query 側と同じ describe() を使って一貫性を保つ
                if sample_rows.is_empty() && columns.is_empty() {
                    use sqlx::{Column as SqlxColumnDesc, Executor};
                    match pool.describe(sql_owned.as_str()).await {
                        Ok(describe) => {
                            columns = describe
                                .columns()
                                .iter()
                                .map(|c: &sqlx::postgres::PgColumn| c.name().to_string())
                                .collect();
                        }
                        Err(e) => {
                            tracing::warn!("describe failed for empty streaming result: {}", e);
                        }
                    }
                }

                // カラム幅を計算してメインスレッドに通知（成功）
                let col_widths = calculate_column_widths(&columns, &sample_rows);
                let _ = col_tx.send(Ok((columns.clone(), col_widths.clone())));

                // バッファした行をskimに送信
                for data in &sample_rows {
                    let display = format_row_display(data, &col_widths);

                    let item = Arc::new(ResultRowItem { row_index, display }) as Arc<dyn SkimItem>;
                    if skim_tx.send(item).is_err() {
                        // skimが閉じたらチャンクファイルに書き出して終了する
                        flush_preview_chunk(&pdir_clone, row_index.saturating_sub(1), &chunk_buf);
                        return columns;
                    }

                    append_preview_to_chunk(&pdir_clone, row_index, &columns, data, &mut chunk_buf);

                    row_index += 1;
                }

                // Phase 2: 残りの行をストリーミング送信
                // データが流れ始めたため、ここではタイムアウトを適用しない（ユーザーがESCで中断できる）
                while let Some(row_result) = stream.next().await {
                    let row = match row_result {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::error!("Streaming query error: {}", e);
                            break;
                        }
                    };

                    let data: Vec<String> = row
                        .columns()
                        .iter()
                        .enumerate()
                        .map(|(i, col)| crate::query::convert_value_to_string(&row, i, col))
                        .collect();

                    let display = format_row_display(&data, &col_widths);

                    let item = Arc::new(ResultRowItem { row_index, display }) as Arc<dyn SkimItem>;
                    if skim_tx.send(item).is_err() {
                        tracing::debug!(
                            "skim channel closed, stopping stream at row {}",
                            row_index
                        );
                        break;
                    }

                    append_preview_to_chunk(
                        &pdir_clone,
                        row_index,
                        &columns,
                        &data,
                        &mut chunk_buf,
                    );

                    row_index += 1;
                }

                // 最終チャンクを書き出し
                flush_preview_chunk(&pdir_clone, row_index.saturating_sub(1), &chunk_buf);

                columns
            })
        });

        // カラム情報とカラム幅を待つ（最初の100行サンプリング後に届く）
        // SQLエラー（構文エラー・権限エラー等）が発生した場合はErrが届くので、skim起動前にエラーを伝播する
        let col_result = col_rx.recv().map_err(|_| {
            // バックグラウンドスレッドが何も送信せずに終了した場合（結果が空の場合）
            // これは正常ケース（空のSELECT結果等）ではなく、スレッド異常終了を示す
            Error::Other("ストリーミングスレッドが予期せず終了しました".to_string())
        })?;

        let (columns, col_widths) = col_result.map_err(|e| {
            // SQLエラー・タイムアウトエラーをAppState::Errorに遷移させるためErr(Error::...)として返す
            Error::Other(e)
        })?;

        // ヘッダー行（動的カラム幅でフォーマット）
        let header_line = if columns.is_empty() {
            "(結果なし)".to_string()
        } else {
            format_row_display(&columns, &col_widths)
        };

        // previewコマンド: チャンクファイルから該当行を高速検索
        // 引数 sql からテーブル名を抽出してプレビューのヘッダーに表示する
        let table_name = crate::completion::extract_from_table(sql);
        // パンくずリスト用にテーブル名を更新する
        self.current_table = table_name.clone();
        let preview_cmd = build_preview_cmd(&pdir, table_name.as_deref());
        let prompt_str = t!(TuiMsg::QueryResultPrompt);

        let options = build_result_skim_options(&header_line, &preview_cmd, &prompt_str)?;

        // サンプリング完了後・skim起動直前にTUIを離脱することでちらつきを防ぐ
        // LeaveAlternateScreen を skim 起動直前まで遅延させることで素のターミナルが見える時間をゼロにする
        crossterm::terminal::disable_raw_mode()
            .map_err(|e| crate::error::Error::Tui(format!("ターミナル復元失敗: {}", e)))?;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        )
        .map_err(|e| crate::error::Error::Tui(format!("ターミナル復元失敗: {}", e)))?;

        // skim_rxの送信側はバックグラウンドスレッドが保持しているため、skimが閉じると自動的にストリームが終了する
        let skim_output = Skim::run_with(&options, Some(skim_rx));

        // バックグラウンドスレッドの終了を待つ
        // skimが閉じると skim_tx.send() が Err を返してバックグラウンドスレッドがbreakする
        let columns = match handle.join() {
            Ok(data) => data,
            Err(e) => {
                // パニックは通常発生しないが、発生した場合もUIを継続できるようエラーログに留める
                tracing::error!("ストリーミングスレッドがパニックしました: {:?}", e);
                Vec::new()
            }
        };

        // キャンセルされた場合
        let Some(output) = skim_output else {
            cleanup_preview_dir(&pdir);
            return Ok(None);
        };

        if output.is_abort {
            cleanup_preview_dir(&pdir);
            return Ok(None);
        }

        let Some(selected_item) = output.selected_items.first() else {
            cleanup_preview_dir(&pdir);
            return Ok(None);
        };

        // ResultRowItemにダウンキャスト
        let result_item = selected_item
            .as_ref()
            .as_any()
            .downcast_ref::<ResultRowItem>()
            .ok_or_else(|| Error::Other("選択された行の情報を復元できません".to_string()))?;

        // チャンクファイルから行データを読み出す
        // all_rows をメモリ上に保持する代わりにチャンクファイルを参照することで OOM を防ぐ
        let row_data = read_row_from_chunk(&pdir, result_item.row_index, &columns)?;

        // read_row_from_chunk が参照するチャンクファイルを使い終わったため、ここで削除する
        cleanup_preview_dir(&pdir);

        let first_value = row_data.first().map(|s| s.as_str()).unwrap_or("");

        if first_value.is_empty() || first_value == "NULL" {
            return Ok(None);
        }

        let first_column = columns.first().map(|s| s.as_str()).unwrap_or("");
        let action = determine_skim_action(first_column, first_value, &columns, &row_data, sql);

        Ok(Some(action))
    }
}
