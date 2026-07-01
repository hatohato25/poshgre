use crossterm::event::{self, Event, KeyCode, KeyModifiers};

use crate::error::Result;
use crate::i18n::TuiMsg;
use crate::t;

use super::{App, AppState, CompletionState, InputFocus, MAX_SQL_HISTORY};
use crate::query::is_write_sql;

impl App {
    /// イベント処理
    pub(super) async fn handle_event(&mut self, event: Event) -> Result<()> {
        if let Event::Key(key_event) = event {
            // Ctrl+C で終了（全状態共通）
            // Connected状態では選択範囲コピー処理を優先するため handle_connected_input に委譲する
            if key_event.code == KeyCode::Char('c')
                && key_event.modifiers.contains(KeyModifiers::CONTROL)
                && !matches!(self.state, AppState::Connected { .. })
            {
                // abort() のみ呼び take() しない（run() 側で is_some() を確認して process::exit するため）
                if let Some(ref task) = self.connecting_task {
                    task.abort();
                }
                self.should_quit = true;
                return Ok(());
                // Connected状態の場合はhandle_connected_inputに流す
            }

            // 状態別のキーハンドリング
            match &self.state {
                AppState::Selecting { .. } => {
                    // Selecting状態はrun()で既に処理済みのため到達しない
                }
                AppState::Connected { .. } => self.handle_connected_input(key_event).await?,
                AppState::Connecting { .. }
                | AppState::Executing { .. }
                | AppState::StreamingQuery { .. }
                | AppState::SelectingColumns { .. } => {
                    // 接続中・実行中・ストリーミング待ち・カラム選択中は入力を受け付けない
                }
                // ShowingResultはskimへの即遷移トリガーのみで使われるため、キー入力は到達しない
                AppState::ShowingResult { .. } => {}
                AppState::Error { .. } => self.handle_error_input(key_event),
            }
        }

        Ok(())
    }

