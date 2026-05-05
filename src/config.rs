use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum AccountSource {
    OAuth,
    WebCookie,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub source: AccountSource,
    /// For WebCookie: the sessionKey value.
    /// For OAuth: optional keychain service override (default: "Claude Code-credentials").
    #[serde(default)]
    pub credential: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// Polling interval in seconds (default 15 minutes)
    pub interval_secs: u64,

    /// Alert when session utilization (%) reaches this value.
    pub alert_pct_session: f64,

    /// Alert when weekly utilization (%) reaches this value.
    pub alert_pct_weekly: f64,

    /// Optional webhook URL for Slack/Discord notifications
    pub webhook_url: Option<String>,

    #[serde(default)]
    pub notification_icon: Option<String>,

    /// Output JSON file path
    pub output_path: PathBuf,

    /// List of tracked accounts
    #[serde(default)]
    pub accounts: Vec<Account>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval_secs: 900,
            alert_pct_session: 80.0,
            alert_pct_weekly: 80.0,
            webhook_url: None,
            notification_icon: None,
            output_path: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".claude")
                .join("usage-tracker.json"),
            accounts: vec![Account {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Default".to_string(),
                source: AccountSource::OAuth,
                credential: None,
            }],
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
    if !path.exists() {
        let cfg = Config::default();
        save(&cfg)?;
        return Ok(cfg);
    }

    let content = std::fs::read_to_string(&path)?;
    let raw: serde_json::Value = serde_json::from_str(&content)?;

    // Migration: old format had `session_cookie` field, no `accounts`
    if raw.get("accounts").is_none() {
        let mut cfg = Config::default();
        // Preserve old scalar fields
        if let Some(v) = raw.get("interval_secs").and_then(|v| v.as_u64()) {
            cfg.interval_secs = v;
        }
        if let Some(v) = raw.get("alert_pct_session").and_then(|v| v.as_f64()) {
            cfg.alert_pct_session = v;
        }
        if let Some(v) = raw.get("alert_pct_weekly").and_then(|v| v.as_f64()) {
            cfg.alert_pct_weekly = v;
        }
        if let Some(v) = raw.get("webhook_url").and_then(|v| v.as_str()) {
            cfg.webhook_url = Some(v.to_string());
        }
        if let Some(v) = raw.get("notification_icon").and_then(|v| v.as_str()) {
            cfg.notification_icon = Some(v.to_string());
        }
        if let Some(v) = raw.get("output_path").and_then(|v| v.as_str()) {
            cfg.output_path = PathBuf::from(v);
        }
        // Migrate session_cookie → account
        if let Some(cookie) = raw.get("session_cookie").and_then(|v| v.as_str()) {
            cfg.accounts = vec![Account {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Default".to_string(),
                source: AccountSource::WebCookie,
                credential: Some(cookie.to_string()),
            }];
        }
        save(&cfg)?;
        return Ok(cfg);
    }

    let cfg: Config = serde_json::from_value(raw)?;
    Ok(cfg)
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
