use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::sources::claude_code::{self, Usage};

const SESSION_WINDOW: Duration = Duration::hours(5);
const WEEKLY_WINDOW: Duration = Duration::days(7);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub captured_at: DateTime<Utc>,
    pub session: UsageSummary,
    pub weekly: UsageSummary,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub total_tokens: u64,
    /// When the oldest entry in this window will exit the rolling window
    pub reset_at: Option<DateTime<Utc>>,
}

fn to_summary(u: Usage, window: Duration) -> UsageSummary {
    let reset_at = u.oldest_entry_at.map(|t| t + window);
    UsageSummary {
        input_tokens: u.input_tokens,
        output_tokens: u.output_tokens,
        cache_read_tokens: u.cache_read_tokens,
        cache_creation_tokens: u.cache_creation_tokens,
        total_tokens: u.total_tokens(),
        reset_at,
    }
}

pub async fn snapshot() -> Result<Snapshot> {
    let session = claude_code::current_session_usage()?;
    let weekly = claude_code::weekly_usage()?;

    Ok(Snapshot {
        captured_at: Utc::now(),
        session: to_summary(session, SESSION_WINDOW),
        weekly: to_summary(weekly, WEEKLY_WINDOW),
    })
}
