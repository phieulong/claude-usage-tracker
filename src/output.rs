use anyhow::Result;
use chrono::{Local, Utc};

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

fn reset_label(summary: &UsageSummary) -> String {
    match summary.reset_at {
        None => "no active window".to_string(),
        Some(t) => {
            let secs = (t - Utc::now()).num_seconds();
            let local = t.with_timezone(&Local);
            format!(
                "resets in {} ({})",
                format_duration_hm(secs),
                local.format("%a %H:%M %Z")
            )
        }
    }
}

fn primary_pct_label(summary: &UsageSummary) -> String {
    match summary.utilization_pct {
        Some(p) => format!("{:5.1}%", p),
        None => "  n/a%".to_string(),
    }
}

pub fn print_snapshot(snap: &Snapshot, _cfg: &Config) {
    let captured_local = snap.captured_at.with_timezone(&Local);
    println!(
        "=== Claude Usage  {}  ===",
        captured_local.format("%Y-%m-%d %H:%M:%S %Z"),
    );
    println!(
        "Session (5h)   {}   {}",
        primary_pct_label(&snap.session),
        reset_label(&snap.session),
    );
    println!(
        "Weekly  (7d)   {}   {}",
        primary_pct_label(&snap.weekly),
        reset_label(&snap.weekly),
    );
}
