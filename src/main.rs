use anyhow::Result;
use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use poshgre::config::Config;
use poshgre::i18n::{self, Lang};
use poshgre::tui;

#[derive(Parser, Debug)]
#[command(name = "posh", version)]
#[command(about = "A TUI PostgreSQL client with bastion (jump host) support", long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<String>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Enable readonly mode (prevents write operations)
    #[arg(short = 'r', long)]
    readonly: bool,

    /// Display language (en/ja)
    #[arg(long)]
    lang: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // CLI引数パース
    let args = Args::parse();

    // ログ初期化
    init_logging(args.verbose)?;

    tracing::info!("Starting posh");

    // 設定ファイルパスの解決
    // 明示指定 → ~/.config/poshgre/config.toml → ./config.toml の優先順位で探索する
    let config_path = resolve_config_path(args.config.as_deref());
    tracing::debug!("Loading config from: {}", config_path);

    // 設定読み込み
    // 注意: Config::load() 自体のエラーメッセージはset_lang前なのでデフォルト(en)で表示される
    let config = Config::load(&config_path)?;

    // 言語解決: CLIオプション > config.toml [settings] language > デフォルト (en)
    let lang = if let Some(ref lang_str) = args.lang {
        lang_str
            .parse::<Lang>()
            .map_err(|e| anyhow::anyhow!("Invalid --lang value: {}", e))?
    } else if let Some(ref lang_str) = config.settings.language {
        lang_str
            .parse::<Lang>()
            .map_err(|e| anyhow::anyhow!("Invalid language in config.toml: {}", e))?
    } else {
        Lang::En
    };
    i18n::set_lang(lang);

    // シャットダウンフラグ作成
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_clone = Arc::clone(&shutdown_flag);

    // SIGINT/SIGTERM ハンドラ設定
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to listen for ctrl-c signal: {}", e);
            return;
        }
        tracing::info!("Received shutdown signal (SIGINT/SIGTERM)");
        shutdown_flag_clone.store(true, Ordering::Relaxed);
    });

    // TUIアプリケーション起動
    let mut app = tui::App::new(config, shutdown_flag, args.readonly);
    app.run().await?;

    tracing::info!("posh terminated successfully");
    Ok(())
}

/// 設定ファイルパスを解決する
///
/// 優先順位:
/// 1. --config で明示指定 → そのパス
/// 2. ~/.config/poshgre/config.toml が存在する → そのパス
/// 3. ./config.toml が存在する → そのパス
/// 4. いずれも見つからない → ~/.config/poshgre/config.toml を返す（Config::load 側でエラー）
fn resolve_config_path(explicit: Option<&str>) -> String {
    if let Some(path) = explicit {
        return shellexpand::tilde(path).to_string();
    }

    let xdg_path = shellexpand::tilde("~/.config/poshgre/config.toml").to_string();
    if std::path::Path::new(&xdg_path).exists() {
        return xdg_path;
    }

    let local_path = "config.toml";
    if std::path::Path::new(local_path).exists() {
        return local_path.to_string();
    }

    // どちらも存在しない場合は標準パスを返し、Config::load にエラー処理を委ねる
    xdg_path
}

fn init_logging(verbose: bool) -> Result<()> {
    let filter = if verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    // ログをファイルに出力（TUI表示と干渉しないようにする）
    let log_dir = std::env::temp_dir().join("posh");
    std::fs::create_dir_all(&log_dir)?;
    let log_file = std::fs::File::create(log_dir.join("posh.log"))?;

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(log_file).with_ansi(false))
        .init();

    Ok(())
}
