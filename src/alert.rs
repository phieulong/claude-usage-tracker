use anyhow::Result;
use notify_rust::Notification;

use crate::aggregator::Snapshot;
use crate::config::Config;

pub fn notify_mac(title: &str, body: &str) -> Result<()> {
    Notification::new()
        .summary(title)
        .body(body)
        .sound_name("Sosumi")
        .show()?;
    Ok(())
}

pub async fn maybe_notify(snap: &Snapshot, cfg: &Config) -> Result<()> {
    let session_total = snap.session.total_tokens;
    let weekly_total = snap.weekly.total_tokens;

    if session_total >= cfg.session_token_alert {
        let msg = format!(
            "Session: {} tokens (in: {}, out: {})",
            session_total, snap.session.input_tokens, snap.session.output_tokens
        );
        tracing::warn!("Session threshold hit: {msg}");
        if let Err(e) = notify_mac("Claude Usage Alert — Session", &msg) {
            tracing::error!("macOS notification failed: {e}");
        }
        if let Some(url) = &cfg.webhook_url {
            send_webhook(url, "session", &msg).await?;
        }
    }

    if weekly_total >= cfg.weekly_token_alert {
        let msg = format!(
            "Weekly: {} tokens (in: {}, out: {})",
            weekly_total, snap.weekly.input_tokens, snap.weekly.output_tokens
        );
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
