use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{Account, AccountSource, Config};
use crate::sources::{claude_web, oauth_api};
use crate::sources::oauth_api::{OauthUsageResponse, RateLimitedError};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub captured_at: DateTime<Utc>,
    pub session: UsageSummary,
    pub weekly: UsageSummary,
    pub source: DataSource,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub account_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UsageSummary {
    pub utilization_pct: Option<f64>,
    pub reset_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub enum DataSource {
    #[default]
    OAuth,
    WebCookie,
}

fn map_response(resp: OauthUsageResponse) -> (UsageSummary, UsageSummary) {
    let session = UsageSummary {
        utilization_pct: resp.five_hour.as_ref().map(|rl| rl.utilization),
        reset_at: resp.five_hour.and_then(|rl| rl.resets_at),
    };
    let weekly = UsageSummary {
        utilization_pct: resp.seven_day.as_ref().map(|rl| rl.utilization),
        reset_at: resp.seven_day.and_then(|rl| rl.resets_at),
    };
    (session, weekly)
}

async fn try_oauth(keychain_service: Option<&str>) -> Result<(UsageSummary, UsageSummary)> {
    loop {
        match oauth_api::fetch_usage(keychain_service).await {
            Ok(resp) => return Ok(map_response(resp)),
            Err(ref e) if e.downcast_ref::<RateLimitedError>().is_some() => {
                tracing::warn!("OAuth 429 — retrying in 3 minutes");
                tokio::time::sleep(tokio::time::Duration::from_secs(180)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Fetch usage for a single account.
async fn snapshot_one(account: &Account) -> Result<Snapshot> {
    let now = Utc::now();
    let (session, weekly, source) = match account.source {
        AccountSource::WebCookie => {
            let cookie = account.credential.as_deref()
                .ok_or_else(|| anyhow!("Account '{}' has no session cookie", account.name))?;
            let resp = claude_web::fetch_usage(cookie).await?;
            let (s, w) = map_response(resp);
            (s, w, DataSource::WebCookie)
        }
        AccountSource::OAuth => {
            let service = account.credential.as_deref();
            let (s, w) = try_oauth(service).await?;
            (s, w, DataSource::OAuth)
        }
    };
    Ok(Snapshot {
        captured_at: now,
        session,
        weekly,
        source,
        account_id: account.id.clone(),
        account_name: account.name.clone(),
    })
}

/// Poll all accounts concurrently. Returns results keyed by account id.
pub async fn snapshot_all(cfg: &Config) -> Vec<(Account, Result<Snapshot>)> {
    let futs: Vec<_> = cfg.accounts.iter().map(|acc| {
        let acc = acc.clone();
        async move {
            let result = snapshot_one(&acc).await;
            (acc, result)
        }
    }).collect();
    futures::future::join_all(futs).await
}

/// Single-account snapshot (for `status` subcommand backward compat)
pub async fn snapshot(cfg: &Config) -> Result<Snapshot> {
    if let Some(acc) = cfg.accounts.first() {
        snapshot_one(acc).await
    } else {
        Err(anyhow!("No accounts configured"))
    }
}
