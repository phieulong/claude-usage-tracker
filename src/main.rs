mod aggregator;
mod alert;
mod config;
mod output;
mod sources;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::time::{interval, Duration};

/// Embedded icon — compiled into the binary at build time.
const ICON_BYTES: &[u8] = include_bytes!("../claude_ai_icon.jpg");
const ICON_FILENAME: &str = "claude_ai_icon.jpg";

/// Extract the bundled icon to `~/.claude/claude_ai_icon.jpg` if not already there.
/// Returns the path to the icon file.
fn ensure_icon() -> std::path::PathBuf {
    let dest = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claude")
        .join(ICON_FILENAME);

    if !dest.exists() {
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&dest, ICON_BYTES) {
            tracing::warn!("Could not extract bundled icon to {}: {e}", dest.display());
        }
    }
    dest
}

#[derive(Parser)]
#[command(name = "claude-usage-tracker")]
#[command(about = "Track Claude Code token usage by session and weekly period")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Polling interval in seconds (overrides config)
    #[arg(long)]
    interval: Option<u64>,
}

#[derive(Subcommand)]
enum Command {
    /// Print current usage snapshot once and exit
    Status,
    /// Print current config and exit
    Config,
    /// Run the daemon loop (default behaviour)
    Daemon,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let mut cfg = config::load()?;

    // Ensure the bundled icon is extracted; use it as default if none configured.
    let icon_path = ensure_icon();
    if cfg.notification_icon.is_none() {
        cfg.notification_icon = Some(icon_path.to_string_lossy().into_owned());
    }

    if let Some(secs) = cli.interval {
        cfg.interval_secs = secs;
    }

    match cli.command.unwrap_or(Command::Daemon) {
        Command::Status => {
            let snap = aggregator::snapshot(&cfg).await?;
            output::print_snapshot(&snap, &cfg);
        }
        Command::Config => {
            println!("{}", serde_json::to_string_pretty(&cfg)?);
        }
        Command::Daemon => {
            tracing::info!(
                "Starting daemon — polling every {}s, output: {}",
                cfg.interval_secs,
                cfg.output_path.display()
            );
            run_daemon(cfg).await?;
        }
    }

    Ok(())
}

async fn run_daemon(cfg: config::Config) -> Result<()> {
    let mut ticker = interval(Duration::from_secs(cfg.interval_secs));
    let mut alert_state = alert::AlertState::default();

    loop {
        ticker.tick().await;
        match aggregator::snapshot(&cfg).await {
            Ok(snap) => {
                output::print_snapshot(&snap, &cfg);
                if let Err(e) = output::write_json(&snap, &cfg) {
                    tracing::error!("Failed to write JSON: {e}");
                }
                if let Err(e) = alert::maybe_notify(&snap, &cfg, &mut alert_state).await {
                    tracing::error!("Alert error: {e}");
                }
            }
            Err(e) => tracing::error!("Snapshot failed: {e}"),
        }
    }
}
