use anyhow::Result;

use crate::aggregator::{Snapshot, UsageSummary};
use crate::config::Config;
use crate::menubar::NotifRequest;

/// Tracks the last percentage level at which an alert was fired for each period.
/// Resets to None when utilization drops back below the configured threshold.
#[derive(Default)]
pub struct AlertState {
    pub session_last_notified: Option<i64>,
    pub weekly_last_notified: Option<i64>,
}

pub fn notify_mac(title: &str, body: &str, icon: Option<&str>) -> Result<()> {
    use mac_notification_sys::Notification;
    let mut n = Notification::new();
    n.title(title).message(body).sound("Sosumi").asynchronous(true);
    if let Some(path) = icon {
        n.app_icon(path);
    }
    n.send().map_err(|e| anyhow::anyhow!("notification error: {e:?}"))?;
    Ok(())
}

fn effective_pct(summary: &UsageSummary) -> f64 {
    summary.utilization_pct.unwrap_or(0.0)
}

/// Returns true and updates `last_alerted_step` if an alert should fire.
/// Steps are clean multiples of 5% above `threshold`:
///   threshold=70 → alerts at 70, 75, 80, 85, 90, 95, 100, ...
/// Resets when pct drops back below threshold.
fn should_alert(pct: f64, threshold: f64, last_alerted_step: &mut Option<i64>) -> bool {
    if pct < threshold {
        *last_alerted_step = None;
        return false;
    }
    // Which 5%-wide band above threshold are we in? (0 = [threshold, threshold+5), 1 = [threshold+5, threshold+10), …)
    let current_step = ((pct - threshold) / 5.0).floor() as i64;
    match *last_alerted_step {
        None => {
            *last_alerted_step = Some(current_step);
            true
        }
        Some(prev) if current_step > prev => {
            *last_alerted_step = Some(current_step);
            true
        }
        _ => false,
    }
}

pub async fn maybe_notify(
    snap: &Snapshot,
    cfg: &Config,
    state: &mut AlertState,
    notif_tx: &std::sync::mpsc::Sender<NotifRequest>,
) -> Result<()> {
    let session_pct = effective_pct(&snap.session);
    let weekly_pct = effective_pct(&snap.weekly);

    if should_alert(session_pct, cfg.alert_pct_session, &mut state.session_last_notified) {
        let msg = format!("Session at {:.1}% used", session_pct);
        tracing::warn!("Session threshold hit: {msg}");
        let _ = notif_tx.send(NotifRequest {
            title: "Claude Usage Alert — Session".to_string(),
            body: msg.clone(),
            icon: cfg.notification_icon.clone(),
        });
        if let Some(url) = &cfg.webhook_url {
            send_webhook(url, "session", &msg).await?;
        }
    }

    if should_alert(weekly_pct, cfg.alert_pct_weekly, &mut state.weekly_last_notified) {
        let msg = format!("Weekly at {:.1}% used", weekly_pct);
        tracing::warn!("Weekly threshold hit: {msg}");
        let _ = notif_tx.send(NotifRequest {
            title: "Claude Usage Alert — Weekly".to_string(),
            body: msg.clone(),
            icon: cfg.notification_icon.clone(),
        });
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
