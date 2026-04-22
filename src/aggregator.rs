use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::sources::oauth_api::{self, RateLimitedError};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub captured_at: DateTime<Utc>,
    pub session: UsageSummary,
    pub weekly: UsageSummary,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UsageSummary {
    /// Utilization percentage (0.0..~100+) as reported by Claude
    pub utilization_pct: Option<f64>,
    /// When this window resets
    pub reset_at: Option<DateTime<Utc>>,
}

pub async fn snapshot(_cfg: &Config) -> Result<Snapshot> {
    let now_utc = Utc::now();

    // OAuth API is the sole source. On 429 retry after 10 min; other errors propagate.
    let (session, weekly) = loop {
        match oauth_api::fetch_usage().await {
            Ok(resp) => {
                let session = UsageSummary {
                    utilization_pct: resp.five_hour.as_ref().map(|rl| rl.utilization),
                    reset_at: resp.five_hour.and_then(|rl| rl.resets_at),
                };
                let weekly = UsageSummary {
                    utilization_pct: resp.seven_day.as_ref().map(|rl| rl.utilization),
                    reset_at: resp.seven_day.and_then(|rl| rl.resets_at),
                };
                break (session, weekly);
            }
            Err(ref e) if e.downcast_ref::<RateLimitedError>().is_some() => {
                tracing::warn!(
                    "OAuth usage fetch returned 429 Too Many Requests. \
                     Retrying in 10 minutes..."
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(600)).await;
            }
            Err(e) => {
                return Err(e);
            }
        }
    };

    Ok(Snapshot { captured_at: now_utc, session, weekly })
}
