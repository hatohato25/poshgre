use crate::completion::{CompletionCache, CompletionItem};
use ::skim::prelude::*;
use crossterm::{
    event::{self},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::task::JoinHandle;
use unicode_width::UnicodeWidthStr;

use crate::config::{BastionConfig, BastionSetting, Config};
use crate::connection::ConnectionManager;
use crate::error::{Error, Result};
use crate::i18n::TuiMsg;
use crate::query::QueryResult;
use crate::t;

mod input;
mod render;
mod skim;

/// skimでレコードを選択した際の返却アクション
pub(super) enum SkimAction {
    /// ドリルダウン: SELECT FROM を実行する（データベース/テーブル一覧用）
    DrillDown(String),
    /// レコード選択: WHERE テンプレートとレコード詳細を返す（通常 SELECT 結果用）
    SelectRecord {
        where_template: String,
        record: SelectedRecord,
    },
}

/// 選択されたレコードの詳細情報（SQL入力画面でプレビュー表示用）
#[derive(Debug, Clone)]
pub(super) struct SelectedRecord {
    /// カラム名と値のペア
    columns: Vec<(String, String)>,
}

/// 単純な文字列をskimアイテムとして使うためのラッパー
///
/// String は SkimItem を直接実装していないため、このラッパーを使って
/// テーブル名・カラム名のリストをskimに渡す。
struct SimpleSkimItem(String);

impl SkimItem for SimpleSkimItem {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.0)
    }
}

/// skim結果表示用のアイテム
struct ResultRowItem {
    row_index: usize,
    display: String,
}

impl SkimItem for ResultRowItem {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.display)
    }

    fn output(&self) -> Cow<'_, str> {
        // previewコマンドの {} 置換で行インデックスのみを渡す
        // テキストを含めるとパイプ等の特殊文字がシェルパースエラーを起こすため
        Cow::Owned(self.row_index.to_string())
    }
}

/// 文字列を表示幅ベースで固定幅にパディングする
///
/// 全角文字（2セル幅）を考慮し、ターミナル上で正しく列が揃うようにする。
/// 表示幅がtarget_widthを超える場合は切り詰めて"..."を付与する。
pub(super) fn pad_to_width(s: &str, target_width: usize) -> String {
    let display_width = UnicodeWidthStr::width(s);
    if display_width > target_width {
        // 表示幅ベースで切り詰め
        let mut truncated = String::new();
        let mut w = 0;
        let suffix = "...";
        let suffix_width = 3;
        let max_content_width = target_width.saturating_sub(suffix_width);
        for ch in s.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if w + ch_width > max_content_width {
                break;
            }
            truncated.push(ch);
            w += ch_width;
        }
        truncated.push_str(suffix);
        // 切り詰め後の表示幅を再計算してパディング
        let truncated_width = UnicodeWidthStr::width(truncated.as_str());
        let padding = target_width.saturating_sub(truncated_width);
        format!("{}{}", truncated, " ".repeat(padding))
    } else {
        let padding = target_width.saturating_sub(display_width);
        format!("{}{}", s, " ".repeat(padding))
    }
}

/// データからカラムごとの最適な表示幅を計算する
///
/// カラム名と各行のデータの表示幅を比較し、最大幅を返す。
/// 最小幅4、最大幅40でクランプする。
pub(super) fn calculate_column_widths(columns: &[String], rows: &[Vec<String>]) -> Vec<usize> {
    let max_width = 40;
    let min_width = 4;

    let mut widths: Vec<usize> = columns
        .iter()
        .map(|col| UnicodeWidthStr::width(col.as_str()))
        .collect();

    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                let cell_width = UnicodeWidthStr::width(cell.as_str());
                if cell_width > widths[i] {
                    widths[i] = cell_width;
                }
            }
        }
    }

    widths
        .iter()
        .map(|&w| w.clamp(min_width, max_width))
        .collect()
}

/// プレビュー用チャンクファイルのディレクトリパスを生成
pub(super) fn preview_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("poshgre_preview_{}", std::process::id()))
}

const PREVIEW_CHUNK_SIZE: usize = 1000;

/// SQL実行履歴の最大保持件数
///
/// 超過した場合は最古のエントリを削除する
pub(super) const MAX_SQL_HISTORY: usize = 100;

/// プレビューデータを1行分チャンクバッファに追加し、チャンク境界でファイルに書き出す
///
/// チャンクファイル: preview_dir/chunk_0.txt (行0-999), chunk_1.txt (行1000-1999), ...
/// 各チャンク内は `---\n` 区切りのセクション形式
pub(super) fn append_preview_to_chunk(
    dir: &std::path::Path,
    row_index: usize,
    columns: &[String],
    data: &[String],
    chunk_buf: &mut String,
) {
    for (col_idx, cell) in data.iter().enumerate() {
        let col_name = columns.get(col_idx).map(|s| s.as_str()).unwrap_or("?");
        chunk_buf.push_str(col_name);
        chunk_buf.push_str(": ");
        chunk_buf.push_str(cell);
        chunk_buf.push('\n');
    }
    chunk_buf.push_str("---\n");

    // チャンク境界に達したらファイルに書き出してバッファをクリア
    if (row_index + 1) % PREVIEW_CHUNK_SIZE == 0 {
        let chunk_idx = row_index / PREVIEW_CHUNK_SIZE;
        if let Err(e) = std::fs::write(
            dir.join(format!("chunk_{}.txt", chunk_idx)),
            chunk_buf.as_str(),
        ) {
            // 書き込み失敗時はプレビューが表示されないだけで致命的ではないためwarnログに留める
            tracing::warn!(
                "プレビューチャンクの書き込みに失敗しました (chunk={}): {}",
                chunk_idx,
                e
            );
        }
        chunk_buf.clear();
    }
}

/// チャンクバッファの残りをファイルに書き出す（最終チャンク）
pub(super) fn flush_preview_chunk(dir: &std::path::Path, row_index: usize, chunk_buf: &str) {
    if !chunk_buf.is_empty() {
        let chunk_idx = row_index / PREVIEW_CHUNK_SIZE;
        if let Err(e) = std::fs::write(dir.join(format!("chunk_{}.txt", chunk_idx)), chunk_buf) {
            // 書き込み失敗時はプレビューが表示されないだけで致命的ではないためwarnログに留める
            tracing::warn!(
                "最終プレビューチャンクの書き込みに失敗しました (chunk={}): {}",
                chunk_idx,
                e
            );
        }
    }
}

/// チャンクファイルから指定行のデータを読み出す
///
/// `append_preview_to_chunk` が書き出すフォーマット（`col_name: value\n` + `---\n` 区切り）を
/// パースして各カラムの値を Vec<String> として返す。
/// `all_rows` をメモリ上に保持せずに済むため、数百万行のクエリ結果でも OOM にならない。
pub(super) fn read_row_from_chunk(
    dir: &std::path::Path,
    row_index: usize,
    columns: &[String],
) -> crate::error::Result<Vec<String>> {
    let chunk_idx = row_index / PREVIEW_CHUNK_SIZE;
    let row_in_chunk = row_index % PREVIEW_CHUNK_SIZE;

    let chunk_path = dir.join(format!("chunk_{}.txt", chunk_idx));
    let content = std::fs::read_to_string(&chunk_path).map_err(|e| {
        crate::error::Error::Other(format!(
            "チャンクファイルの読み込みに失敗しました (row={}): {}",
            row_index, e
        ))
    })?;

    // `---\n` で区切られたレコードの中から対象行を取得する
    let records: Vec<&str> = content.split("---\n").filter(|r| !r.is_empty()).collect();
    let record = records.get(row_in_chunk).ok_or_else(|| {
        crate::error::Error::Other(format!(
            "選択された行がチャンクファイル内に見つかりません (row={}, chunk={})",
            row_index, chunk_idx
        ))
    })?;

    // `col_name: value\n` 形式の各行からvalueを抽出する
    let mut values: Vec<String> = Vec::with_capacity(columns.len());
    let lines: Vec<&str> = record.lines().collect();

    for (i, col_name) in columns.iter().enumerate() {
        let prefix = format!("{}: ", col_name);
        if let Some(line) = lines.get(i) {
            if let Some(value) = line.strip_prefix(&prefix) {
                values.push(value.to_string());
            } else {
                // カラム名に ": " が含まれるエッジケースのフォールバック
                if let Some(colon_pos) = line.find(": ") {
                    values.push(line[colon_pos + 2..].to_string());
                } else {
                    values.push(line.to_string());
                }
            }
        } else {
            values.push(String::new());
        }
    }

    Ok(values)
}

