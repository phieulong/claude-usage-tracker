use anyhow::Result;
use chrono::Utc;

use crate::aggregator::{Snapshot, UsageSummary};
use crate::config::Config;

pub fn write_json(snap: &Snapshot, cfg: &Config) -> Result<()> {
    let path = &cfg.output_path;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut history: Vec<Snapshot> = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    history.push(snap.clone());

    if history.len() > 1000 {
        history.drain(0..history.len() - 1000);
    }

    let json = serde_json::to_string_pretty(&history)?;
    std::fs::write(path, json)?;
    tracing::debug!("Wrote snapshot to {}", path.display());
    Ok(())
}

fn format_duration_hm(secs: i64) -> String {
    if secs <= 0 {
        return "now".to_string();
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{}h {:02}m", h, m)
    } else {
        format!("{}m", m)
    }
}

fn pct(tokens: u64, threshold: u64) -> f64 {
    if threshold == 0 {
        return 0.0;
    }
    tokens as f64 / threshold as f64 * 100.0
}

fn reset_label(summary: &UsageSummary) -> String {
    match summary.reset_at {
        None => "no activity".to_string(),
        Some(t) => {
            let secs = (t - Utc::now()).num_seconds();
            format!("resets in {}", format_duration_hm(secs))
        }
    }
}

pub fn print_snapshot(snap: &Snapshot, cfg: &Config) {
    println!(
        "=== Claude Usage  {}  ===",
        snap.captured_at.format("%Y-%m-%d %H:%M:%S UTC")
    );

    let s = &snap.session;
    println!(
        "Session (5h)  {:>8} tok  in={:>8}  out={:>8}  cache={:>8}  {:5.1}% of {:>7}  {}",
        s.total_tokens,
        s.input_tokens,
        s.output_tokens,
        s.cache_read_tokens,
        pct(s.total_tokens, cfg.session_token_alert),
        cfg.session_token_alert,
        reset_label(s),
    );

    let w = &snap.weekly;
    println!(
        "Weekly  (7d)  {:>8} tok  in={:>8}  out={:>8}  cache={:>8}  {:5.1}% of {:>7}  {}",
        w.total_tokens,
        w.input_tokens,
        w.output_tokens,
        w.cache_read_tokens,
        pct(w.total_tokens, cfg.weekly_token_alert),
        cfg.weekly_token_alert,
        reset_label(w),
    );
}
