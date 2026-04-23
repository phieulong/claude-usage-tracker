mod aggregator;
mod alert;
mod config;
mod menubar;
mod output;
mod sources;

use anyhow::Result;
use chrono::Utc;
use clap::{Parser, Subcommand};
use std::sync::{Arc, Mutex};
use tokio::time::{interval, Duration};

const ICON_BYTES: &[u8] = include_bytes!("../claude_ai_icon.jpg");
const ICON_FILENAME: &str = "claude_ai_icon.jpg";

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

            let data: Arc<Mutex<menubar::MenuBarData>> =
                Arc::new(Mutex::new(menubar::MenuBarData::default()));
            let data_bg = data.clone();

            let (notif_tx, notif_rx) = std::sync::mpsc::channel::<menubar::NotifRequest>();

            // Notify used by menubar to trigger an immediate daemon poll
            let refresh_notify = Arc::new(tokio::sync::Notify::new());
            let refresh_notify_bg = refresh_notify.clone();

            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
                rt.block_on(run_daemon(cfg, data_bg, notif_tx, refresh_notify_bg));
            });

            menubar::run(data, notif_rx, refresh_notify);
        }
    }

    Ok(())
}

fn format_reset(reset_at: Option<chrono::DateTime<Utc>>) -> String {
    match reset_at {
        None => String::new(),
        Some(t) => {
            let secs = (t - Utc::now()).num_seconds();
            if secs <= 0 {
                "now".to_string()
            } else {
                let h = secs / 3600;
                let m = (secs % 3600) / 60;
                if h > 0 { format!("{}h {:02}m", h, m) } else { format!("{}m", m) }
            }
        }
    }
}

async fn run_daemon(
    initial_cfg: config::Config,
    data: Arc<Mutex<menubar::MenuBarData>>,
    notif_tx: std::sync::mpsc::Sender<menubar::NotifRequest>,
    refresh_notify: Arc<tokio::sync::Notify>,
) {
    let mut ticker = interval(Duration::from_secs(initial_cfg.interval_secs));
    let mut alert_state = alert::AlertState::default();

    loop {
        // Wait for either the regular tick or an immediate refresh request
        tokio::select! {
            _ = ticker.tick() => {}
            _ = refresh_notify.notified() => {}
        }

        // Reload config from disk so changes made via the menu bar take effect
        let cfg = config::load().unwrap_or_else(|_| initial_cfg.clone());

        match aggregator::snapshot(&cfg).await {
            Ok(snap) => {
                {
                    let mut d = data.lock().unwrap();
                    d.session_pct = snap.session.utilization_pct;
                    d.weekly_pct = snap.weekly.utilization_pct;
                    d.session_over = snap.session.utilization_pct
                        .map(|p| p >= cfg.alert_pct_session)
                        .unwrap_or(false);
                    d.weekly_over = snap.weekly.utilization_pct
                        .map(|p| p >= cfg.alert_pct_weekly)
                        .unwrap_or(false);
                    d.session_reset_str = format_reset(snap.session.reset_at);
                    d.weekly_reset_str = format_reset(snap.weekly.reset_at);
                    d.source = snap.source.clone();
                    d.has_cookie = cfg.session_cookie.is_some();
                }

                output::print_snapshot(&snap, &cfg);
                if let Err(e) = output::write_json(&snap, &cfg) {
                    tracing::error!("Failed to write JSON: {e}");
                }
                if let Err(e) = alert::maybe_notify(&snap, &cfg, &mut alert_state, &notif_tx).await {
                    tracing::error!("Alert error: {e}");
                }
            }
            Err(e) => {
                tracing::error!("Snapshot failed: {e}");
            }
        }
    }
}