/// プレビュー用のシェルコマンドを生成
///
/// アイテムテキストの先頭フィールド（スペース区切り）から元の行インデックスを取得し、
/// チャンクファイルを特定して awk でセクションを抽出する。
/// フィルタ後もインデックスがずれない。
pub(super) fn build_preview_cmd(dir: &std::path::Path, table_name: Option<&str>) -> String {
    // パス内のシングルクォートを '\'' でエスケープすることで
    // シングルクォート囲みのシェル文字列内でも安全に使用できる
    let dir_escaped = dir.display().to_string().replace('\'', "'\\''");
    // テーブル名がある場合はプレビューのトップにヘッダーとして表示する
    let header_part = match table_name {
        Some(name) => {
            let name_escaped = name.replace('\'', "'\\''");
            format!("echo '[Table: {}]'; echo ''; ", name_escaped)
        }
        None => String::new(),
    };
    // {} はskimがoutput()の値（行インデックスの数値のみ）に置換する
    format!(
        "IDX={{}}; CHUNK=$((IDX / {chunk})); OFF=$((IDX % {chunk})); \
         FILE='{dir_escaped}/chunk_'$CHUNK'.txt'; \
         {header_part}\
         [ -f \"$FILE\" ] && awk -v idx=$OFF 'BEGIN{{RS=\"---\\n\"}} NR==idx+1{{printf \"%s\", $0}}' \"$FILE\" || echo '(読み込み中...)'",
        chunk = PREVIEW_CHUNK_SIZE,
        dir_escaped = dir_escaped,
        header_part = header_part
    )
}

/// プレビュー用ディレクトリを削除
pub(super) fn cleanup_preview_dir(dir: &std::path::Path) {
    if let Err(e) = std::fs::remove_dir_all(dir) {
        // 一時ファイルの削除失敗は動作に影響しないためwarnログに留める
        tracing::warn!("プレビュー用ディレクトリの削除に失敗しました: {}", e);
    }
}

/// データ行を表示幅に合わせてフォーマットする
pub(super) fn format_row_display(row: &[String], col_widths: &[usize]) -> String {
    row.iter()
        .enumerate()
        .map(|(i, cell)| {
            let w = col_widths.get(i).copied().unwrap_or(10);
            pad_to_width(cell, w)
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// 結果表示用のskimオプションを構築する
///
/// no_mouse(true): skimのマウスイベント処理を無効化することで、ターミナルネイティブの
/// テキスト選択（マウスドラッグ→Cmd+C）を可能にする。キーボード操作は引き続き利用できる。
pub(super) fn build_result_skim_options<'a>(
    header_line: &'a str,
    preview_cmd: &'a str,
    prompt: &'a str,
    preview_window: &'a str,
) -> std::result::Result<SkimOptions<'a>, crate::error::Error> {
    SkimOptionsBuilder::default()
        .height(Some("100%"))
        .multi(false)
        .reverse(true)
        .header(Some(header_line))
        .prompt(Some(prompt))
        .preview(Some(preview_cmd))
        .preview_window(Some(preview_window))
        .no_mouse(true)
        .build()
        .map_err(|e| crate::error::Error::Other(format!("{}: {:?}", t!(TuiMsg::SkimInitError), e)))
}

/// skimで選択された行からアクションを決定する
///
/// スキーマ一覧結果ならSET search_path、テーブル一覧結果ならSELECT、
/// それ以外ならWHEREテンプレート付きのレコード選択を返す。
pub(super) fn determine_skim_action(
    first_column: &str,
    first_value: &str,
    columns: &[String],
    values: &[String],
    source_sql: &str,
) -> SkimAction {
    if first_column == "schema_name" {
        // スキーマ一覧結果の場合: search_path を切り替える
        SkimAction::DrillDown(format!(
            "SET search_path TO {}",
            crate::query::escape_identifier(first_value)
        ))
    } else if first_column == "tablename" {
        // pg_tables クエリ結果の場合: テーブルの SELECT に展開
        SkimAction::DrillDown(format!(
            "SELECT * FROM {}",
            crate::query::escape_identifier(first_value)
        ))
    } else {
        let record = SelectedRecord {
            columns: columns
                .iter()
                .zip(values.iter())
                .map(|(col, val)| (col.clone(), val.clone()))
                .collect(),
        };
        // source_sqlからテーブル名を抽出してSELECT文のテンプレートを生成する
        // テーブル名が取得できない場合は "?" をフォールバックとして使う
        // extract_from_table は "db.table" 形式（バッククォートなし）を返すため
        // "." で分割して各部分を個別に escape_identifier に渡す
        let table_raw =
            crate::completion::extract_from_table(source_sql).unwrap_or_else(|| "?".to_string());
        let escaped_table = if let Some((db, tbl)) = table_raw.split_once('.') {
            format!(
                "{}.{}",
                crate::query::escape_identifier(db),
                crate::query::escape_identifier(tbl)
            )
        } else {
            crate::query::escape_identifier(&table_raw)
        };
        // PostgreSQLのエスケープルールに従い、シングルクォートを '' にエスケープする
        // （PostgreSQLではバックスラッシュは通常エスケープ文字ではないため不要）
        let escaped_value = first_value.replace('\'', "''");
        let where_clause = format!(
            "SELECT * FROM {} WHERE {} = '{}'",
            escaped_table,
            crate::query::escape_identifier(first_column),
            escaped_value
        );
        SkimAction::SelectRecord {
            where_template: where_clause,
            record,
        }
    }
}

/// SQL入力エリアの状態管理
pub(super) struct SqlInputState {
    /// 入力中のSQLテキスト
    pub text: String,
    /// カーソル位置（char単位）
    pub cursor_position: usize,
    /// テキスト選択開始位置（char単位、None=選択なし）
    pub selection_start: Option<usize>,
    /// 最後に実行したSQL（WHEREテンプレート生成時にテーブル名を抽出するために保持）
    pub last_sql: String,
    /// SQL実行履歴（最新が末尾）
    pub history: VecDeque<String>,
    /// 履歴参照中の現在位置（None=新規入力中、Some(index)=履歴参照中）
    pub history_index: Option<usize>,
    /// 履歴参照を開始した時点で退避しておいた入力中テキスト
    pub history_draft: String,
    /// Ctrl+K / Ctrl+U で削除したテキストを保存するキルバッファ
    pub kill_buffer: String,
    /// 補完候補キャッシュ（接続確立後に非同期で充填）
    pub completion_cache: Arc<tokio::sync::RwLock<CompletionCache>>,
    /// 補完ポップアップ状態
    pub completion_state: Option<CompletionState>,
}

/// Shell入力エリアの状態管理
#[derive(Debug)]
pub(super) struct ShellInputState {
    /// 入力中のテキスト
    pub text: String,
    /// カーソル位置（char単位）
    pub cursor_position: usize,
    /// テキスト選択開始位置（char単位、None=選択なし）
    ///
    /// Shift+矢印キーで選択範囲を設定する。cursor_positionと組み合わせて
    /// min(selection_start, cursor_position)..max(selection_start, cursor_position) が選択範囲となる。
    pub selection_start: Option<usize>,
    /// Ctrl+K / Ctrl+U で削除したテキストを保存するキルバッファ
    ///
    /// Ctrl+Y（yank）でペースト可能。システムクリップボードとは独立している。
    pub kill_buffer: String,
    /// Shell実行履歴（最新が末尾）
    pub history: VecDeque<String>,
    /// Shell履歴参照中の現在位置
    pub history_index: Option<usize>,
    /// Shell履歴参照を開始した時点で退避しておいた入力中テキスト
    pub history_draft: String,
    /// 実行待ちのシェルコマンド
    pub pending_command: Option<String>,
}

/// SQL入力エリアとShell入力エリアのフォーカス状態
///
/// Tab キーで Sql → Shell → Prompt → Sql の順に循環する。
/// Shell / Prompt フォーカス時は補完ポップアップを表示しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum InputFocus {
    /// SQL入力エリア（デフォルト）
    #[default]
    Sql,
    /// Shell入力エリア
    Shell,
    /// PROMPT入力エリア（Claude AI 連携）
    Prompt,
}

/// PROMPT 入力エリアの状態管理
///
/// Claude API へのリクエスト状態と入力テキストを管理する。
#[derive(Debug)]
pub(super) struct PromptInputState {
    /// 入力中のプロンプトテキスト
    pub text: String,
    /// カーソル位置（char単位）
    pub cursor_position: usize,
    /// テキスト選択開始位置（char単位、None=選択なし）
    ///
    /// Shift+矢印キーで選択範囲を設定する。cursor_positionと組み合わせて
    /// min(selection_start, cursor_position)..max(selection_start, cursor_position) が選択範囲となる。
    pub selection_start: Option<usize>,
    /// Ctrl+K / Ctrl+U で削除したテキストを保存するキルバッファ
    ///
    /// Ctrl+Y（yank）でペースト可能。システムクリップボードとは独立している。
    pub kill_buffer: String,
    /// API リクエスト処理中フラグ
    pub is_processing: bool,
    /// 最後のエラーメッセージ（None = エラーなし）
    pub last_error: Option<String>,
    /// ローディングアニメーションのフレームカウンター
    ///
    /// is_processing が true の間、イベントループのポーリングごとにインクリメントされ、
    /// 描画時に braille スピナーのフレーム選択に使用する。
    pub loading_tick: u8,
}