    /// 接続済み状態のキー入力処理
    pub(super) async fn handle_connected_input(
        &mut self,
        key_event: event::KeyEvent,
    ) -> Result<()> {
        // Shell フォーカス中は Shell 専用ハンドラに委譲する
        if self.input_focus == InputFocus::Shell {
            return self.handle_shell_input(key_event).await;
        }

        // PROMPT フォーカス中は PROMPT 専用ハンドラに委譲する
        if self.input_focus == InputFocus::Prompt {
            return self.handle_prompt_input(key_event).await;
        }

        // 補完ポップアップ表示中は専用ハンドラに優先委譲する
        // 補完ハンドラが消費したキーは以降の処理に伝播させない
        if self.sql.completion_state.is_some() && self.handle_completion_key(key_event).await? {
            return Ok(());
        }

        // APIキーが設定されているかを事前に判定する（フォーカス循環に使用）
        let has_api_key = self
            .settings
            .anthropic_api_key
            .as_ref()
            .map(|k| !k.as_str().is_empty())
            .unwrap_or(false);

        match key_event.code {
            // Tab（補完ポップアップ非表示時）: Sql → Shell → Prompt の順にフォーカスを循環させる
            // APIキー未設定時は Prompt をスキップして Sql → Shell → Sql の2段循環にする
            KeyCode::Tab if self.sql.completion_state.is_none() => {
                self.input_focus = InputFocus::Shell;
                return Ok(());
            }
            // Shift+Tab: 逆順でフォーカスを循環させる（Sql → Prompt → Shell → Sql）
            // APIキー未設定時は Prompt をスキップして Sql → Shell → Sql の2段逆循環にする
            KeyCode::BackTab if self.sql.completion_state.is_none() => {
                // APIキーがある場合のみ Prompt にフォーカスを移す
                if has_api_key {
                    self.input_focus = InputFocus::Prompt;
                } else {
                    self.input_focus = InputFocus::Shell;
                }
                return Ok(());
            }
            // Ctrl+T: テーブル一覧をストリーミング表示
            KeyCode::Char('t') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.sql.text = "SELECT tablename FROM pg_catalog.pg_tables WHERE schemaname = current_schema() ORDER BY tablename".to_string();
                self.sql.cursor_position = self.sql.text.chars().count();
                self.selected_record = None;
                self.sql.selection_start = None;
                self.transition_to_streaming()?;
            }
            // Ctrl+S: skimベースのカラム選択（テーブル一覧 → テーブル選択 → カラム一覧 → カラム選択）
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.selected_record = None;
                self.sql.selection_start = None;
                self.transition_to_column_select()?;
            }
            // Ctrl+C: 選択範囲をクリップボードにコピー（選択なしの場合は終了）
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some((byte_start, byte_end)) = self.selection_byte_range() {
                    let selected_text = &self.sql.text[byte_start..byte_end];
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(selected_text.to_string());
                    }
                    // コピー後は選択解除
                    self.sql.selection_start = None;
                } else {
                    // 選択範囲がない場合は従来のCtrl+C動作（終了）
                    self.should_quit = true;
                }
            }
            // Ctrl+X: 選択範囲をカット
            KeyCode::Char('x') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some((byte_start, byte_end)) = self.selection_byte_range() {
                    let selected_text = self.sql.text[byte_start..byte_end].to_string();
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(selected_text);
                    }
                    // 選択範囲を削除してカーソルを選択開始位置に移動
                    let cursor_start = self
                        .sql
                        .selection_start
                        .unwrap_or(self.sql.cursor_position)
                        .min(self.sql.cursor_position);
                    self.sql.text.replace_range(byte_start..byte_end, "");
                    self.sql.cursor_position = cursor_start;
                    self.sql.selection_start = None;
                }
            }
            // Ctrl+V: クリップボードからペースト
            // SQL入力欄は1行のため、改行文字はスペースに変換して挿入する
            KeyCode::Char('v') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                // 選択範囲があれば先に削除（上書きペースト）
                self.delete_selection();
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        let sanitized = text.replace('\n', " ").replace('\r', "");
                        let byte_pos = self.char_to_byte(self.sql.cursor_position);
                        self.sql.text.insert_str(byte_pos, &sanitized);
                        self.sql.cursor_position += sanitized.chars().count();
                    }
                }
            }
            // Ctrl+A: 全選択
            // 従来の「カーソルを行頭へ」から変更し、全テキストを選択状態にする
            KeyCode::Char('a') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.sql.selection_start = Some(0);
                self.sql.cursor_position = self.sql.text.chars().count();
            }
            // Ctrl+E: カーソルを行末へ
            KeyCode::Char('e') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.sql.cursor_position = self.sql.text.chars().count();
                self.sql.selection_start = None;
            }
            // Ctrl+K: カーソル位置から行末までを削除（kill-line）
            // 選択範囲がある場合は選択範囲を削除する
            KeyCode::Char('k') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.sql.selection_start.is_some() {
                    self.delete_selection();
                } else {
                    let byte_start = self.char_to_byte(self.sql.cursor_position);
                    // 削除範囲を kill_buffer に保存する
                    self.sql.kill_buffer = self.sql.text[byte_start..].to_string();
                    self.sql.text.truncate(byte_start);
                    // cursor_position はそのまま（行末に到達した状態）
                }
            }
            // Ctrl+W: カーソル直前の単語を削除（bash readline backward-kill-word）
            // Opt+Backspace と同じ動作（Linux環境での互換性）
            KeyCode::Char('w') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.sql.selection_start.is_some() {
                    self.delete_selection();
                } else {
                    let new_pos = self.word_left(self.sql.cursor_position);
                    if new_pos < self.sql.cursor_position {
                        let byte_start = self.char_to_byte(new_pos);
                        let byte_end = self.char_to_byte(self.sql.cursor_position);
                        self.sql.text.replace_range(byte_start..byte_end, "");
                        self.sql.cursor_position = new_pos;
                    }
                }
            }
            // Ctrl+U: 行頭からカーソル位置までを削除（kill-whole-line 前半）
            // 選択範囲がある場合は選択範囲を削除する
            KeyCode::Char('u') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.sql.selection_start.is_some() {
                    self.delete_selection();
                } else {
                    let byte_end = self.char_to_byte(self.sql.cursor_position);
                    // 削除範囲を kill_buffer に保存する
                    self.sql.kill_buffer = self.sql.text[..byte_end].to_string();
                    self.sql.text.replace_range(..byte_end, "");
                    self.sql.cursor_position = 0;
                }
            }
            // Ctrl+Y: キルバッファからペースト（yank）
            // Ctrl+V はシステムクリップボードからのペーストのまま維持する
            KeyCode::Char('y') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.delete_selection();
                if !self.sql.kill_buffer.is_empty() {
                    let byte_pos = self.char_to_byte(self.sql.cursor_position);
                    let yanked = self.sql.kill_buffer.clone();
                    self.sql.text.insert_str(byte_pos, &yanked);
                    self.sql.cursor_position += yanked.chars().count();
                }
            }
            // Opt+Shift+← / Alt+Shift+←: 選択しながら1単語左へ移動
            // ALT+SHIFT 複合修飾子は ALT 単独・SHIFT 単独より前に配置してマッチ優先度を確保する
            KeyCode::Left
                if key_event.modifiers.contains(KeyModifiers::ALT)
                    && key_event.modifiers.contains(KeyModifiers::SHIFT) =>
            {
                if self.sql.selection_start.is_none() {
                    self.sql.selection_start = Some(self.sql.cursor_position);
                }
                self.sql.cursor_position = self.word_left(self.sql.cursor_position);
            }
            // Opt+Shift+→ / Alt+Shift+→: 選択しながら1単語右へ移動
            KeyCode::Right
                if key_event.modifiers.contains(KeyModifiers::ALT)
                    && key_event.modifiers.contains(KeyModifiers::SHIFT) =>
            {
                if self.sql.selection_start.is_none() {
                    self.sql.selection_start = Some(self.sql.cursor_position);
                }
                self.sql.cursor_position = self.word_right(self.sql.cursor_position);
            }
            // Shift+Left: 選択しながら左へ移動
            // 通常のLeftより前に配置してShift修飾子付きが先にマッチするようにする
            KeyCode::Left
                if key_event.modifiers.contains(KeyModifiers::SHIFT)
                    && self.sql.cursor_position > 0 =>
            {
                // 選択中でなければ現在位置を選択開始点として記録
                if self.sql.selection_start.is_none() {
                    self.sql.selection_start = Some(self.sql.cursor_position);
                }
                self.sql.cursor_position -= 1;
            }
            // Shift+Right: 選択しながら右へ移動
            KeyCode::Right if key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                let char_count = self.sql.text.chars().count();
                if self.sql.cursor_position < char_count {
                    if self.sql.selection_start.is_none() {
                        self.sql.selection_start = Some(self.sql.cursor_position);
                    }
                    self.sql.cursor_position += 1;
                }
            }
            // Shift+Home: 選択しながら行頭へ
            KeyCode::Home if key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                if self.sql.selection_start.is_none() {
                    self.sql.selection_start = Some(self.sql.cursor_position);
                }
                self.sql.cursor_position = 0;
            }
            // Shift+End: 選択しながら行末へ
            KeyCode::End if key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                if self.sql.selection_start.is_none() {
                    self.sql.selection_start = Some(self.sql.cursor_position);
                }
                self.sql.cursor_position = self.sql.text.chars().count();
            }
            // Opt+← / Alt+←: 1単語左へ移動（選択解除）
            // iTerm2 / crossterm 標準パターン
            KeyCode::Left if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.sql.selection_start = None;
                self.sql.cursor_position = self.word_left(self.sql.cursor_position);
            }
            // ESC b（Terminal.app の meta-key 送信形式）
            KeyCode::Char('b') if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.sql.selection_start = None;
                self.sql.cursor_position = self.word_left(self.sql.cursor_position);
            }
            // Opt+→ / Alt+→: 1単語右へ移動（選択解除）
            // iTerm2 / crossterm 標準パターン
            KeyCode::Right if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.sql.selection_start = None;
                self.sql.cursor_position = self.word_right(self.sql.cursor_position);
            }
            // ESC f（Terminal.app の meta-key 送信形式）
            KeyCode::Char('f') if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.sql.selection_start = None;
                self.sql.cursor_position = self.word_right(self.sql.cursor_position);
            }
            // query_inputが空の時のみqで終了（SQL入力中はqを文字として扱う）
            KeyCode::Char('q') if self.sql.text.is_empty() => {
                self.should_quit = true;
            }
            KeyCode::Char(c) => {
                // 選択範囲があれば先に削除（選択テキストを上書き入力）
                self.delete_selection();
                // カーソル位置（char単位）をバイト位置に変換して文字を挿入する
                let byte_pos = self.char_to_byte(self.sql.cursor_position);
                self.sql.text.insert(byte_pos, c);
                self.sql.cursor_position += 1;
            }
            // Opt+Backspace / Alt+Backspace: カーソル直前の単語を削除
            // 選択範囲がある場合は選択範囲を削除する
            KeyCode::Backspace if key_event.modifiers.contains(KeyModifiers::ALT) => {
                if self.sql.selection_start.is_some() {
                    self.delete_selection();
                } else {
                    let new_pos = self.word_left(self.sql.cursor_position);
                    if new_pos < self.sql.cursor_position {
                        let byte_start = self.char_to_byte(new_pos);
                        let byte_end = self.char_to_byte(self.sql.cursor_position);
                        self.sql.text.replace_range(byte_start..byte_end, "");
                        self.sql.cursor_position = new_pos;
                    }
                }
            }
            KeyCode::Backspace => {
                if self.sql.selection_start.is_some() {
                    // 選択範囲があれば選択範囲全体を削除
                    self.delete_selection();
                } else {
                    // カーソル位置の直前の文字を削除する
                    if self.sql.cursor_position > 0 {
                        let byte_pos = self.char_to_byte(self.sql.cursor_position - 1);
                        self.sql.text.remove(byte_pos);
                        self.sql.cursor_position -= 1;
                    }
                }
            }
            // Opt+Delete / Alt+Delete: カーソル直後の単語を削除
            // 選択範囲がある場合は選択範囲を削除する
            KeyCode::Delete if key_event.modifiers.contains(KeyModifiers::ALT) => {
                if self.sql.selection_start.is_some() {
                    self.delete_selection();
                } else {
                    let new_pos = self.word_right(self.sql.cursor_position);
                    if new_pos > self.sql.cursor_position {
                        let byte_start = self.char_to_byte(self.sql.cursor_position);
                        let byte_end = self.char_to_byte(new_pos);
                        self.sql.text.replace_range(byte_start..byte_end, "");
                        // cursor_position はそのまま（次の単語が繰り上がる）
                    }
                }
            }
            KeyCode::Delete => {
                if self.sql.selection_start.is_some() {
                    // 選択範囲があれば選択範囲全体を削除
                    self.delete_selection();
                } else {
                    // カーソル位置の文字を削除する（Deleteキー）
                    let char_count = self.sql.text.chars().count();
                    if self.sql.cursor_position < char_count {
                        let byte_pos = self.char_to_byte(self.sql.cursor_position);
                        self.sql.text.remove(byte_pos);
                        // cursor_positionはそのまま（次の文字が繰り上がる）
                    }
                }
            }
            KeyCode::Left => {
                self.sql.selection_start = None;
                if self.sql.cursor_position > 0 {
                    self.sql.cursor_position -= 1;
                }
            }
            KeyCode::Right => {
                self.sql.selection_start = None;
                let char_count = self.sql.text.chars().count();
                if self.sql.cursor_position < char_count {
                    self.sql.cursor_position += 1;
                }
            }
            KeyCode::Home => {
                self.sql.selection_start = None;
                self.sql.cursor_position = 0;
            }
            KeyCode::End => {
                self.sql.selection_start = None;
                self.sql.cursor_position = self.sql.text.chars().count();
            }
            KeyCode::Enter if !self.sql.text.trim().is_empty() => {
                // sc エイリアス: カラム選択モードに遷移（expand_aliases より前に処理する）
                if self.sql.text.trim().to_lowercase() == "sc" {
                    self.sql.text.clear();
                    self.sql.cursor_position = 0;
                    self.selected_record = None;
                    self.sql.selection_start = None;
                    self.transition_to_column_select()?;
                    return Ok(());
                }

                // 新しいクエリ実行時は選択レコードプレビューをクリア
                self.selected_record = None;
                // エイリアス展開してから実行
                self.expand_aliases();

                self.add_to_history(&self.sql.text.clone());

                // エイリアス展開後にカーソルを末尾に合わせる
                self.sql.cursor_position = self.sql.text.chars().count();

                // readonlyモード: 書き込み系SQLをクライアント側で即座にブロックする
                // サーバー側でもブロックされるが、ユーザーへの即時フィードバックのため先にチェックする
                if self.is_current_readonly() && is_write_sql(&self.sql.text) {
                    let current_state = std::mem::replace(
                        &mut self.state,
                        AppState::Selecting {
                            connections: Vec::new(),
                            selected_index: 0,
                        },
                    );
                    self.state = AppState::Error {
                        message: t!(TuiMsg::ReadonlyBlocked),
                        previous_state: Box::new(current_state),
                    };
                    return Ok(());
                }

                let sql_upper = self.sql.text.trim().to_uppercase();
                if sql_upper.starts_with("SET ") {
                    // SETコマンドは結果表示不要なのでバックグラウンドクエリ実行
                    self.execute_query()?;
                } else {
                    // 表示クエリはストリーミングモードへ遷移
                    self.transition_to_streaming()?;
                }
            }
            // ↑キー: 履歴を遡る（古い方向へ）
            KeyCode::Up => {
                if self.sql.history.is_empty() {
                    return Ok(());
                }
                match self.sql.history_index {
                    None => {
                        // 新規入力中 → 現在の入力を退避して最新の履歴を表示
                        self.sql.history_draft = self.sql.text.clone();
                        let idx = self.sql.history.len() - 1;
                        self.sql.history_index = Some(idx);
                        self.sql.text = self.sql.history[idx].clone();
                    }
                    Some(idx) if idx > 0 => {
                        // 履歴参照中 → さらに古い履歴へ
                        let new_idx = idx - 1;
                        self.sql.history_index = Some(new_idx);
                        self.sql.text = self.sql.history[new_idx].clone();
                    }
                    _ => {
                        // 最古の履歴に到達済み → 何もしない
                        return Ok(());
                    }
                }
                self.sql.cursor_position = self.sql.text.chars().count();
                self.sql.selection_start = None;
            }
            // ↓キー: 履歴を進む（新しい方向へ）
            KeyCode::Down => {
                match self.sql.history_index {
                    Some(idx) => {
                        if idx + 1 < self.sql.history.len() {
                            // より新しい履歴へ
                            let new_idx = idx + 1;
                            self.sql.history_index = Some(new_idx);
                            self.sql.text = self.sql.history[new_idx].clone();
                        } else {
                            // 履歴の末尾を超えた → 退避した入力を復元して新規入力状態に戻す
                            self.sql.history_index = None;
                            self.sql.text = self.sql.history_draft.clone();
                            self.sql.history_draft.clear();
                        }
                        self.sql.cursor_position = self.sql.text.chars().count();
                        self.sql.selection_start = None;
                    }
                    None => {
                        // 新規入力中 → 何もしない
                    }
                }
            }
            // WHEREテンプレート+レコードプレビュー表示中にESCで通常のSQL入力画面に戻る
            KeyCode::Esc => {
                self.sql.text.clear();
                self.sql.cursor_position = 0;
                self.selected_record = None;
            }
            _ => {}
        }

        // 文字入力・Backspace・Delete の後に補完候補を更新する
        // 非入力キー（移動キー等）では候補が変わらないだけで副作用はない
        self.update_completion_state().await;

        Ok(())
    }

    /// 補完ポップアップ表示中のキー処理
    ///
    /// キーを消費した場合 true を返す（呼び出し元がこれ以上処理しないよう通知）
    pub(super) async fn handle_completion_key(
        &mut self,
        key_event: event::KeyEvent,
    ) -> Result<bool> {
        match key_event.code {
            // Tab / ↓: 次の候補へ（ラップアラウンド）
            KeyCode::Tab | KeyCode::Down => {
                if let Some(ref mut state) = self.sql.completion_state {
                    if state.candidates.is_empty() {
                        self.sql.completion_state = None;
                    } else {
                        state.selected_index = (state.selected_index + 1) % state.candidates.len();
                    }
                }
                return Ok(true);
            }
            // Shift+Tab: 前の候補へ（ラップアラウンド）
            KeyCode::BackTab => {
                if let Some(ref mut state) = self.sql.completion_state {
                    if !state.candidates.is_empty() {
                        let len = state.candidates.len();
                        state.selected_index = (state.selected_index + len - 1) % len;
                    }
                }
                return Ok(true);
            }
            // ↑（ポップアップ表示中のみ）: 前の候補へ
            // 履歴ナビゲーションより補完を優先する
            KeyCode::Up if self.sql.completion_state.is_some() => {
                if let Some(ref mut state) = self.sql.completion_state {
                    if !state.candidates.is_empty() {
                        let len = state.candidates.len();
                        state.selected_index = (state.selected_index + len - 1) % len;
                    }
                }
                return Ok(true);
            }
            // Enter（ポップアップ表示中のみ）: 候補確定（SQL実行は行わない）
            KeyCode::Enter if self.sql.completion_state.is_some() => {
                self.confirm_completion();
                return Ok(true);
            }
            // Esc: ポップアップを閉じる（非表示時は既存の Esc 処理へ流す）
            KeyCode::Esc => {
                if self.sql.completion_state.is_some() {
                    self.sql.completion_state = None;
                    return Ok(true);
                }
                return Ok(false);
            }
            _ => {}
        }
        Ok(false)
    }

    /// 選択中の補完候補を確定して入力欄に挿入する
    pub(super) fn confirm_completion(&mut self) {
        let Some(state) = self.sql.completion_state.take() else {
            return;
        };
        let Some(item) = state.candidates.get(state.selected_index) else {
            return;
        };

        // カーソル位置のバイトオフセットを計算して現在トークンを置換する
        let cursor_byte = self.char_to_byte(self.sql.cursor_position);
        let (_, token_start_byte) =
            crate::completion::current_token_with_pos(&self.sql.text, cursor_byte);

        // 既存トークンを削除して補完テキストを挿入する
        let completion_text = item.text.clone();
        self.sql
            .text
            .replace_range(token_start_byte..cursor_byte, &completion_text);
        // 挿入後のカーソル位置 = トークン開始位置 + 挿入テキストのchar数
        let inserted_chars = completion_text.chars().count();
        let token_start_char = self.sql.text[..token_start_byte].chars().count();
        self.sql.cursor_position = token_start_char + inserted_chars;
        // 選択状態をリセット（補完後に選択範囲が残らないようにする）
        self.sql.selection_start = None;
    }

    /// 補完候補リストを現在の入力状態に基づいて更新する
    ///
    /// 文字入力・削除のたびに呼ばれる。キャッシュはRwLock経由で非同期に参照する。
    pub(super) async fn update_completion_state(&mut self) {
        let cursor_byte = self.char_to_byte(self.sql.cursor_position);
        let sql_before = self.sql.text[..cursor_byte].to_string();
        let (token_ref, token_start) =
            crate::completion::current_token_with_pos(&self.sql.text, cursor_byte);
        let token = token_ref.to_string();

        // トークンが空でポップアップ非表示の場合は表示しない（1文字目から補完を表示する）
        if token.is_empty() && self.sql.completion_state.is_none() {
            return;
        }
        // トークンが空でポップアップ表示中の場合は閉じる
        if token.is_empty() {
            self.sql.completion_state = None;
            return;
        }

        let context = crate::completion::analyze_context(&sql_before);

        // db.table パターンの検出: トークン開始位置の直前が '.' の場合
        // 例: "SELECT * FROM warehouse.b" → qualified_db = Some("warehouse")
        let qualified_db: Option<String> = if token_start > 0 {
            let before_dot = &self.sql.text[..token_start];
            if let Some(db_part) = before_dot.strip_suffix('.') {
                db_part
                    .rsplit(|c: char| c.is_whitespace())
                    .next()
                    .map(|s| s.trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        } else {
            None
        };

        // schema.table パターン（qualified_db が Some）の場合は DatabaseTableName コンテキストに変換する。
        let context = if let Some(ref db_name) = qualified_db {
            // schema.table パターン: 指定スキーマのテーブル名コンテキストに変換する
            crate::completion::SqlContext::DatabaseTableName {
                database: db_name.clone(),
            }
        } else {
            context
        };

        // db.table パターン: 指定DBのテーブルキャッシュを取得する
        // spawnではなくawaitで完了を待つことで、キャッシュ未充填のまま候補生成に進むのを防ぐ
        // キャッシュ済みの場合は即座にreturnするため2回目以降のパフォーマンス影響はない
        if let crate::completion::SqlContext::DatabaseTableName { ref database } = context {
            let cache_arc = self.sql.completion_cache.clone();
            let db_name_clone = database.clone();
            if let AppState::Connected { ref manager } = self.state {
                let pool = manager.pool().clone();
                crate::completion::fetch_database_tables_if_needed(
                    &cache_arc,
                    &pool,
                    &db_name_clone,
                )
                .await;
            }
        }

        // カラム補完でテーブルが特定された場合、カラムキャッシュを取得する
        // spawnではなくawaitで完了を待つことで、キャッシュ未充填のまま候補生成に進むのを防ぐ
        // キャッシュ済みの場合は即座にreturnするため2回目以降のパフォーマンス影響はない
        if let crate::completion::SqlContext::ColumnName {
            table: Some(ref table_name),
        } = context
        {
            let cache_arc = self.sql.completion_cache.clone();
            let table_name = table_name.clone();
            if let AppState::Connected { ref manager } = self.state {
                let pool = manager.pool().clone();
                crate::completion::fetch_column_cache_if_needed(&cache_arc, &pool, &table_name)
                    .await;
            }
        }

        let cache = self.sql.completion_cache.read().await;

        let candidates = crate::completion::get_candidates(&token, &context, &cache);

        if candidates.is_empty() {
            self.sql.completion_state = None;
            return;
        }

        // 既存ポップアップの選択位置を維持しつつ候補リストを更新する
        let selected_index = self
            .sql
            .completion_state
            .as_ref()
            .map(|s| s.selected_index.min(candidates.len().saturating_sub(1)))
            .unwrap_or(0);

        self.sql.completion_state = Some(CompletionState {
            candidates,
            selected_index,
            current_token: token,
        });
    }

    /// SQL実行履歴に追加する
    ///
    /// 直前と同じクエリは重複追加しない。最大MAX_SQL_HISTORY件を保持する。
    pub(super) fn add_to_history(&mut self, sql: &str) {
        let sql = sql.trim().to_string();
        if sql.is_empty() {
            return;
        }
        if self.sql.history.back().map(|s| s.as_str()) != Some(&sql) {
            self.sql.history.push_back(sql);
            if self.sql.history.len() > MAX_SQL_HISTORY {
                self.sql.history.pop_front();
            }
        }
        // 履歴参照状態をリセット（実行後は新規入力状態に戻す）
        self.sql.history_index = None;
        self.sql.history_draft.clear();
    }

    /// カーソルを1単語左に移動した時の位置（char単位）を返す
    ///
    /// bash readline の backward-word に相当する動作:
    /// 1. 現在位置の直前にある区切り文字をスキップ
    /// 2. 単語文字が続く間遡る
    pub(super) fn word_left(&self, from: usize) -> usize {
        if from == 0 {
            return 0;
        }
        let chars: Vec<char> = self.sql.text.chars().collect();
        let mut pos = from;

        // まず区切り文字をスキップ
        while pos > 0 && crate::completion::is_completion_separator(chars[pos - 1]) {
            pos -= 1;
        }
        // 次に単語文字を遡る
        while pos > 0 && !crate::completion::is_completion_separator(chars[pos - 1]) {
            pos -= 1;
        }
        pos
    }

    /// カーソルを1単語右に移動した時の位置（char単位）を返す
    ///
    /// bash readline の forward-word に相当する動作:
    /// 1. 現在位置の直後にある区切り文字をスキップ
    /// 2. 単語文字が続く間進む
    pub(super) fn word_right(&self, from: usize) -> usize {
        let chars: Vec<char> = self.sql.text.chars().collect();
        let len = chars.len();
        if from >= len {
            return len;
        }
        let mut pos = from;

        // まず区切り文字をスキップ
        while pos < len && crate::completion::is_completion_separator(chars[pos]) {
            pos += 1;
        }
        // 次に単語文字を進む
        while pos < len && !crate::completion::is_completion_separator(chars[pos]) {
            pos += 1;
        }
        pos
    }

    /// query_inputのchar位置をバイト位置に変換する
    ///
    /// char_indices()でO(n)だが入力文字列は通常短いため許容範囲。
    pub(super) fn char_to_byte(&self, char_pos: usize) -> usize {
        self.sql
            .text
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.sql.text.len())
    }

    /// 選択範囲のバイト範囲を返す（選択なしの場合はNone）
    pub(super) fn selection_byte_range(&self) -> Option<(usize, usize)> {
        let sel_start = self.sql.selection_start?;
        let start = sel_start.min(self.sql.cursor_position);
        let end = sel_start.max(self.sql.cursor_position);
        // start==0のときchar_to_byteは0を返すが、unwrap_or(0)と等価
        let byte_start = self.char_to_byte(start);
        let byte_end = self.char_to_byte(end);
        Some((byte_start, byte_end))
    }

    /// 選択範囲のテキストを削除し、カーソルを選択開始位置に移動する
    ///
    /// selection_startがNoneの場合は何もしない。
    /// 呼び出し後はselection_startがNoneになる。
    pub(super) fn delete_selection(&mut self) {
        if let Some((byte_start, byte_end)) = self.selection_byte_range() {
            let start = self
                .sql
                .selection_start
                .unwrap_or(self.sql.cursor_position)
                .min(self.sql.cursor_position);
            self.sql.text.replace_range(byte_start..byte_end, "");
            self.sql.cursor_position = start;
            self.sql.selection_start = None;
        }
    }

    /// SQL入力のエイリアスを展開する
    ///
    /// 短縮形を完全なSQL文に変換することで入力効率を向上させる
    pub(super) fn expand_aliases(&mut self) {
        let expanded = match self.sql.text.trim().to_lowercase().as_str() {
            "ss" => "SELECT schema_name FROM information_schema.schemata WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast') ORDER BY schema_name".to_string(),
            "st" => "SELECT tablename FROM pg_catalog.pg_tables WHERE schemaname = current_schema() ORDER BY tablename".to_string(),
            _ => return,
        };
        self.sql.text = expanded;
    }

    /// エラー状態のキー入力処理
    pub(super) fn handle_error_input(&mut self, key_event: event::KeyEvent) {
        match key_event.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                let is_connecting_error = matches!(
                    &self.state,
                    AppState::Error { previous_state, .. }
                    if matches!(previous_state.as_ref(), AppState::Selecting { .. })
                );

                if is_connecting_error {
                    // 接続失敗エラーはアプリ終了（シェルに戻る）
                    self.should_quit = true;
                } else {
                    // SQL実行エラー等はConnected状態に戻る
                    let previous_state = match std::mem::replace(
                        &mut self.state,
                        AppState::Selecting {
                            connections: Vec::new(),
                            selected_index: 0,
                        },
                    ) {
                        AppState::Error { previous_state, .. } => *previous_state,
                        other => other,
                    };
                    self.state = previous_state;
                }
            }
            _ => {}
        }
    }

    /// Shell入力エリアのキー入力処理
    pub(super) async fn handle_shell_input(&mut self, key_event: event::KeyEvent) -> Result<()> {
        match key_event.code {
            // Enter: トリム後が空でなければシェルコマンドを実行予約する
            KeyCode::Enter if !self.shell.text.trim().is_empty() => {
                self.execute_shell_command();
            }
            // Ctrl+C: 選択範囲をクリップボードにコピー（選択なしの場合は終了）
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some((byte_start, byte_end)) = self.shell_selection_byte_range() {
                    let selected_text = &self.shell.text[byte_start..byte_end];
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(selected_text.to_string());
                    }
                    // コピー後は選択解除
                    self.shell.selection_start = None;
                } else {
                    // 選択範囲がない場合は従来のCtrl+C動作（終了）
                    self.should_quit = true;
                }
            }
            // Ctrl+X: 選択範囲をカット
            KeyCode::Char('x') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some((byte_start, byte_end)) = self.shell_selection_byte_range() {
                    let selected_text = self.shell.text[byte_start..byte_end].to_string();
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(selected_text);
                    }
                    // 選択範囲を削除してカーソルを選択開始位置に移動
                    let cursor_start = self
                        .shell
                        .selection_start
                        .unwrap_or(self.shell.cursor_position)
                        .min(self.shell.cursor_position);
                    self.shell.text.replace_range(byte_start..byte_end, "");
                    self.shell.cursor_position = cursor_start;
                    self.shell.selection_start = None;
                }
            }
            // Ctrl+V: クリップボードからペースト
            KeyCode::Char('v') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                // 選択範囲があれば先に削除（上書きペースト）
                self.delete_shell_selection();
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        let sanitized = text.replace('\r', "");
                        let byte_pos = self.shell_char_to_byte(self.shell.cursor_position);
                        self.shell.text.insert_str(byte_pos, &sanitized);
                        self.shell.cursor_position += sanitized.chars().count();
                    }
                }
            }
            // Tab: Shell → Prompt にフォーカスを進める（Sql → Shell → Prompt → Sql の循環）
            // APIキー未設定時は Prompt をスキップして Shell → Sql に進める
            KeyCode::Tab => {
                let has_api_key = self
                    .settings
                    .anthropic_api_key
                    .as_ref()
                    .map(|k| !k.as_str().is_empty())
                    .unwrap_or(false);
                if has_api_key {
                    self.input_focus = InputFocus::Prompt;
                } else {
                    self.input_focus = InputFocus::Sql;
                }
            }
            // ↑: Shell履歴を遡る
            KeyCode::Up => {
                self.shell_history_prev();
            }
            // ↓: Shell履歴を進む
            KeyCode::Down => {
                self.shell_history_next();
            }
            // Ctrl+A: 全選択（行頭を選択開始点にしてカーソルを末尾へ）
            KeyCode::Char('a') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.shell.selection_start = Some(0);
                self.shell.cursor_position = self.shell.text.chars().count();
            }
            // Ctrl+E: カーソルを行末へ（選択解除）
            KeyCode::Char('e') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.shell.cursor_position = self.shell.text.chars().count();
                self.shell.selection_start = None;
            }
            // Ctrl+K: カーソル位置から行末までを削除（kill-line）
            // 選択範囲がある場合は選択範囲を削除する
            KeyCode::Char('k') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.shell.selection_start.is_some() {
                    self.delete_shell_selection();
                } else {
                    let byte_start = self.shell_char_to_byte(self.shell.cursor_position);
                    // 削除範囲を kill_buffer に保存する
                    self.shell.kill_buffer = self.shell.text[byte_start..].to_string();
                    self.shell.text.truncate(byte_start);
                    // cursor_position はそのまま（行末に到達した状態）
                }
            }
            // Ctrl+U: 行頭からカーソル位置までを削除（kill-whole-line 前半）
            // 選択範囲がある場合は選択範囲を削除する
            KeyCode::Char('u') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.shell.selection_start.is_some() {
                    self.delete_shell_selection();
                } else {
                    let byte_end = self.shell_char_to_byte(self.shell.cursor_position);
                    // 削除範囲を kill_buffer に保存する
                    self.shell.kill_buffer = self.shell.text[..byte_end].to_string();
                    self.shell.text.replace_range(..byte_end, "");
                    self.shell.cursor_position = 0;
                }
            }
            // Ctrl+W: カーソル直前の単語を削除（bash readline backward-kill-word）
            // Opt+Backspace と同じ動作（Linux環境での互換性）
            KeyCode::Char('w') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.shell.selection_start.is_some() {
                    self.delete_shell_selection();
                } else {
                    let new_pos = self.shell_word_left(self.shell.cursor_position);
                    if new_pos < self.shell.cursor_position {
                        let byte_start = self.shell_char_to_byte(new_pos);
                        let byte_end = self.shell_char_to_byte(self.shell.cursor_position);
                        self.shell.text.replace_range(byte_start..byte_end, "");
                        self.shell.cursor_position = new_pos;
                    }
                }
            }
            // Ctrl+Y: キルバッファからペースト（yank）
            KeyCode::Char('y') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.delete_shell_selection();
                if !self.shell.kill_buffer.is_empty() {
                    let byte_pos = self.shell_char_to_byte(self.shell.cursor_position);
                    let yanked = self.shell.kill_buffer.clone();
                    self.shell.text.insert_str(byte_pos, &yanked);
                    self.shell.cursor_position += yanked.chars().count();
                }
            }
            // Opt+Shift+← / Alt+Shift+←: 選択しながら1単語左へ移動
            // ALT+SHIFT 複合修飾子は ALT 単独・SHIFT 単独より前に配置してマッチ優先度を確保する
            KeyCode::Left
                if key_event.modifiers.contains(KeyModifiers::ALT)
                    && key_event.modifiers.contains(KeyModifiers::SHIFT) =>
            {
                if self.shell.selection_start.is_none() {
                    self.shell.selection_start = Some(self.shell.cursor_position);
                }
                self.shell.cursor_position = self.shell_word_left(self.shell.cursor_position);
            }
            // Opt+Shift+→ / Alt+Shift+→: 選択しながら1単語右へ移動
            KeyCode::Right
                if key_event.modifiers.contains(KeyModifiers::ALT)
                    && key_event.modifiers.contains(KeyModifiers::SHIFT) =>
            {
                if self.shell.selection_start.is_none() {
                    self.shell.selection_start = Some(self.shell.cursor_position);
                }
                self.shell.cursor_position = self.shell_word_right(self.shell.cursor_position);
            }
            // Shift+Left: 選択しながら左へ移動
            // 通常の Left より前に配置して Shift 修飾子付きが先にマッチするようにする
            KeyCode::Left
                if key_event.modifiers.contains(KeyModifiers::SHIFT)
                    && self.shell.cursor_position > 0 =>
            {
                if self.shell.selection_start.is_none() {
                    self.shell.selection_start = Some(self.shell.cursor_position);
                }
                self.shell.cursor_position -= 1;
            }
            // Shift+Right: 選択しながら右へ移動
            KeyCode::Right if key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                let char_count = self.shell.text.chars().count();
                if self.shell.cursor_position < char_count {
                    if self.shell.selection_start.is_none() {
                        self.shell.selection_start = Some(self.shell.cursor_position);
                    }
                    self.shell.cursor_position += 1;
                }
            }
            // Shift+Home: 選択しながら行頭へ
            KeyCode::Home if key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                if self.shell.selection_start.is_none() {
                    self.shell.selection_start = Some(self.shell.cursor_position);
                }
                self.shell.cursor_position = 0;
            }
            // Shift+End: 選択しながら行末へ
            KeyCode::End if key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                if self.shell.selection_start.is_none() {
                    self.shell.selection_start = Some(self.shell.cursor_position);
                }
                self.shell.cursor_position = self.shell.text.chars().count();
            }
            // Opt+← / Alt+←: 1単語左へ移動（選択解除）
            // iTerm2 / crossterm 標準パターン
            KeyCode::Left if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.shell.selection_start = None;
                self.shell.cursor_position = self.shell_word_left(self.shell.cursor_position);
            }
            // ESC b（Terminal.app の meta-key 送信形式）
            KeyCode::Char('b') if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.shell.selection_start = None;
                self.shell.cursor_position = self.shell_word_left(self.shell.cursor_position);
            }
            // Opt+→ / Alt+→: 1単語右へ移動（選択解除）
            // iTerm2 / crossterm 標準パターン
            KeyCode::Right if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.shell.selection_start = None;
                self.shell.cursor_position = self.shell_word_right(self.shell.cursor_position);
            }
            // ESC f（Terminal.app の meta-key 送信形式）
            KeyCode::Char('f') if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.shell.selection_start = None;
                self.shell.cursor_position = self.shell_word_right(self.shell.cursor_position);
            }
            // Opt+Backspace / Alt+Backspace: カーソル直前の単語を削除
            // 選択範囲がある場合は選択範囲を削除する
            KeyCode::Backspace if key_event.modifiers.contains(KeyModifiers::ALT) => {
                if self.shell.selection_start.is_some() {
                    self.delete_shell_selection();
                } else {
                    let new_pos = self.shell_word_left(self.shell.cursor_position);
                    if new_pos < self.shell.cursor_position {
                        let byte_start = self.shell_char_to_byte(new_pos);
                        let byte_end = self.shell_char_to_byte(self.shell.cursor_position);
                        self.shell.text.replace_range(byte_start..byte_end, "");
                        self.shell.cursor_position = new_pos;
                    }
                }
            }
            // Opt+Delete / Alt+Delete: カーソル直後の単語を削除
            // 選択範囲がある場合は選択範囲を削除する
            KeyCode::Delete if key_event.modifiers.contains(KeyModifiers::ALT) => {
                if self.shell.selection_start.is_some() {
                    self.delete_shell_selection();
                } else {
                    let new_pos = self.shell_word_right(self.shell.cursor_position);
                    if new_pos > self.shell.cursor_position {
                        let byte_start = self.shell_char_to_byte(self.shell.cursor_position);
                        let byte_end = self.shell_char_to_byte(new_pos);
                        self.shell.text.replace_range(byte_start..byte_end, "");
                        // cursor_position はそのまま（次の単語が繰り上がる）
                    }
                }
            }
            // Left: 1文字左へ移動（選択解除）
            KeyCode::Left => {
                self.shell.selection_start = None;
                if self.shell.cursor_position > 0 {
                    self.shell.cursor_position -= 1;
                }
            }
            // Right: 1文字右へ移動（選択解除）
            KeyCode::Right => {
                self.shell.selection_start = None;
                let char_count = self.shell.text.chars().count();
                if self.shell.cursor_position < char_count {
                    self.shell.cursor_position += 1;
                }
            }
            // Home: 行頭へ移動（選択解除）
            KeyCode::Home => {
                self.shell.selection_start = None;
                self.shell.cursor_position = 0;
            }
            // End: 行末へ移動（選択解除）
            KeyCode::End => {
                self.shell.selection_start = None;
                self.shell.cursor_position = self.shell.text.chars().count();
            }
            // Backspace: カーソル直前の1文字を削除（選択範囲があれば選択範囲を削除）
            KeyCode::Backspace => {
                if self.shell.selection_start.is_some() {
                    self.delete_shell_selection();
                } else if self.shell.cursor_position > 0 {
                    let byte_pos = self.shell_char_to_byte(self.shell.cursor_position - 1);
                    self.shell.text.remove(byte_pos);
                    self.shell.cursor_position -= 1;
                }
            }
            // Delete: カーソル直後の1文字を削除（選択範囲があれば選択範囲を削除）
            KeyCode::Delete => {
                if self.shell.selection_start.is_some() {
                    self.delete_shell_selection();
                } else {
                    let char_count = self.shell.text.chars().count();
                    if self.shell.cursor_position < char_count {
                        let byte_pos = self.shell_char_to_byte(self.shell.cursor_position);
                        self.shell.text.remove(byte_pos);
                    }
                }
            }
            // Esc: Shell入力をクリアする
            KeyCode::Esc => {
                self.shell.text.clear();
                self.shell.cursor_position = 0;
                self.shell.selection_start = None;
            }
            // Ctrl+J: 改行挿入（Shell入力でも複数行コマンドを入力できるようにする）
            KeyCode::Char('j') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.delete_shell_selection();
                let byte_pos = self.shell_char_to_byte(self.shell.cursor_position);
                self.shell.text.insert(byte_pos, '\n');
                self.shell.cursor_position += 1;
            }
            // 通常の文字入力（選択範囲があれば先に削除して上書き）
            KeyCode::Char(c) => {
                self.delete_shell_selection();
                let byte_pos = self.shell_char_to_byte(self.shell.cursor_position);
                self.shell.text.insert(byte_pos, c);
                self.shell.cursor_position += 1;
            }
            _ => {}
        }
        Ok(())
    }

    /// PROMPT 入力エリアのキー入力処理
    ///
    /// Enter でバックグラウンドタスクを spawn して Claude API エージェントを起動する。
    /// is_processing が true の間は Enter と文字入力を無視する。
    pub(super) async fn handle_prompt_input(&mut self, key_event: event::KeyEvent) -> Result<()> {
        match key_event.code {
            // Enter: API キーが設定されていてかつ未処理の場合のみエージェントを起動する
            KeyCode::Enter if !self.prompt.is_processing => {
                let prompt_text = self.prompt.text.trim().to_string();
                if prompt_text.is_empty() {
                    return Ok(());
                }

                // AppState::Connected からプールを取得する（他の状態では何もしない）
                let pool = match &self.state {
                    AppState::Connected { manager } => manager.pool().clone(),
                    _ => return Ok(()),
                };

                // APIキーは config.toml の settings.anthropic_api_key を優先する
                // （Config::load() 内で環境変数 ANTHROPIC_API_KEY へのフォールバック済み）
                // APIキーはログに出力しない
                let api_key = match self.settings.anthropic_api_key.as_ref() {
                    Some(key) => key.as_str().to_string(),
                    None => {
                        self.prompt.last_error =
                            Some("ANTHROPIC_API_KEY が設定されていません".to_string());
                        return Ok(());
                    }
                };

                // モデルは config.toml の settings.claude_model を優先する
                // （Config::load() 内で環境変数 CLAUDE_MODEL へのフォールバック済み）
                let model = self
                    .settings
                    .claude_model
                    .clone()
                    .unwrap_or_else(|| "claude-3-5-haiku-20241022".to_string());

                self.prompt.is_processing = true;
                self.prompt.last_error = None;

                // バックグラウンドタスクを spawn してエージェントを起動する
                // エージェント内部の SELECT は pool のコピーで実行されるため
                // TUI のクエリ結果テーブルには影響しない
                let handle = tokio::spawn(async move {
                    crate::claude::run_agent(&api_key, &model, &pool, &prompt_text).await
                });
                self.prompt_task = Some(handle);
            }
            // 処理中は Enter を無視する（上記の guard が false の場合）
            KeyCode::Enter => {}
            // Esc: SQL 入力エリアにフォーカスを戻す
            KeyCode::Esc => {
                self.input_focus = InputFocus::Sql;
            }
            // Tab: PROMPT → Sql にフォーカスを進める（循環）
            KeyCode::Tab => {
                self.input_focus = InputFocus::Sql;
            }
            // Shift+Tab: PROMPT → Shell に戻る（逆循環）
            KeyCode::BackTab => {
                self.input_focus = InputFocus::Shell;
            }
            // Ctrl+C: 終了
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            // 処理中の場合は文字入力を無視する
            KeyCode::Char(_) | KeyCode::Backspace if self.prompt.is_processing => {}
            // 通常の文字入力: プロンプトテキストに挿入する
            KeyCode::Char(c) => {
                let byte_pos = self.prompt_char_to_byte(self.prompt.cursor_position);
                self.prompt.text.insert(byte_pos, c);
                self.prompt.cursor_position += 1;
            }
            // Backspace: カーソル直前の文字を削除する
            KeyCode::Backspace if self.prompt.cursor_position > 0 => {
                let byte_pos = self.prompt_char_to_byte(self.prompt.cursor_position - 1);
                self.prompt.text.remove(byte_pos);
                self.prompt.cursor_position -= 1;
            }
            KeyCode::Backspace => {}
            // Left: カーソルを左に移動する
            KeyCode::Left if self.prompt.cursor_position > 0 => {
                self.prompt.cursor_position -= 1;
            }
            KeyCode::Left => {}
            // Right: カーソルを右に移動する
            KeyCode::Right => {
                let char_count = self.prompt.text.chars().count();
                if self.prompt.cursor_position < char_count {
                    self.prompt.cursor_position += 1;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// prompt.text の char 位置をバイト位置に変換する
    pub(super) fn prompt_char_to_byte(&self, char_pos: usize) -> usize {
        self.prompt
            .text
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.prompt.text.len())
    }

    /// Shell入力を実行予約する（pending 方式）
    ///
    /// terminal は run_loop が所有しているため、handle_shell_input から直接 TUI を停止できない。
    /// 代わりに pending_shell_command フィールドにセットして run_loop 側で実行させる。
    pub(super) fn execute_shell_command(&mut self) {
        let cmd = self.shell.text.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        self.add_to_shell_history(&cmd);
        self.shell.pending_command = Some(cmd);
        self.shell.text.clear();
        self.shell.cursor_position = 0;
    }

    /// shell_input の char 位置をバイト位置に変換する
    ///
    /// SQL 用の char_to_byte は query_input を参照するため、
    /// Shell 入力エリア専用の変換メソッドを用意する。
    pub(super) fn shell_char_to_byte(&self, char_pos: usize) -> usize {
        self.shell
            .text
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.shell.text.len())
    }

    /// shell_input でカーソルを1単語左に移動した時の位置を返す
    ///
    /// word_left は query_input を参照するため、Shell 用の実装を用意する。
    pub(super) fn shell_word_left(&self, from: usize) -> usize {
        if from == 0 {
            return 0;
        }
        let chars: Vec<char> = self.shell.text.chars().collect();
        let mut pos = from;

        // まず区切り文字をスキップ
        while pos > 0 && crate::completion::is_completion_separator(chars[pos - 1]) {
            pos -= 1;
        }
        // 次に単語文字を遡る
        while pos > 0 && !crate::completion::is_completion_separator(chars[pos - 1]) {
            pos -= 1;
        }
        pos
    }

    /// shell_input でカーソルを1単語右に移動した時の位置を返す
    ///
    /// word_right は query_input を参照するため、Shell 用の実装を用意する。
    pub(super) fn shell_word_right(&self, from: usize) -> usize {
        let chars: Vec<char> = self.shell.text.chars().collect();
        let len = chars.len();
        if from >= len {
            return len;
        }
        let mut pos = from;

        // まず区切り文字をスキップ
        while pos < len && crate::completion::is_completion_separator(chars[pos]) {
            pos += 1;
        }
        // 次に単語文字を進む
        while pos < len && !crate::completion::is_completion_separator(chars[pos]) {
            pos += 1;
        }
        pos
    }
}
