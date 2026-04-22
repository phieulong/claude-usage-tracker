use anyhow::Result;
use notify_rust::Notification;

use crate::aggregator::{Snapshot, UsageSummary};
use crate::config::Config;

pub fn notify_mac(title: &str, body: &str) -> Result<()> {
    Notification::new()
        .summary(title)
        .body(body)
        .sound_name("Sosumi")
        .show()?;
    Ok(())
}

fn effective_pct(summary: &UsageSummary) -> f64 {
    summary.utilization_pct.unwrap_or(0.0)
}

pub async fn maybe_notify(snap: &Snapshot, cfg: &Config) -> Result<()> {
    let session_pct = effective_pct(&snap.session);
    let weekly_pct = effective_pct(&snap.weekly);

    if session_pct >= cfg.alert_pct_session {
        let msg = format!("Session at {:.1}% used", session_pct);
        tracing::warn!("Session threshold hit: {msg}");
        if let Err(e) = notify_mac("Claude Usage Alert — Session", &msg) {
            tracing::error!("macOS notification failed: {e}");
        }
        if let Some(url) = &cfg.webhook_url {
            send_webhook(url, "session", &msg).await?;
        }
    }

    if weekly_pct >= cfg.alert_pct_weekly {
        let msg = format!("Weekly at {:.1}% used", weekly_pct);
        tracing::warn!("Weekly threshold hit: {msg}");
        if let Err(e) = notify_mac("Claude Usage Alert — Weekly", &msg) {
            tracing::error!("macOS notification failed: {e}");
        }
        if let Some(url) = &cfg.webhook_url {
            send_webhook(url, "weekly", &msg).await?;
        }
    }

    Ok(())
}

async fn send_webhook(url: &str, period: &str, message: &str) -> Result<()> {
    let payload = serde_json::json!({
        "text": format!("[claude-usage-tracker] {} alert: {}", period, message)
    });
    let client = reqwest::Client::new();
    client.post(url).json(&payload).send().await?;
    Ok(())
}