/// 実行中クエリの管理情報
pub(super) struct RunningQuery {
    manager: ConnectionManager,
    task: JoinHandle<Result<QueryResult>>,
}

/// アプリケーション状態
///
/// design.mdに基づく状態管理: 各状態がデータを保持
pub enum AppState {
    /// 接続先選択中
    Selecting {
        connections: Vec<crate::config::ConnectionConfig>,
        selected_index: usize,
    },

    /// 接続処理中（バックグラウンドで接続を試みている間）
    Connecting {
        connection_name: String,
        /// スピナーアニメーションのフレーム番号
        spinner_frame: u8,
    },

    /// 接続済み（SQL入力待ち）
    Connected { manager: ConnectionManager },

    /// クエリ実行中
    Executing { query: String },

    /// 結果表示中
    ShowingResult {
        result: QueryResult,
        /// エラー回復時に戻るConnectionManager
        manager: Option<ConnectionManager>,
    },

    /// ストリーミング結果表示待ち
    ///
    /// 表示系クエリ（SET以外）はDBから行を取得しながら即座にskimに送信する。
    /// TUIループがこの状態を検出したら、一時停止→ストリーミング表示→再開を行う。
    StreamingQuery {
        manager: ConnectionManager,
        sql: String,
        /// クエリタイムアウト（PostgresConfig.timeoutから取得）
        timeout_secs: u64,
    },

    /// カラム選択中（TUI一時停止→skim起動→TUI再開）
    ///
    /// Ctrl+S または sc エイリアスで遷移する。
    /// TUIループがこの状態を検出したら、テーブル選択 → カラム選択 を行い、
    /// 生成した SELECT 文を query_input にセットして Connected 状態に戻る。
    SelectingColumns {
        manager: ConnectionManager,
        /// クエリタイムアウト（PostgresConfig.timeoutから取得）
        timeout_secs: u64,
    },

    /// エラー表示中
    Error {
        message: String,
        /// エラー発生前の状態（戻り先）
        previous_state: Box<AppState>,
    },
}

/// 補完ポップアップの表示状態
#[derive(Debug, Clone)]
pub struct CompletionState {
    /// 現在表示中の補完候補リスト（フィルタ済み）
    pub candidates: Vec<CompletionItem>,
    /// 選択中の候補インデックス（0ベース）
    pub selected_index: usize,
    /// 現在入力中のトークン（ポップアップ表示開始時点のスナップショット）
    pub current_token: String,
}

/// TUIアプリケーション
pub struct App {
    /// 現在の状態
    pub(super) state: AppState,

    /// SQL入力エリアの状態（テキスト・カーソル・履歴・補完など）
    pub(super) sql: SqlInputState,

    /// Shell入力エリアの状態（テキスト・カーソル・履歴・実行予約など）
    pub(super) shell: ShellInputState,

    /// 終了フラグ
    pub(super) should_quit: bool,

    /// バックグラウンドで実行中のクエリ
    pub(super) running_query: Option<RunningQuery>,

    /// 選択されたレコードのプレビュー情報（SQL入力画面で表示）
    pub(super) selected_record: Option<SelectedRecord>,

    /// グレースフルシャットダウン用フラグ
    pub(super) shutdown_flag: Arc<AtomicBool>,

    /// USEコマンドで選択中のデータベース名
    ///
    /// 初期値はNone。USEコマンド成功時に更新される。
    /// 接続情報表示で「選択データベース: xxx」として表示する。
    pub(super) current_database: Option<String>,

    /// 接続先の名前（パンくずリスト表示用）
    ///
    /// 接続確立時（Selecting→Connected遷移時）に設定される。
    /// 切断するまで変化しない。
    pub(super) connection_name: Option<String>,

    /// bastion経由接続時のbastionホスト名（パンくずリスト表示用）
    ///
    /// bastion経由でない場合はNone。接続確立時に設定され、切断するまで変化しない。
    pub(super) bastion_name: Option<String>,

    /// 現在操作中のテーブル名（パンくずリスト表示用）
    ///
    /// SELECT文実行時やCtrl+Sでテーブル選択時に更新される。
    /// USEコマンドでDB切り替え時にクリアされる。
    pub(super) current_table: Option<String>,

    /// readonlyモードフラグ（CLI --readonly または接続設定 readonly=true）
    ///
    /// CLIフラグが true の場合は全接続をreadonly強制する。
    /// 接続設定の readonly=true との論理和で最終的な判定を行う。
    pub(super) readonly: bool,

    /// アプリケーション設定（APIキー・モデル名などを保持）
    pub(super) settings: crate::config::AppSettings,

    /// 全接続設定リスト（Selecting状態復帰時に使用）
    pub(super) connections: Vec<crate::config::ConnectionConfig>,

    /// 接続中のバックグラウンドタスク（Ctrl+C で abort するために保持）
    pub(super) connecting_task: Option<JoinHandle<crate::error::Result<ConnectionManager>>>,

    /// SQL/Shell/Prompt 入力エリアのフォーカス状態
    ///
    /// Tab キーで Sql → Shell → Prompt → Sql の順に循環する。
    /// Shell / Prompt フォーカス時は補完ポップアップを非表示にする。
    pub(super) input_focus: InputFocus,

    /// PROMPT 入力エリアの状態（テキスト・カーソル・処理中フラグ・エラーなど）
    pub(super) prompt: PromptInputState,

    /// PROMPT バックグラウンドタスクのハンドル
    ///
    /// Enter で claude::run_agent を spawn し、完了時に poll_prompt_completion() で
    /// 生成 SQL を sql.text に書き込む。
    pub(super) prompt_task: Option<JoinHandle<crate::error::Result<String>>>,
}

impl App {
    /// 新しいアプリケーションを作成
    pub fn new(config: Config, shutdown_flag: Arc<AtomicBool>, cli_readonly: bool) -> Self {
        // default_bastionを適用した接続設定リストを取得
        let connections = config.resolve_connections();
        let settings = config.settings;
        Self {
            state: AppState::Selecting {
                connections: connections.clone(),
                selected_index: 0,
            },
            sql: SqlInputState {
                text: String::new(),
                cursor_position: 0,
                selection_start: None,
                last_sql: String::new(),
                history: VecDeque::new(),
                history_index: None,
                history_draft: String::new(),
                kill_buffer: String::new(),
                completion_cache: Arc::new(tokio::sync::RwLock::new(CompletionCache::new())),
                completion_state: None,
            },
            shell: ShellInputState {
                text: String::new(),
                cursor_position: 0,
                selection_start: None,
                kill_buffer: String::new(),
                history: VecDeque::new(),
                history_index: None,
                history_draft: String::new(),
                pending_command: None,
            },
            should_quit: false,
            running_query: None,
            selected_record: None,
            shutdown_flag,
            current_database: None,
            connection_name: None,
            bastion_name: None,
            current_table: None,
            readonly: cli_readonly,
            settings,
            connections,
            connecting_task: None,
            input_focus: InputFocus::default(),
            prompt: PromptInputState {
                text: String::new(),
                cursor_position: 0,
                selection_start: None,
                kill_buffer: String::new(),
                is_processing: false,
                last_error: None,
                loading_tick: 0,
            },
            prompt_task: None,
        }
    }

    /// アプリケーションのメインループを実行
    pub async fn run(&mut self) -> Result<()> {
        // 接続先選択（Selecting状態の場合のみ）
        if let AppState::Selecting {
            ref connections, ..
        } = self.state
        {
            let selected_connection = crate::selector::select_connection(connections)?;

            // CLIフラグとTOML設定の論理和でreadonly判定
            // CLI --readonly が true の場合は全接続に適用し、TOML設定のreadonly=trueも尊重する
            let readonly = self.readonly || selected_connection.readonly;

            // 接続名をパンくずリスト表示用に先に保存する（connect でムーブされるため）
            let connection_name = selected_connection.name.clone();
            self.connection_name = Some(connection_name.clone());

            // bastion経由の場合はbastionホスト名をパンくず表示用に保存する（connect でムーブされるため）
            // resolve_connections() 後は BastionSetting::Config のみ存在する
            self.bastion_name = match &selected_connection.bastion {
                Some(crate::config::BastionSetting::Config(ref cfg)) => Some(cfg.host.clone()),
                _ => None,
            };

            tracing::info!("Connecting to: {}", connection_name);

            // 接続処理をバックグラウンドタスクに回してTUIループを先に起動し、接続中UIを表示できるようにする
            self.connecting_task = Some(tokio::spawn(async move {
                crate::connection::ConnectionManager::connect(selected_connection, readonly).await
            }));

            self.state = AppState::Connecting {
                connection_name,
                spinner_frame: 0,
            };
        }

        // ターミナル初期化
        enable_raw_mode().map_err(|e| Error::Tui(format!("ターミナル初期化失敗: {}", e)))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)
            .map_err(|e| Error::Tui(format!("ターミナル初期化失敗: {}", e)))?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal =
            Terminal::new(backend).map_err(|e| Error::Tui(format!("ターミナル作成失敗: {}", e)))?;

