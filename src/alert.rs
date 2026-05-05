use anyhow::Result;
use std::collections::HashMap;

use crate::aggregator::{Snapshot, UsageSummary};
use crate::config::Config;
use crate::menubar::NotifRequest;

#[derive(Default)]
struct AccountAlertState {
    session_last_notified: Option<i64>,
    weekly_last_notified: Option<i64>,
}

#[derive(Default)]
pub struct AlertState {
    accounts: HashMap<String, AccountAlertState>,
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

fn should_alert(pct: f64, threshold: f64, last_alerted_step: &mut Option<i64>) -> bool {
    if pct < threshold {
        *last_alerted_step = None;
        return false;
    }
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
    let acc_state = state.accounts.entry(snap.account_id.clone()).or_default();
    let account_label = if snap.account_name.is_empty() {
        "Default".to_string()
    } else {
        snap.account_name.clone()
    };

    let session_pct = effective_pct(&snap.session);
    let weekly_pct = effective_pct(&snap.weekly);

    if should_alert(session_pct, cfg.alert_pct_session, &mut acc_state.session_last_notified) {
        let msg = format!("[{}] Session at {:.1}% used", account_label, session_pct);
        tracing::warn!("{msg}");
        let _ = notif_tx.send(NotifRequest {
            title: format!("Claude Usage — {} — Session", account_label),
            body: msg.clone(),
            icon: cfg.notification_icon.clone(),
        });
        if let Some(url) = &cfg.webhook_url {
            send_webhook(url, &account_label, "session", &msg).await?;
        }
    }

    if should_alert(weekly_pct, cfg.alert_pct_weekly, &mut acc_state.weekly_last_notified) {
        let msg = format!("[{}] Weekly at {:.1}% used", account_label, weekly_pct);
        tracing::warn!("{msg}");
        let _ = notif_tx.send(NotifRequest {
            title: format!("Claude Usage — {} — Weekly", account_label),
            body: msg.clone(),
            icon: cfg.notification_icon.clone(),
        });
        if let Some(url) = &cfg.webhook_url {
            send_webhook(url, &account_label, "weekly", &msg).await?;
        }
    }

    Ok(())
}

async fn send_webhook(url: &str, account: &str, period: &str, message: &str) -> Result<()> {
    let payload = serde_json::json!({
        "text": format!("[claude-usage-tracker] [{}] {} alert: {}", account, period, message)
    });
    let client = reqwest::Client::new();
    client.post(url).json(&payload).send().await?;
    Ok(())
}
