use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// Polling interval in seconds (default 15 minutes)
    pub interval_secs: u64,

    /// Alert when session tokens exceed this threshold
    pub session_token_alert: u64,

    /// Alert when weekly tokens exceed this threshold
    pub weekly_token_alert: u64,

    /// Optional webhook URL for Slack/Discord notifications
    pub webhook_url: Option<String>,

    /// Output JSON file path
    pub output_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval_secs: 1, // 15 minutes
            session_token_alert: 50_000,
            weekly_token_alert: 500_000,
            webhook_url: None,
            output_path: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".claude")
                .join("usage-tracker.json"),
        }
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