        // メインループ
        let result = self.run_loop(&mut terminal).await;

        // ターミナル復元
        disable_raw_mode().map_err(|e| Error::Tui(format!("ターミナル復元失敗: {}", e)))?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)
            .map_err(|e| Error::Tui(format!("ターミナル復元失敗: {}", e)))?;
        terminal
            .show_cursor()
            .map_err(|e| Error::Tui(format!("カーソル表示失敗: {}", e)))?;

        // spawn_blocking 内の同期タスク（SSH/DNS）は abort() できないため、
        // 接続中にキャンセルされた場合はプロセスを即終了してタイムアウト待ちを回避する
        if self.connecting_task.is_some() {
            std::process::exit(0);
        }

        result
    }

    /// メインイベントループ
    async fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        loop {
            self.poll_query_completion().await?;
            self.poll_connecting().await?;
            self.poll_prompt_completion().await;

            // StreamingQuery状態に遷移した場合、ストリーミングでskimに渡す
            if matches!(self.state, AppState::StreamingQuery { .. }) {
                let (manager, sql, timeout_secs) = match std::mem::replace(
                    &mut self.state,
                    AppState::Selecting {
                        connections: Vec::new(),
                        selected_index: 0,
                    },
                ) {
                    AppState::StreamingQuery {
                        manager,
                        sql,
                        timeout_secs,
                    } => (manager, sql, timeout_secs),
                    other => {
                        self.state = other;
                        continue;
                    }
                };

                // ストリーミング表示（SQLエラー・タイムアウト時は?でErrを返してrun_loopに伝播）
                // LeaveAlternateScreen は show_result_streaming 内でサンプリング完了後に行う（ちらつき防止）
                let streaming_result = self.show_result_streaming(
                    manager.pool().clone(),
                    &sql,
                    std::time::Duration::from_secs(timeout_secs),
                    terminal,
                );

                // TUI再開（ストリーミング結果に関わらず必ず再開する）
                enable_raw_mode()
                    .map_err(|e| Error::Tui(format!("ターミナル初期化失敗: {}", e)))?;
                execute!(terminal.backend_mut(), EnterAlternateScreen)
                    .map_err(|e| Error::Tui(format!("ターミナル初期化失敗: {}", e)))?;
                terminal
                    .clear()
                    .map_err(|e| Error::Tui(format!("画面クリア失敗: {}", e)))?;

                // SQLエラー・タイムアウト発生時はError状態に遷移してSQL入力画面に戻れるようにする
                let next_query = match streaming_result {
                    Err(e) => {
                        tracing::error!("Streaming query failed: {}", e);
                        self.state = AppState::Error {
                            message: format!("{}", e),
                            previous_state: Box::new(AppState::Connected { manager }),
                        };
                        continue;
                    }
                    Ok(action) => action,
                };

                match next_query {
                    Some(SkimAction::DrillDown(next_sql)) => {
                        self.state = AppState::Connected { manager };
                        self.selected_record = None;
                        self.sql.text = next_sql;
                        self.sql.cursor_position = self.sql.text.chars().count();
                        self.add_to_history(&self.sql.text.clone());
                        let sql_upper = self.sql.text.trim().to_uppercase();
                        if sql_upper.starts_with("SET ") {
                            self.execute_query()?;
                        } else {
                            self.transition_to_streaming()?;
                        }
                    }
                    Some(SkimAction::SelectRecord {
                        where_template,
                        record,
                    }) => {
                        self.state = AppState::Connected { manager };
                        self.selected_record = Some(record);
                        self.sql.text = where_template;
                        self.sql.cursor_position = self.sql.text.chars().count();
                    }
                    None => {
                        self.state = AppState::Connected { manager };
                        self.sql.text.clear();
                        self.sql.cursor_position = 0;
                    }
                }

                // 状態遷移直後に即描画してちらつきを抑制する
                terminal
                    .draw(|f| self.render(f))
                    .map_err(|e| Error::Tui(format!("描画エラー: {}", e)))?;

                continue;
            }

            // SelectingColumns状態に遷移した場合、skimでカラム選択
            if matches!(self.state, AppState::SelectingColumns { .. }) {
                let (manager, timeout_secs) = match std::mem::replace(
                    &mut self.state,
                    AppState::Selecting {
                        connections: Vec::new(),
                        selected_index: 0,
                    },
                ) {
                    AppState::SelectingColumns {
                        manager,
                        timeout_secs,
                    } => (manager, timeout_secs),
                    other => {
                        self.state = other;
                        continue;
                    }
                };

                // カラム選択（DBエラー時は Error 状態に遷移）
                // current_database を渡してテーブル一覧を正しく表示する
                // LeaveAlternateScreen は select_columns_interactive 内でデータ準備後に行う（ちらつき防止）
                let select_result = self.select_columns_interactive(
                    manager.pool(),
                    std::time::Duration::from_secs(timeout_secs),
                    self.current_database.as_deref(),
                    terminal,
                );

                // TUI再開（カラム選択結果に関わらず必ず再開する）
                enable_raw_mode()
                    .map_err(|e| Error::Tui(format!("ターミナル初期化失敗: {}", e)))?;
                execute!(terminal.backend_mut(), EnterAlternateScreen)
                    .map_err(|e| Error::Tui(format!("ターミナル初期化失敗: {}", e)))?;
                terminal
                    .clear()
                    .map_err(|e| Error::Tui(format!("画面クリア失敗: {}", e)))?;

                match select_result {
                    Err(e) => {
                        tracing::error!("Column selection failed: {}", e);
                        self.state = AppState::Error {
                            message: format!("{}", e),
                            previous_state: Box::new(AppState::Connected { manager }),
                        };
                    }
                    Ok(Some(sql)) => {
                        // 生成されたSELECT文を即実行する
                        self.state = AppState::Connected { manager };
                        self.sql.text = sql;
                        self.sql.cursor_position = self.sql.text.chars().count();
                        self.execute_query()?;
                    }
                    Ok(None) => {
                        // キャンセル: Connected 状態に戻るだけ
                        self.state = AppState::Connected { manager };
                    }
                }

                // 状態遷移直後に即描画してちらつきを抑制する
                terminal
                    .draw(|f| self.render(f))
                    .map_err(|e| Error::Tui(format!("描画エラー: {}", e)))?;

                continue;
            }

            // ShowingResult状態に遷移した場合、skimで結果を表示
            if matches!(self.state, AppState::ShowingResult { .. }) {
                // 状態からresultとmanagerを取り出す
                let (result, manager_opt) = match std::mem::replace(
                    &mut self.state,
                    AppState::Selecting {
                        connections: Vec::new(),
                        selected_index: 0,
                    },
                ) {
                    AppState::ShowingResult { result, manager } => (result, manager),
                    other => {
                        self.state = other;
                        continue;
                    }
                };

                // skimで結果表示（LeaveAlternateScreen は show_result_with_skim 内でデータ準備後に行う）
                let next_query = self.show_result_with_skim(&result, terminal)?;

                // ratatuiを再開
                enable_raw_mode()
                    .map_err(|e| Error::Tui(format!("ターミナル初期化失敗: {}", e)))?;
                execute!(terminal.backend_mut(), EnterAlternateScreen)
                    .map_err(|e| Error::Tui(format!("ターミナル初期化失敗: {}", e)))?;
                terminal
                    .clear()
                    .map_err(|e| Error::Tui(format!("画面クリア失敗: {}", e)))?;

                // 結果に応じて状態遷移
                match next_query {
                    Some(SkimAction::DrillDown(sql)) => {
                        if let Some(manager) = manager_opt {
                            self.state = AppState::Connected { manager };
                            self.selected_record = None;
                            self.sql.text = sql;
                            self.sql.cursor_position = self.sql.text.chars().count();
                            self.add_to_history(&self.sql.text.clone());
                            self.execute_query()?;
                        } else {
                            self.should_quit = true;
                        }
                    }
                    Some(SkimAction::SelectRecord {
                        where_template,
                        record,
                    }) => {
                        if let Some(manager) = manager_opt {
                            self.state = AppState::Connected { manager };
                            self.selected_record = Some(record);
                            self.sql.text = where_template;
                            self.sql.cursor_position = self.sql.text.chars().count();
                        } else {
                            self.should_quit = true;
                        }
                    }
                    None => {
                        if let Some(manager) = manager_opt {
                            self.state = AppState::Connected { manager };
                        } else {
                            self.should_quit = true;
                        }
                    }
                }

                // 状態遷移直後に即描画してちらつきを抑制する
                terminal
                    .draw(|f| self.render(f))
                    .map_err(|e| Error::Tui(format!("描画エラー: {}", e)))?;

                continue;
            }

            // AI処理中はローディングアニメーションのカウンターを更新する
            // ポーリングループ（100ms間隔）ごとにインクリメントすることで
            // 描画時に braille スピナーのフレームが自然に切り替わる
            if self.prompt.is_processing {
                self.prompt.loading_tick = self.prompt.loading_tick.wrapping_add(1);
            }

            // 画面描画
            terminal
                .draw(|f| self.render(f))
                .map_err(|e| Error::Tui(format!("描画エラー: {}", e)))?;

            // シャットダウンフラグチェック
            if self.shutdown_flag.load(Ordering::Relaxed) {
                tracing::info!("Shutdown signal received, waiting for ongoing operations...");

                // 実行中のクエリがある場合は最大5秒待機
                if matches!(self.state, AppState::Executing { .. }) {
                    tracing::info!("Query is executing, waiting up to 5 seconds...");

                    let start = std::time::Instant::now();
                    while matches!(self.state, AppState::Executing { .. })
                        && start.elapsed() < std::time::Duration::from_secs(5)
                    {
                        self.poll_query_completion().await?;
                        if matches!(self.state, AppState::Executing { .. }) {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }

                    if matches!(self.state, AppState::Executing { .. }) {
                        tracing::warn!("Query execution timed out during shutdown, aborting task");
                        self.abort_running_query();
                    } else {
                        tracing::info!("Query completed successfully before shutdown");
                    }
                }

                self.should_quit = true;
            }

            // 終了チェック
            if self.should_quit {
                self.abort_running_query();
                // abort() のみ呼び take() しない（run() 側で is_some() を確認して process::exit するため）
                if let Some(ref task) = self.connecting_task {
                    task.abort();
                }
                break;
            }

            // pending_shell_command チェック: TUI を一時停止してシェルコマンドを実行する
            // handle_shell_input は terminal への参照を持てないため、
            // App フィールド経由でトリガーを通知し、run_loop 側で実際の停止・再起動を担う
            if let Some(cmd) = self.shell.pending_command.take() {
                use std::process::Stdio;

                // bastion経由接続中の場合はbastionサーバー上でコマンドを実行する。
                // resolve_connections()適用後のConfigではbastionはConfig(BastionConfig)かNoneのみなので
                // Toggle(true/false)は考慮不要。
                let bastion_config: Option<BastionConfig> = self
                    .current_connection_config()
                    .and_then(|config| match &config.bastion {
                        Some(BastionSetting::Config(cfg)) => Some(cfg.clone()),
                        _ => None,
                    });

                if bastion_config.is_some() {
                    tracing::info!("Executing shell command on bastion server: {}", cmd);
                } else {
                    tracing::info!("Executing shell command locally: {}", cmd);
                }

                // TUI を一時停止
                disable_raw_mode().map_err(|e| Error::Tui(format!("ターミナル復元失敗: {}", e)))?;
                execute!(terminal.backend_mut(), LeaveAlternateScreen)
                    .map_err(|e| Error::Tui(format!("ターミナル復元失敗: {}", e)))?;

                let status = if let Some(ref bastion_cfg) = bastion_config {
                    // bastion経由: ssh コマンド経由でリモート実行する
                    let mut ssh_cmd = tokio::process::Command::new("ssh");
                    ssh_cmd
                        .arg("-p")
                        .arg(bastion_cfg.port.to_string())
                        .arg(format!("{}@{}", bastion_cfg.user, bastion_cfg.host));

                    // key_pathが指定されている場合のみ -i オプションを付ける。
                    // 指定がない場合は SSH agent に委ねる。
                    if let Some(ref key_path) = bastion_cfg.key_path {
                        ssh_cmd.arg("-i").arg(key_path);
                    }

                    ssh_cmd
                        .arg(&cmd)
                        .stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit())
                        .status()
                        .await
                        .map_err(|e| Error::Tui(format!("SSHコマンド実行失敗: {}", e)))?
                } else {
                    // 直接接続: sh -c でローカル実行する（標準 I/O を継承）
                    tokio::process::Command::new("sh")
                        .arg("-c")
                        .arg(&cmd)
                        .stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit())
                        .status()
                        .await
                        .map_err(|e| Error::Tui(format!("シェルコマンド実行失敗: {}", e)))?
                };

                if !status.success() {
                    tracing::warn!("Shell command exited with status: {}", status);
                }

                // ユーザーが結果を確認できるよう Enter 入力まで待機
                println!("\n[Press Enter to continue...]");
                let _ = std::io::stdin().read_line(&mut String::new());

                // TUI を再開
                enable_raw_mode()
                    .map_err(|e| Error::Tui(format!("ターミナル初期化失敗: {}", e)))?;
                execute!(terminal.backend_mut(), EnterAlternateScreen)
                    .map_err(|e| Error::Tui(format!("ターミナル初期化失敗: {}", e)))?;
                terminal
                    .clear()
                    .map_err(|e| Error::Tui(format!("画面クリア失敗: {}", e)))?;

                continue;
            }

            // イベント処理（100ms待機）
            if event::poll(std::time::Duration::from_millis(100))
                .map_err(|e| Error::Tui(format!("イベント取得失敗: {}", e)))?
            {
                let ev = event::read()
                    .map_err(|e| Error::Tui(format!("イベント読み込み失敗: {}", e)))?;
                self.handle_event(ev).await?;
            }
        }

        Ok(())
    }

    /// 現在接続中のConnectionConfigを取得する（bastion判定に使用）
    ///
    /// manager を保持しているすべての AppState からconfig参照を返す。
    /// 接続していない状態（Selecting / Executing / Error 等）ではNoneを返す。
    fn current_connection_config(&self) -> Option<&crate::config::ConnectionConfig> {
        match &self.state {
            AppState::Connected { manager } => Some(manager.config()),
            AppState::StreamingQuery { manager, .. } => Some(manager.config()),
            AppState::SelectingColumns { manager, .. } => Some(manager.config()),
            AppState::ShowingResult {
                manager: Some(manager),
                ..
            } => Some(manager.config()),
            _ => None,
        }
    }

    /// 現在の接続がreadonlyモードかどうかを返す
    ///
    /// Connected状態ではConnectionManagerのis_readonly()を参照する。
    /// それ以外の状態ではCLIフラグ由来のself.readonlyを返す。
    fn is_current_readonly(&self) -> bool {
        match &self.state {
            AppState::Connected { manager } => manager.is_readonly(),
            _ => self.readonly,
        }
    }

    /// クエリを実行
    fn execute_query(&mut self) -> Result<()> {
        // AppStateからmanagerをムーブ
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
                // 元の状態に戻す
                self.state = other;
                return Err(Error::Other("接続がありません".to_string()));
            }
        };

        let query = self.sql.text.clone();
        let pool = manager.pool().clone();
        let query_for_task = query.clone();
        // プールのセッション状態問題を回避するため、現在のデータベースをキャプチャしておく。
        // クエリ実行は別タスクで行われるため、クロージャにムーブする必要がある。
        let current_database_for_task = self.current_database.clone();

        // 次の show_result_with_skim でテーブル名を抽出できるよう保存する
        self.sql.last_sql = query.clone();

        // 実行中状態に遷移
        self.state = AppState::Executing {
            query: query.clone(),
        };
        self.running_query = Some(RunningQuery {
            manager,
            // TUIの再描画と入力処理を止めないため、クエリは別タスクで実行する
            task: tokio::spawn(async move {
                crate::query::execute_query(
                    &pool,
                    &query_for_task,
                    current_database_for_task.as_deref(),
                )
                .await
            }),
        });

        Ok(())
    }

    /// 実行中クエリの完了を取り込み、状態遷移を進める
    async fn poll_query_completion(&mut self) -> Result<()> {
        let task_finished = self
            .running_query
            .as_ref()
            .is_some_and(|running_query| running_query.task.is_finished());

        if !task_finished {
            return Ok(());
        }

        let Some(running_query) = self.running_query.take() else {
            return Ok(());
        };

        let query = match std::mem::replace(
            &mut self.state,
            AppState::Selecting {
                connections: Vec::new(),
                selected_index: 0,
            },
        ) {
            AppState::Executing { query } => query,
            other => {
                self.state = other;
                self.running_query = Some(running_query);
                return Ok(());
            }
        };

        let RunningQuery { manager, task } = running_query;

        match task.await {
            Ok(Err(e)) => {
                tracing::error!("Query execution failed: {}", e);
                let error_message = t!(TuiMsg::QueryFailed {
                    detail: &e.user_message()
                });
                let previous_state = Box::new(AppState::Connected { manager });
                self.state = AppState::Error {
                    message: error_message,
                    previous_state,
                };
                Ok(())
            }
            Ok(Ok(result)) => {
                if result.should_display {
                    // 結果表示状態に遷移（managerを保持）
                    self.state = AppState::ShowingResult {
                        result,
                        manager: Some(manager),
                    };
                    self.sql.text.clear();
                    self.sql.cursor_position = 0;
                } else {
                    // SET等の結果を表示しないコマンドは即座にConnected状態に戻る
                    tracing::debug!("Command executed, returning to Connected state");

                    // SET search_path 実行後はパンくずリストのスキーマ名を更新する
                    // 実行したSQLからスキーマ名を抽出して current_database に反映する
                    if let Some(schema) = extract_search_path_from_set_sql(&query) {
                        self.current_database = Some(schema);
                        // スキーマ変更時はテーブルが変わる可能性があるためテーブル名をクリアする
                        self.current_table = None;
                    }

                    self.state = AppState::Connected { manager };
                    self.sql.text.clear();
                    self.sql.cursor_position = 0;

                    // SETコマンド実行後、テーブルキャッシュを更新する
                    // （例: SET search_path でスキーマが変更された場合に備える）
                    if let AppState::Connected { ref manager } = self.state {
                        let cache_arc = self.sql.completion_cache.clone();
                        let pool = manager.pool().clone();
                        let current_db = self.current_database.clone();
                        tokio::spawn(async move {
                            if let Err(e) =
                                refresh_table_cache(&cache_arc, &pool, current_db.as_deref()).await
                            {
                                tracing::warn!("テーブルキャッシュの更新に失敗しました: {}", e);
                            }
                        });
                    }
                }
                Ok(())
            }
            Err(join_error) => {
                tracing::error!(
                    "Query execution task failed for '{}': {}",
                    query,
                    join_error
                );
                let error_message = if join_error.is_cancelled() {
                    t!(TuiMsg::QueryCancelled { query: &query })
                } else {
                    t!(TuiMsg::QueryTaskFailed {
                        detail: &join_error.to_string()
                    })
                };
                let previous_state = Box::new(AppState::Connected { manager });
                self.state = AppState::Error {
                    message: error_message,
                    previous_state,
                };
                Ok(())
            }
        }
    }

    /// 接続バックグラウンドタスクの完了をポーリングし、完了したら Connected または Error 状態に遷移する
    async fn poll_connecting(&mut self) -> Result<()> {
        if !matches!(self.state, AppState::Connecting { .. }) {
            return Ok(());
        }

        let finished = self
            .connecting_task
            .as_ref()
            .is_some_and(|t| t.is_finished());

        if !finished {
            // 接続中はスピナーフレームを進める
            if let AppState::Connecting {
                ref mut spinner_frame,
                ..
            } = self.state
            {
                *spinner_frame = spinner_frame.wrapping_add(1);
            }
            return Ok(());
        }

        let Some(task) = self.connecting_task.take() else {
            return Ok(());
        };

        let connection_name = match &self.state {
            AppState::Connecting {
                connection_name, ..
            } => connection_name.clone(),
            _ => return Ok(()),
        };

        let connections = self.connections.clone();

        match task.await {
            Ok(Ok(manager)) => {
                tracing::info!("Connection established: {}", connection_name);
                let cache_arc = self.sql.completion_cache.clone();
                let pool = manager.pool().clone();
                tokio::spawn(async move {
                    if let Err(e) = initialize_completion_cache(&cache_arc, &pool, None).await {
                        tracing::warn!("補完キャッシュの初期化に失敗しました: {}", e);
                    }
                });
                self.state = AppState::Connected { manager };
            }
            Ok(Err(e)) => {
                tracing::error!("Connection failed: {}", e);
                self.state = AppState::Error {
                    message: e.user_message(),
                    previous_state: Box::new(AppState::Selecting {
                        connections,
                        selected_index: 0,
                    }),
                };
            }
            Err(join_error) => {
                tracing::error!("Connection task panicked: {}", join_error);
                self.state = AppState::Error {
                    message: format!("接続タスクが異常終了しました: {}", join_error),
                    previous_state: Box::new(AppState::Selecting {
                        connections,
                        selected_index: 0,
                    }),
                };
            }
        }

        Ok(())
    }

    /// Shell実行履歴に追加する
    ///
    /// 直前と同じコマンドは重複追加しない。最大MAX_SQL_HISTORY件を保持する。
    pub(super) fn add_to_shell_history(&mut self, cmd: &str) {
        let cmd = cmd.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        if self.shell.history.back().map(|s| s.as_str()) != Some(&cmd) {
            self.shell.history.push_back(cmd);
            if self.shell.history.len() > MAX_SQL_HISTORY {
                self.shell.history.pop_front();
            }
        }
        // 履歴参照状態をリセット（実行後は新規入力状態に戻す）
        self.shell.history_index = None;
        self.shell.history_draft.clear();
    }

    /// Shell履歴を遡る（古い方向へ）
    pub(super) fn shell_history_prev(&mut self) {
        if self.shell.history.is_empty() {
            return;
        }
        match self.shell.history_index {
            None => {
                // 新規入力中 → 現在の入力を退避して最新の履歴を表示
                self.shell.history_draft = self.shell.text.clone();
                let idx = self.shell.history.len() - 1;
                self.shell.history_index = Some(idx);
                self.shell.text = self.shell.history[idx].clone();
            }
            Some(idx) if idx > 0 => {
                // 履歴参照中 → さらに古い履歴へ
                let new_idx = idx - 1;
                self.shell.history_index = Some(new_idx);
                self.shell.text = self.shell.history[new_idx].clone();
            }
            _ => {
                // 最古の履歴に到達済み → 何もしない
                return;
            }
        }
        self.shell.cursor_position = self.shell.text.chars().count();
    }

    /// Shell履歴を進む（新しい方向へ）
    pub(super) fn shell_history_next(&mut self) {
        match self.shell.history_index {
            Some(idx) => {
                if idx + 1 < self.shell.history.len() {
                    // より新しい履歴へ
                    let new_idx = idx + 1;
                    self.shell.history_index = Some(new_idx);
                    self.shell.text = self.shell.history[new_idx].clone();
                } else {
                    // 履歴の末尾を超えた → 退避した入力を復元して新規入力状態に戻す
                    self.shell.history_index = None;
                    self.shell.text = self.shell.history_draft.clone();
                    self.shell.history_draft.clear();
                }
                self.shell.cursor_position = self.shell.text.chars().count();
            }
            None => {
                // 新規入力中 → 何もしない
            }
        }
    }

    /// PROMPT バックグラウンドタスクの完了をポーリングし、
    /// 完了時に生成 SQL を sql.text に書き込む
    ///
    /// - 成功時: sql.text に生成 SQL をセットし、フォーカスを Sql に戻す
    /// - エラー時: prompt.last_error にエラーメッセージをセットする
    /// - いずれの場合も is_processing = false にリセットする
    /// - エージェントが内部で実行した SELECT は別接続で完結するため
    ///   TUI 側のクエリ結果テーブルには影響しない
    pub(super) async fn poll_prompt_completion(&mut self) {
        let task_finished = self.prompt_task.as_ref().is_some_and(|t| t.is_finished());

        if !task_finished {
            return;
        }

        let Some(task) = self.prompt_task.take() else {
            return;
        };

        match task.await {
            Ok(Ok(sql)) => {
                tracing::debug!("PROMPT task completed: generated SQL length={}", sql.len());
                // TUI の SQL 入力エリアは1行表示のため、改行をスペースに変換してワンライナーにする
                let sql = normalize_sql_to_oneliner(&sql);
                self.sql.text = sql;
                self.sql.cursor_position = self.sql.text.chars().count();
                self.prompt.is_processing = false;
                self.prompt.last_error = None;
                // アニメーションを停止するためカウンターをリセットする
                self.prompt.loading_tick = 0;
                // 生成 SQL を確認しやすいよう SQL 入力エリアにフォーカスを戻す
                self.input_focus = InputFocus::Sql;
            }
            Ok(Err(e)) => {
                tracing::error!("PROMPT task failed: {}", e);
                self.prompt.last_error = Some(e.user_message());
                self.prompt.is_processing = false;
                // アニメーションを停止するためカウンターをリセットする
                self.prompt.loading_tick = 0;
            }
            Err(join_error) => {
                tracing::error!("PROMPT task panicked: {}", join_error);
                self.prompt.last_error = Some(format!("タスクが異常終了しました: {}", join_error));
                self.prompt.is_processing = false;
                // アニメーションを停止するためカウンターをリセットする
                self.prompt.loading_tick = 0;
            }
        }
    }

    /// 実行中クエリがあれば中断する
    fn abort_running_query(&mut self) {
        if let Some(running_query) = self.running_query.take() {
            tracing::info!("Aborting running query task");
            running_query.task.abort();
        }
    }
}

