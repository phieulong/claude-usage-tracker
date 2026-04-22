//! Fetch Claude usage via the claude.ai web API using a `sessionKey` cookie.
//!
//! This uses undocumented endpoints. The request flow:
//!   1. GET https://claude.ai/api/organizations  → extract the first org uuid
//!   2. Try a list of candidate usage endpoints under that org until one succeeds
//!   3. Best-effort map the JSON response to the same shape as the OAuth source.
//!
//! If every candidate endpoint returns non-200 or an unparseable payload, this
//! module logs the raw bodies at `error!` level and surfaces a descriptive error
//! so the user can share the log and we can adjust the endpoint list.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::sources::oauth_api::{OauthUsageResponse, RateLimit};

const BASE: &str = "https://claude.ai";
const USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

#[derive(Debug, Deserialize)]
struct Org {
    uuid: String,
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(USER_AGENT)
        .build()
        .context("build reqwest client")
}

fn cookie_header(session_key: &str) -> String {
    let trimmed = session_key.trim();
    if trimmed.starts_with("sessionKey=") {
        trimmed.to_string()
    } else {
        format!("sessionKey={}", trimmed)
    }
}

async fn fetch_org_uuid(c: &reqwest::Client, cookie: &str) -> Result<String> {
    let url = format!("{BASE}/api/organizations");
    let resp = c
        .get(&url)
        .header("Cookie", cookie)
        .header("Accept", "application/json")
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!(
            "{url} returned {status}: {}",
            body.chars().take(300).collect::<String>()
        ));
    }
    let orgs: Vec<Org> = serde_json::from_str(&body)
        .with_context(|| format!("parse /api/organizations: {}", body.chars().take(300).collect::<String>()))?;
    orgs.into_iter()
        .next()
        .map(|o| o.uuid)
        .ok_or_else(|| anyhow!("no organizations returned for session cookie"))
}

/// Try a sequence of candidate usage endpoints. Return the first JSON body that
/// parses into a non-empty `OauthUsageResponse`.
async fn fetch_usage_body(c: &reqwest::Client, cookie: &str, org_uuid: &str) -> Result<Value> {
    let candidates = [
        format!("{BASE}/api/organizations/{org_uuid}/usage"),
        format!("{BASE}/api/organizations/{org_uuid}/rate_limits"),
        format!("{BASE}/api/organizations/{org_uuid}/usage_limits"),
        format!("{BASE}/api/bootstrap/{org_uuid}/usage"),
    ];

    let mut last_err: Option<String> = None;
    for url in &candidates {
        let resp = match c
            .get(url)
            .header("Cookie", cookie)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_err = Some(format!("{url}: {e}"));
                continue;
            }
        };
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::debug!("claude.ai {url} → {status}");
        if !status.is_success() {
            last_err = Some(format!(
                "{url} → {status}: {}",
                body.chars().take(200).collect::<String>()
            ));
            continue;
        }
        match serde_json::from_str::<Value>(&body) {
            Ok(v) => {
                tracing::info!("claude.ai usage endpoint matched: {url}");
                return Ok(v);
            }
            Err(e) => {
                last_err = Some(format!("{url} parse error: {e}"));
            }
        }
    }

    Err(anyhow!(
        "no claude.ai usage endpoint responded with valid JSON. \
         Last error: {}. Open DevTools → Network on claude.ai/settings/billing, \
         find the XHR that returns usage data, and share its URL.",
        last_err.unwrap_or_else(|| "<none>".into())
    ))
}

/// Best-effort mapping from arbitrary claude.ai JSON → `OauthUsageResponse`.
/// Walks the tree looking for `five_hour`/`seven_day` or similarly-named keys.
fn map_response(v: &Value) -> OauthUsageResponse {
    let extract = |keys: &[&str]| -> Option<RateLimit> {
        for k in keys {
            if let Some(obj) = find_key(v, k) {
                if let Some(rl) = parse_rate_limit(obj) {
                    return Some(rl);
                }
            }
        }
        None
    };

    OauthUsageResponse {
        five_hour: extract(&["five_hour", "fiveHour", "5h", "session"]),
        seven_day: extract(&["seven_day", "sevenDay", "7d", "weekly"]),
        seven_day_opus: extract(&["seven_day_opus", "sevenDayOpus"]),
        seven_day_sonnet: extract(&["seven_day_sonnet", "sevenDaySonnet"]),
    }
}

fn find_key<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Object(m) => {
            if let Some(x) = m.get(key) {
                return Some(x);
            }
            for (_, val) in m {
                if let Some(x) = find_key(val, key) {
                    return Some(x);
                }
            }
            None
        }
        Value::Array(a) => {
            for item in a {
                if let Some(x) = find_key(item, key) {
                    return Some(x);
                }
            }
            None
        }
        _ => None,
    }
}

fn parse_rate_limit(v: &Value) -> Option<RateLimit> {
    let util = ["utilization", "utilization_pct", "percent", "pct"]
        .iter()
        .find_map(|k| v.get(k).and_then(|x| x.as_f64()))?;
    let resets = ["resets_at", "reset_at", "resetsAt", "resetAt"]
        .iter()
        .find_map(|k| {
            v.get(k)
                .and_then(|x| x.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
        });
    Some(RateLimit { utilization: util, resets_at: resets })
}

pub async fn fetch_usage(session_cookie: &str) -> Result<OauthUsageResponse> {
    let c = client()?;
    let cookie = cookie_header(session_cookie);
    let org_uuid = fetch_org_uuid(&c, &cookie).await?;
    let body = fetch_usage_body(&c, &cookie, &org_uuid).await?;
    let mapped = map_response(&body);
    if mapped.five_hour.is_none() && mapped.seven_day.is_none() {
        let preview: String = serde_json::to_string(&body)
            .unwrap_or_default()
            .chars()
            .take(600)
            .collect();
        return Err(anyhow!(
            "claude.ai usage endpoint returned JSON but no known rate-limit fields \
             were found. Preview: {preview}"
        ));
    }
    Ok(mapped)
}
