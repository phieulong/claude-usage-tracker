use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::process::Command;

/// Returned when the usage endpoint responds with HTTP 429 Too Many Requests.
#[derive(Debug)]
pub struct RateLimitedError;

impl fmt::Display for RateLimitedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OAuth usage endpoint returned 429 Too Many Requests")
    }
}

impl std::error::Error for RateLimitedError {}

const USAGE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const ANTHROPIC_BETA: &str = "oauth-2025-04-20";

/// Read the installed Claude Code version from
/// `~/Library/Application Support/Claude/claude-code/<version>/`.
/// Returns e.g. "2.1.111", or "unknown" if the directory cannot be read.
fn claude_version() -> String {
    let base = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Library/Application Support/Claude/claude-code");

    std::fs::read_dir(&base)
        .ok()
        .and_then(|entries| {
            let mut versions: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|name| name.chars().next().map_or(false, |c| c.is_ascii_digit()))
                .collect();
            // Sort semver-style so the highest version wins
            versions.sort_by(|a, b| {
                let parse = |s: &str| -> Vec<u64> {
                    s.split('.').filter_map(|p| p.parse().ok()).collect()
                };
                parse(b).cmp(&parse(a))
            });
            versions.into_iter().next()
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn user_agent() -> String {
    format!("claude-cli/{} (external, cli)", claude_version())
}

#[derive(Debug, Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: OAuth,
}

#[derive(Debug, Deserialize)]
struct OAuth {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "expiresAt", default)]
    expires_at: Option<i64>,
}

/// Raw response from /api/oauth/usage.
/// Fields are optional because some plans don't populate them.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OauthUsageResponse {
    pub five_hour: Option<RateLimit>,
    pub seven_day: Option<RateLimit>,
    #[serde(default)]
    pub seven_day_opus: Option<RateLimit>,
    #[serde(default)]
    pub seven_day_sonnet: Option<RateLimit>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimit {
    /// Utilization as a percentage (0.0 .. ~100.0). Sometimes > 100 for overage.
    pub utilization: f64,
    /// ISO 8601 timestamp when this window resets.
    pub resets_at: Option<DateTime<Utc>>,
}

/// Read the OAuth access token from macOS Keychain.
fn read_access_token() -> Result<String> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"])
        .output()
        .context("failed to spawn `security` to read Claude Code credentials")?;
    if !output.status.success() {
        return Err(anyhow!(
            "keychain lookup failed for '{}' (status {}). Is Claude Code logged in?",
            KEYCHAIN_SERVICE,
            output.status
        ));
    }
    let body = String::from_utf8(output.stdout).context("keychain output not UTF-8")?;
    let creds: Credentials = serde_json::from_str(body.trim())
        .context("failed to parse keychain JSON")?;
    if let Some(expires_ms) = creds.claude_ai_oauth.expires_at {
        let expires = expires_ms / 1000;
        let now = Utc::now().timestamp();
        if now >= expires {
            tracing::warn!(
                "OAuth token appears expired (expiresAt={}, now={}). May need to refresh via Claude Code.",
                expires,
                now
            );
        }
    }
    Ok(creds.claude_ai_oauth.access_token)
}

/// Fetch current usage from Claude's OAuth usage endpoint.
pub async fn fetch_usage() -> Result<OauthUsageResponse> {
    let token = read_access_token()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let resp = client
        .get(USAGE_ENDPOINT)
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", ANTHROPIC_BETA)
        .header("User-Agent", user_agent())
        .header("Content-Type", "application/json")
        .send()
        .await
        .context("usage endpoint request failed")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(anyhow::Error::new(RateLimitedError));
    }

    if !status.is_success() {
        return Err(anyhow!(
            "usage endpoint returned {}: {}",
            status,
            body.chars().take(500).collect::<String>()
        ));
    }

    let parsed: OauthUsageResponse = serde_json::from_str(&body)
        .with_context(|| format!("failed to parse usage response: {}", body.chars().take(500).collect::<String>()))?;
    Ok(parsed)
}