/// SQL 文字列を TUI 入力欄用のワンライナーに正規化する
///
/// Claude が生成する SQL には改行や連続スペースが含まれることがあるため、
/// 1行表示の入力エリアにセットする前にフラットな文字列に変換する。
/// 変換内容:
/// 1. `\r\n` / `\r` / `\n` をスペースに置換
/// 2. 連続するスペースを1つに圧縮
/// 3. 前後をトリム
fn normalize_sql_to_oneliner(sql: &str) -> String {
    // \r\n を先に処理することで \r が二重変換されるのを防ぐ
    // \r\n を先に処理してから残りの \r / \n をまとめてスペースに置換する
    let replaced = sql.replace("\r\n", " ").replace(['\r', '\n'], " ");
    // 連続スペースを1つに圧縮する
    let compressed: String = replaced
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    compressed
}

/// SET search_path SQL文からスキーマ名を抽出する
///
/// `SET search_path TO schema_name` または `SET search_path = schema_name` の形式を解析し、
/// スキーマ名を返す。解析できない場合は None を返す。
/// パンくずリストの `current_database` を更新するために使用する。
fn extract_search_path_from_set_sql(sql: &str) -> Option<String> {
    let trimmed = sql.trim();
    let upper = trimmed.to_uppercase();

    // SET search_path TO ... / SET search_path = ... の両形式に対応する
    // 大文字変換後の接頭辞でマッチして、接頭辞バイト長分だけ元の文字列を切り捨てる
    let value_part = if upper.starts_with("SET SEARCH_PATH TO ") {
        trimmed["SET SEARCH_PATH TO ".len()..].trim()
    } else if upper.starts_with("SET SEARCH_PATH=") || upper.starts_with("SET SEARCH_PATH =") {
        // = の直後（空白を含む）からスキーマ部分を取り出す
        trimmed.split_once('=')?.1.trim()
    } else {
        return None;
    };

    // カンマ区切りで複数指定された場合は最初のスキーマを使用する
    let first = value_part.split(',').next()?.trim();
    // ダブルクォートを除去する（PostgreSQL の識別子クォート形式）
    let schema = first.trim_matches('"').to_string();
    if schema.is_empty() {
        None
    } else {
        Some(schema)
    }
}

