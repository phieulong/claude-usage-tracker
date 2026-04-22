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

fn pct(tokens: u64, threshold: u64) -> f64 {
    if threshold == 0 {
        return 0.0;
    }
    tokens as f64 / threshold as f64 * 100.0
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

fn window_start_label(summary: &UsageSummary) -> String {
    match summary.window_start {
        None => "—".to_string(),
        Some(t) => {
            let local = t.with_timezone(&Local);
            local.format("%Y-%m-%d %H:%M %Z").to_string()
        }
    }
}

fn fmt_n(n: u64) -> String {
    // Thousands separator for readability
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

fn primary_pct_label(summary: &UsageSummary, fallback_threshold: u64) -> String {
    match summary.utilization_pct {
        Some(p) => format!("{:5.1}% (Claude)", p),
        None => format!(
            "{:5.1}% of {}",
            pct(summary.total_tokens, fallback_threshold),
            fmt_n(fallback_threshold)
        ),
    }
}

pub fn print_snapshot(snap: &Snapshot, cfg: &Config) {
    let captured_local = snap.captured_at.with_timezone(&Local);
    println!(
        "=== Claude Usage  {}  [source: {}]  ===",
        captured_local.format("%Y-%m-%d %H:%M:%S %Z"),
        snap.source,
    );

    let s = &snap.session;
    println!(
        "Session (5h)   {}   {}",
        primary_pct_label(s, cfg.session_token_alert),
        reset_label(s),
    );
    println!(
        "               local tokens: total={}  in={}  out={}  cache_create={}  cache_read={}  window_start={}",
        fmt_n(s.total_tokens),
        fmt_n(s.input_tokens),
        fmt_n(s.output_tokens),
        fmt_n(s.cache_creation_tokens),
        fmt_n(s.cache_read_tokens),
        window_start_label(s),
    );

    let w = &snap.weekly;
    println!(
        "Weekly  (7d)   {}   {}",
        primary_pct_label(w, cfg.weekly_token_alert),
        reset_label(w),
    );
    println!(
        "               local tokens: total={}  in={}  out={}  cache_create={}  cache_read={}  window_start={}",
        fmt_n(w.total_tokens),
        fmt_n(w.input_tokens),
        fmt_n(w.output_tokens),
        fmt_n(w.cache_creation_tokens),
        fmt_n(w.cache_read_tokens),
        window_start_label(w),
    );
}
