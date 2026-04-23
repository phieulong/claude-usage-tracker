use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::sources::{claude_web, oauth_api};
use crate::sources::oauth_api::{OauthUsageResponse, RateLimitedError};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub captured_at: DateTime<Utc>,
    pub session: UsageSummary,
    pub weekly: UsageSummary,
    pub source: DataSource,
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

async fn try_oauth() -> Result<(UsageSummary, UsageSummary)> {
    loop {
        match oauth_api::fetch_usage().await {
            Ok(resp) => return Ok(map_response(resp)),
            Err(ref e) if e.downcast_ref::<RateLimitedError>().is_some() => {
                tracing::warn!("OAuth 429 — retrying in 3 minutes");
                tokio::time::sleep(tokio::time::Duration::from_secs(180)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

pub async fn snapshot(cfg: &Config) -> Result<Snapshot> {
    let now_utc = Utc::now();

    // If session_cookie is configured, prefer web API and fall back to OAuth on failure.
    if let Some(cookie) = &cfg.session_cookie {
        match claude_web::fetch_usage(cookie).await {
            Ok(resp) => {
                let (session, weekly) = map_response(resp);
                return Ok(Snapshot { captured_at: now_utc, session, weekly, source: DataSource::WebCookie });
            }
            Err(e) => {
                tracing::warn!("Web API failed ({e}), falling back to OAuth");
            }
        }
    }

    let (session, weekly) = try_oauth().await?;
    Ok(Snapshot { captured_at: now_utc, session, weekly, source: DataSource::OAuth })
}