/// テーブル一覧を取得して Vec<String> で返す
///
/// PostgreSQLではpg_catalog.pg_tablesを使用してテーブル一覧を取得する。
/// current_schema() に基づき、現在の search_path のスキーマのテーブルのみを返す。
async fn fetch_tables(
    pool: &sqlx::Pool<sqlx::Postgres>,
    _database: Option<&str>,
) -> std::result::Result<Vec<String>, sqlx::Error> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT tablename FROM pg_catalog.pg_tables \
         WHERE schemaname = current_schema() \
         ORDER BY tablename",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| row.try_get::<String, _>(0).unwrap_or_default())
        .collect())
}

/// 補完キャッシュを初期化する（接続確立直後に1回呼ぶ）
///
/// pg_tables からテーブル一覧を取得してキャッシュに書き込む。
/// PostgreSQLでは接続ごとにDBが固定されるため、current_databaseパラメータは不要。
async fn initialize_completion_cache(
    cache: &Arc<tokio::sync::RwLock<CompletionCache>>,
    pool: &sqlx::Pool<sqlx::Postgres>,
    current_database: Option<&str>,
) -> crate::error::Result<()> {
    let tables = fetch_tables(pool, current_database)
        .await
        .map_err(crate::error::Error::QueryExecution)?;

    let mut cache_write = cache.write().await;
    cache_write.tables = tables;
    cache_write.is_ready = true;

    tracing::debug!(
        "Completion cache initialized: {} tables",
        cache_write.tables.len(),
    );

    Ok(())
}

