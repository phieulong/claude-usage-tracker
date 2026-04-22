use anyhow::{Context, Result};
use chrono::{NaiveTime, Weekday};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// Polling interval in seconds (default 15 minutes)
    pub interval_secs: u64,

    /// Alert when session utilization (%) reaches this value.
    /// Uses Claude's OAuth endpoint utilization when available; otherwise falls
    /// back to `total_tokens / session_token_alert`.
    pub alert_pct_session: f64,

    /// Alert when weekly utilization (%) reaches this value.
    pub alert_pct_weekly: f64,

    /// Fallback token threshold for session when OAuth data isn't available.
    pub session_token_alert: u64,

    /// Fallback token threshold for weekly when OAuth data isn't available.
    pub weekly_token_alert: u64,

    /// Day of week when the weekly limit resets (Claude shows this in Settings → Usage).
    /// Values: "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun".
    pub weekly_reset_weekday: String,

    /// Local time (HH:MM, 24-hour) when the weekly limit resets.
    pub weekly_reset_time: String,

    /// Optional webhook URL for Slack/Discord notifications
    pub webhook_url: Option<String>,

    /// Output JSON file path
    pub output_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval_secs: 900, // 15 minutes
            alert_pct_session: 80.0,
            alert_pct_weekly: 80.0,
            session_token_alert: 500_000,
            weekly_token_alert: 5_000_000,
            weekly_reset_weekday: "Fri".to_string(),
            weekly_reset_time: "11:00".to_string(),
            webhook_url: None,
            output_path: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".claude")
                .join("usage-tracker.json"),
        }
    }
}

impl Config {
    pub fn parsed_weekly_reset_weekday(&self) -> Result<Weekday> {
        Weekday::from_str(&self.weekly_reset_weekday)
            .with_context(|| format!("invalid weekday: {}", self.weekly_reset_weekday))
    }

    pub fn parsed_weekly_reset_time(&self) -> Result<NaiveTime> {
        NaiveTime::parse_from_str(&self.weekly_reset_time, "%H:%M")
            .with_context(|| format!("invalid time HH:MM: {}", self.weekly_reset_time))
    }
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("usage-tracker-config.json")
}

pub fn load() -> Result<Config> {
    let path = config_path();
    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        let cfg: Config = serde_json::from_str(&content)?;
        Ok(cfg)
    } else {
        let cfg = Config::default();
        save(&cfg)?;
        Ok(cfg)
    }
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(cfg)?;
    std::fs::write(path, content)?;
    Ok(())
}
