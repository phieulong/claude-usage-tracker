use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Utc, Weekday};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::sources::claude_code::{self, Usage};
use crate::sources::oauth_api;

const SESSION_WINDOW: Duration = Duration::hours(5);
pub const WEEKLY_WINDOW: Duration = Duration::days(7);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub captured_at: DateTime<Utc>,
    /// Source used for percentage/reset ("oauth" or "local")
    pub source: String,
    pub session: UsageSummary,
    pub weekly: UsageSummary,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UsageSummary {
    /// Utilization percentage (0.0..~100+) as reported by Claude (if from oauth source)
    pub utilization_pct: Option<f64>,
    /// When this window resets
    pub reset_at: Option<DateTime<Utc>>,
    /// Start of current window
    pub window_start: Option<DateTime<Utc>>,
    /// Local token breakdown (from parsing ~/.claude/projects/*.jsonl)
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    /// total_tokens = input + output + cache_creation (approximate quota formula)
    pub total_tokens: u64,
}

fn fill_local_tokens(summary: &mut UsageSummary, u: &Usage) {
    summary.input_tokens = u.input_tokens;
    summary.output_tokens = u.output_tokens;
    summary.cache_read_tokens = u.cache_read_tokens;
    summary.cache_creation_tokens = u.cache_creation_tokens;
    summary.total_tokens = u.total_tokens();
}

/// Compute the next local-time occurrence of (weekday, time) strictly after `now`.
pub fn next_weekly_reset(
    now: DateTime<Local>,
    reset_weekday: Weekday,
    reset_time: NaiveTime,
) -> DateTime<Local> {
    let mut date = now.date_naive();
    for _ in 0..14 {
        if date.weekday() == reset_weekday {
            if let chrono::LocalResult::Single(dt) =
                Local.from_local_datetime(&date.and_time(reset_time))
            {
                if dt > now {
                    return dt;
                }
            }
        }
        date = date.succ_opt().expect("date overflow");
    }
    now + Duration::days(7)
}

pub async fn snapshot(cfg: &Config) -> Result<Snapshot> {
    let now_utc = Utc::now();

    // Collect local token breakdowns for context
    let session_local = claude_code::current_session_usage()
        .unwrap_or_else(|_| Usage::default());
    let weekly_local_window_start = {
        let now_local = now_utc.with_timezone(&Local);
        let reset_weekday = cfg
            .parsed_weekly_reset_weekday()
            .unwrap_or(Weekday::Fri);
        let reset_time = cfg
            .parsed_weekly_reset_time()
            .unwrap_or_else(|_| NaiveTime::from_hms_opt(11, 0, 0).unwrap());
        let next_reset_local = next_weekly_reset(now_local, reset_weekday, reset_time);
        next_reset_local.with_timezone(&Utc) - WEEKLY_WINDOW
    };
    let weekly_local = claude_code::usage_since(weekly_local_window_start)
        .unwrap_or_else(|_| Usage::default());

    // Try OAuth API as the authoritative source for utilization + reset
    let (session, weekly, source) = match oauth_api::fetch_usage().await {
        Ok(resp) => {
            let mut session = UsageSummary::default();
            if let Some(rl) = resp.five_hour {
                session.utilization_pct = Some(rl.utilization);
                session.reset_at = rl.resets_at;
                session.window_start = rl.resets_at.map(|t| t - SESSION_WINDOW);
            }
            fill_local_tokens(&mut session, &session_local);

            let mut weekly = UsageSummary::default();
            if let Some(rl) = resp.seven_day {
                weekly.utilization_pct = Some(rl.utilization);
                weekly.reset_at = rl.resets_at;
                weekly.window_start = rl.resets_at.map(|t| t - WEEKLY_WINDOW);
            }
            fill_local_tokens(&mut weekly, &weekly_local);

            (session, weekly, "oauth".to_string())
        }
        Err(e) => {
            tracing::warn!("OAuth usage fetch failed, falling back to local JSONL: {e}");
            let mut session = UsageSummary::default();
            session.window_start = session_local.window_start;
            session.reset_at = session_local.window_start.map(|t| t + SESSION_WINDOW);
            fill_local_tokens(&mut session, &session_local);

            let mut weekly = UsageSummary::default();
            weekly.window_start = Some(weekly_local_window_start);
            weekly.reset_at = Some(weekly_local_window_start + WEEKLY_WINDOW);
            fill_local_tokens(&mut weekly, &weekly_local);

            (session, weekly, "local".to_string())
        }
    };

    Ok(Snapshot {
        captured_at: now_utc,
        source,
        session,
        weekly,
    })
}
