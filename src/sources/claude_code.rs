use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug, Deserialize)]
struct RawEntry {
    timestamp: DateTime<Utc>,
    #[serde(default)]
    message: Option<RawMessage>,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    usage: Option<RawUsage>,
}

#[derive(Debug, Deserialize, Default)]
struct RawUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub timestamp: DateTime<Utc>,
    pub usage: Option<EntryUsage>,
}

#[derive(Debug, Clone, Default)]
pub struct EntryUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    /// Start of the active window (session start for session, rolling cutoff for weekly).
    /// `reset_at = window_start + window_duration`
    pub window_start: Option<DateTime<Utc>>,
}

impl Usage {
    /// Tokens counted against Claude's quota.
    /// Claude's session/weekly limits count input + output + cache_creation.
    /// Cache reads are extremely cheap and don't count toward quotas.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_creation_tokens
    }
}

pub fn claude_projects_dir() -> PathBuf {
    dirs::home_dir()
        .expect("cannot find home dir")
        .join(".claude")
        .join("projects")
}

/// Load all entries with timestamp >= `since`, sorted ascending.
pub fn load_entries_since(since: DateTime<Utc>) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let projects_dir = claude_projects_dir();

    if !projects_dir.exists() {
        tracing::warn!("Claude projects dir not found: {}", projects_dir.display());
        return Ok(entries);
    }

    for file in WalkDir::new(&projects_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |x| x == "jsonl"))
    {
        let content = match std::fs::read_to_string(file.path()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to read {}: {e}", file.path().display());
                continue;
            }
        };

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let raw: RawEntry = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if raw.timestamp < since {
                continue;
            }
            let usage = raw.message.and_then(|m| m.usage).map(|u| EntryUsage {
                input_tokens: u.input_tokens.unwrap_or(0),
                output_tokens: u.output_tokens.unwrap_or(0),
                cache_read_tokens: u.cache_read_input_tokens.unwrap_or(0),
                cache_creation_tokens: u.cache_creation_input_tokens.unwrap_or(0),
            });
            entries.push(Entry {
                timestamp: raw.timestamp,
                usage,
            });
        }
    }

    entries.sort_by_key(|e| e.timestamp);
    Ok(entries)
}

/// Detect the start of the currently-active session via gap-based detection.
///
/// Claude's session model: the first message starts a `window` (e.g. 5h) timer.
/// Any message sent after the timer expires begins a new session.
/// Returns `None` if the most recent session has already ended (no active session).
pub fn detect_session_start(
    entries: &[Entry],
    window: Duration,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let mut iter = entries.iter();
    let first = iter.next()?;
    let mut session_start = first.timestamp;
    for e in iter {
        if e.timestamp - session_start > window {
            session_start = e.timestamp;
        }
    }
    if now - session_start > window {
        None
    } else {
        Some(session_start)
    }
}

fn sum_usage_from(entries: &[Entry], from: Option<DateTime<Utc>>) -> Usage {
    let mut total = Usage::default();
    let Some(from) = from else {
        return total;
    };
    total.window_start = Some(from);
    for e in entries.iter().filter(|e| e.timestamp >= from) {
        if let Some(u) = &e.usage {
            total.input_tokens += u.input_tokens;
            total.output_tokens += u.output_tokens;
            total.cache_read_tokens += u.cache_read_tokens;
            total.cache_creation_tokens += u.cache_creation_tokens;
        }
    }
    total
}

/// Usage in the currently-active 5-hour session (gap-detected).
pub fn current_session_usage() -> Result<Usage> {
    let now = Utc::now();
    // Look back 24h to safely find session start even if session is ~5h old.
    let lookback = now - Duration::hours(24);
    let entries = load_entries_since(lookback)?;
    let session_start = detect_session_start(&entries, Duration::hours(5), now);
    Ok(sum_usage_from(&entries, session_start))
}

/// Sum usage from a fixed start time onward (used for weekly window with day-of-week reset).
pub fn usage_since(since: DateTime<Utc>) -> Result<Usage> {
    let entries = load_entries_since(since)?;
    Ok(sum_usage_from(&entries, Some(since)))
}
