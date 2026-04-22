use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug, Deserialize)]
struct SessionEntry {
    timestamp: DateTime<Utc>,
    #[serde(rename = "type")]
    _entry_type: String,
    message: Option<Message>,
}

#[derive(Debug, Deserialize)]
struct Message {
    usage: Option<RawUsage>,
}

#[derive(Debug, Deserialize, Default)]
struct RawUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

#[derive(Debug, Default, Clone)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    /// Oldest entry timestamp within the queried window (for reset countdown)
    pub oldest_entry_at: Option<DateTime<Utc>>,
}

impl Usage {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

pub fn claude_projects_dir() -> PathBuf {
    dirs::home_dir()
        .expect("cannot find home dir")
        .join(".claude")
        .join("projects")
}

pub fn collect_usage_since(since: DateTime<Utc>) -> Result<Usage> {
    let mut total = Usage::default();
    let projects_dir = claude_projects_dir();

    if !projects_dir.exists() {
        tracing::warn!("Claude projects dir not found: {}", projects_dir.display());
        return Ok(total);
    }

    for entry in WalkDir::new(&projects_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |x| x == "jsonl"))
    {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to read {}: {e}", entry.path().display());
                continue;
            }
        };

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(se) = serde_json::from_str::<SessionEntry>(line) {
                if se.timestamp < since {
                    continue;
                }
                // Track oldest entry to compute reset countdown
                total.oldest_entry_at = Some(match total.oldest_entry_at {
                    Some(prev) => prev.min(se.timestamp),
                    None => se.timestamp,
                });
                if let Some(u) = se.message.and_then(|m| m.usage) {
                    total.input_tokens += u.input_tokens.unwrap_or(0);
                    total.output_tokens += u.output_tokens.unwrap_or(0);
                    total.cache_read_tokens += u.cache_read_input_tokens.unwrap_or(0);
                    total.cache_creation_tokens += u.cache_creation_input_tokens.unwrap_or(0);
                }
            }
        }
    }

    Ok(total)
}

pub fn current_session_usage() -> Result<Usage> {
    let since = Utc::now() - Duration::hours(5);
    collect_usage_since(since)
}

pub fn weekly_usage() -> Result<Usage> {
    let since = Utc::now() - Duration::days(7);
    collect_usage_since(since)
}