/// テーブルキャッシュを更新する
///
/// pg_tables からテーブル一覧を再取得してキャッシュを更新する。
/// PostgreSQLでは接続ごとにDBが固定されるため、current_databaseパラメータは不要。
async fn refresh_table_cache(
    cache: &Arc<tokio::sync::RwLock<CompletionCache>>,
    pool: &sqlx::Pool<sqlx::Postgres>,
    current_database: Option<&str>,
) -> crate::error::Result<()> {
    let tables = fetch_tables(pool, current_database)
        .await
        .map_err(crate::error::Error::QueryExecution)?;

    let mut cache_write = cache.write().await;
    cache_write.tables = tables;

    tracing::debug!("Table cache refreshed: {} tables", cache_write.tables.len());

    Ok(())
}
impl Drop for App {
    fn drop(&mut self) {
        self.abort_running_query();
        // PROMPT バックグラウンドタスクを中断する
        if let Some(task) = self.prompt_task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    /// テスト用に sql.text のみをセットした最小限の App を生成する
    ///
    /// App::new() は Config 等の複雑な依存があるため、テストでは
    /// 必要なフィールドのみをセットした App を直接構築する。
    fn make_app_with_input(input: &str) -> App {
        App {
            state: AppState::Selecting {
                connections: Vec::new(),
                selected_index: 0,
            },
            sql: SqlInputState {
                text: input.to_string(),
                cursor_position: 0,
                selection_start: None,
                last_sql: String::new(),
                history: std::collections::VecDeque::new(),
                history_index: None,
                history_draft: String::new(),
                kill_buffer: String::new(),
                completion_cache: Arc::new(tokio::sync::RwLock::new(
                    crate::completion::CompletionCache::new(),
                )),
                completion_state: None,
            },
            shell: ShellInputState {
                text: String::new(),
                cursor_position: 0,
                selection_start: None,
                kill_buffer: String::new(),
                history: std::collections::VecDeque::new(),
                history_index: None,
                history_draft: String::new(),
                pending_command: None,
            },
            should_quit: false,
            running_query: None,
            selected_record: None,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            current_database: None,
            connection_name: None,
            bastion_name: None,
            current_table: None,
            readonly: false,
            settings: crate::config::AppSettings::default(),
            connections: Vec::new(),
            connecting_task: None,
            input_focus: InputFocus::default(),
            prompt: PromptInputState {
                text: String::new(),
                cursor_position: 0,
                selection_start: None,
                kill_buffer: String::new(),
                is_processing: false,
                last_error: None,
                loading_tick: 0,
            },
            prompt_task: None,
        }
    }

    // is_completion_separator のテスト

    #[test]
    fn test_is_word_separator() {
        // 区切り文字
        assert!(crate::completion::is_completion_separator(' '));
        assert!(crate::completion::is_completion_separator('\t'));
        assert!(crate::completion::is_completion_separator(','));
        assert!(crate::completion::is_completion_separator(';'));
        assert!(crate::completion::is_completion_separator('.'));
        assert!(crate::completion::is_completion_separator('('));
        assert!(crate::completion::is_completion_separator(')'));
        assert!(crate::completion::is_completion_separator('['));
        assert!(crate::completion::is_completion_separator(']'));
        assert!(crate::completion::is_completion_separator('='));
        assert!(crate::completion::is_completion_separator('<'));
        assert!(crate::completion::is_completion_separator('>'));
        assert!(crate::completion::is_completion_separator('!'));
        assert!(crate::completion::is_completion_separator('+'));
        assert!(crate::completion::is_completion_separator('-'));
        assert!(crate::completion::is_completion_separator('*'));
        assert!(crate::completion::is_completion_separator('/'));
        // バッククォートはPostgreSQLでは識別子クォートに使用しないため区切り文字でない
        assert!(!crate::completion::is_completion_separator('`'));
        assert!(crate::completion::is_completion_separator('\''));
        assert!(crate::completion::is_completion_separator('"'));
        // 単語文字
        assert!(!crate::completion::is_completion_separator('a'));
        assert!(!crate::completion::is_completion_separator('Z'));
        assert!(!crate::completion::is_completion_separator('0'));
        assert!(!crate::completion::is_completion_separator('9'));
        assert!(!crate::completion::is_completion_separator('_'));
        assert!(!crate::completion::is_completion_separator('あ')); // マルチバイト
        assert!(!crate::completion::is_completion_separator('テ'));
    }

    // word_left のテスト

    #[test]
    fn test_word_left_basic() {
        // "SELECT * FROM users"
        //  chars: S(0)E(1)L(2)E(3)C(4)T(5)' '(6)*(7)' '(8)F(9)R(10)O(11)M(12)' '(13)u(14)s(15)e(16)r(17)s(18)
        //  len = 19
        let app = make_app_with_input("SELECT * FROM users");

        // 末尾(19) → "users" の先頭(14)
        assert_eq!(app.word_left(19), 14);
        // "users" の先頭(14) → "FROM" の先頭(9): ' '(13)スキップ後 M(12)R(11)O(10)F(9) と遡る
        assert_eq!(app.word_left(14), 9);
        // "FROM" の先頭(9) → "SELECT" の先頭(0): ' '(8)と*(7)をスキップ後 T(5)..S(0) と遡る
        assert_eq!(app.word_left(9), 0);
    }

    #[test]
    fn test_word_left_from_zero() {
        let app = make_app_with_input("SELECT");
        // 位置0からの word_left は0を返す
        assert_eq!(app.word_left(0), 0);
    }

    #[test]
    fn test_word_left_multibyte() {
        // "SELECT * FROM テーブル WHERE id = 1"
        // テーブル は4文字(char単位)
        let input = "SELECT * FROM テーブル WHERE id = 1";
        let app = make_app_with_input(input);
        let len = input.chars().count();

        // 末尾から word_left を呼ぶと "1" をスキップして "=" の手前（空白スキップして id）に移動
        // "1" は1文字, " " は区切り, "=" は区切り, " " は区切り, "id" へ
        // 具体的な位置: "1"(末尾文字)の後ろ=len, 1文字前=len-1 は "1"(単語文字), その前は " "(区切り)
        // word_left(len) → "1" をスキップ → len-1 が "1" で単語文字, さらに前へ → " " は区切り → len-1
        assert_eq!(app.word_left(len), len - 1);

        // "テーブル" の末尾位置から word_left すると先頭("テ")に移動する
        // "SELECT * FROM " は 14文字 (0-13)、テーブルは chars[14..18]、末尾は18
        let table_end = 14 + 4; // 18
        let table_start = 14;
        assert_eq!(app.word_left(table_end), table_start);
    }

    #[test]
    fn test_word_left_consecutive_separators() {
        // 連続する区切り文字をスキップすること
        // "a  =  b" -> 末尾(7)から word_left → "b" をスキップして "a" の次へ
        let app = make_app_with_input("a  =  b");
        // 末尾(7) → b(6) は単語文字, その前 5..2 は区切り, a(0) は単語文字 → 0
        assert_eq!(app.word_left(7), 6);
        // b の手前(6) → 5,4,3 は区切り文字("  =") → a(0..1) を遡る → 0
        assert_eq!(app.word_left(6), 0);
    }

    // word_right のテスト

    #[test]
    fn test_word_right_basic() {
        // "SELECT * FROM users"
        //  chars: S(0)E(1)L(2)E(3)C(4)T(5)' '(6)*(7)' '(8)F(9)R(10)O(11)M(12)' '(13)u(14)s(15)e(16)r(17)s(18)
        //  len = 19
        let app = make_app_with_input("SELECT * FROM users");

        // 先頭(0) → "SELECT" の末尾(6): T(5) の次 = 6
        assert_eq!(app.word_right(0), 6);
        // (6) → ' '(6)・*(7)・' '(8) をスキップ後 F(9)R(10)O(11)M(12) と進む → 13
        assert_eq!(app.word_right(6), 13);
        // (13) → ' '(13) スキップ後 u(14)..s(18) と進む → 19
        assert_eq!(app.word_right(13), 19);
    }

    #[test]
    fn test_word_right_from_end() {
        let app = make_app_with_input("SELECT");
        let len = "SELECT".chars().count(); // 6
                                            // 末尾からの word_right は末尾のまま
        assert_eq!(app.word_right(len), len);
    }

    #[test]
    fn test_word_right_multibyte() {
        // "SELECT テーブル WHERE"
        // "SELECT "(7文字) + "テーブル"(4文字) + " WHERE"(6文字)
        let input = "SELECT テーブル WHERE";
        let app = make_app_with_input(input);

        // 先頭(0) → "SELECT" の末尾(6)
        assert_eq!(app.word_right(0), 6);
        // "SELECT" の末尾(6) → "テーブル" の末尾(11): 空白スキップ後にテーブルを進む
        // " "(6)はスキップ, テ(7)ー(8)ブ(9)ル(10)=末尾11
        assert_eq!(app.word_right(6), 11);
        // "テーブル" の末尾(11) → "WHERE" の末尾(17)
        assert_eq!(app.word_right(11), 17);
    }

    #[test]
    fn test_word_right_consecutive_separators() {
        // 連続する区切り文字をスキップすること
        // "a  =  b": a(0), ' '(1), ' '(2), '='(3), ' '(4), ' '(5), b(6)
        let app = make_app_with_input("a  =  b");
        // 先頭(0) → a(0)は単語文字, 次(1)は区切り → a の末尾(1)
        assert_eq!(app.word_right(0), 1);
        // (1) → 1,2,3,4,5 は区切り文字 → b(6) を進む → 7
        assert_eq!(app.word_right(1), 7);
    }

    // extract_search_path_from_set_sql のテスト

    #[test]
    fn test_extract_search_path_to_form() {
        // SET search_path TO schema_name 形式
        assert_eq!(
            extract_search_path_from_set_sql("SET search_path TO myschema"),
            Some("myschema".to_string())
        );
    }

    #[test]
    fn test_extract_search_path_equals_form() {
        // SET search_path = schema_name 形式
        assert_eq!(
            extract_search_path_from_set_sql("SET search_path = myschema"),
            Some("myschema".to_string())
        );
    }

    #[test]
    fn test_extract_search_path_quoted() {
        // ダブルクォートで囲まれたスキーマ名
        assert_eq!(
            extract_search_path_from_set_sql(r#"SET search_path TO "MySchema""#),
            Some("MySchema".to_string())
        );
    }

    #[test]
    fn test_extract_search_path_multiple_schemas() {
        // カンマ区切りで複数のスキーマが指定された場合は最初のスキーマを返す
        assert_eq!(
            extract_search_path_from_set_sql("SET search_path TO myschema, public"),
            Some("myschema".to_string())
        );
    }

    #[test]
    fn test_extract_search_path_unrelated_set() {
        // search_path に関係のない SET 文は None を返す
        assert_eq!(
            extract_search_path_from_set_sql("SET timezone = 'UTC'"),
            None
        );
    }

    #[test]
    fn test_extract_search_path_case_insensitive() {
        // 大文字小文字を区別せず解析できること
        assert_eq!(
            extract_search_path_from_set_sql("SET SEARCH_PATH TO myschema"),
            Some("myschema".to_string())
        );
    }

    // normalize_sql_to_oneliner のテスト

    #[test]
    fn test_normalize_sql_newline_to_space() {
        let sql = "SELECT *\nFROM users\nWHERE id = 1";
        assert_eq!(
            normalize_sql_to_oneliner(sql),
            "SELECT * FROM users WHERE id = 1"
        );
    }

    #[test]
    fn test_normalize_sql_crlf() {
        let sql = "SELECT *\r\nFROM users";
        assert_eq!(normalize_sql_to_oneliner(sql), "SELECT * FROM users");
    }

    #[test]
    fn test_normalize_sql_cr_only() {
        let sql = "SELECT *\rFROM users";
        assert_eq!(normalize_sql_to_oneliner(sql), "SELECT * FROM users");
    }

    #[test]
    fn test_normalize_sql_consecutive_spaces() {
        // 改行後の連続スペース（インデント）も圧縮される
        let sql = "SELECT *\n  FROM users\n  WHERE id = 1";
        assert_eq!(
            normalize_sql_to_oneliner(sql),
            "SELECT * FROM users WHERE id = 1"
        );
    }

    #[test]
    fn test_normalize_sql_trim() {
        let sql = "\n  SELECT 1  \n";
        assert_eq!(normalize_sql_to_oneliner(sql), "SELECT 1");
    }

    #[test]
    fn test_normalize_sql_already_oneliner() {
        // すでに1行の場合はそのまま返す
        let sql = "SELECT * FROM users WHERE id = 1";
        assert_eq!(normalize_sql_to_oneliner(sql), sql);
    }
}
