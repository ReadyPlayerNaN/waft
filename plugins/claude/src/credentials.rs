//! Read Claude Code OAuth credentials from ~/.claude/.credentials.json.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OauthCredentials {
    access_token: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CredentialsFile {
    claude_ai_oauth: OauthCredentials,
}

/// Load a valid (non-expired) access token from ~/.claude/.credentials.json.
///
/// Returns `Err` if the file is missing, malformed, or the token is expired.
pub fn load_access_token() -> Result<String> {
    let path = credentials_path()?;
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    let file: CredentialsFile = serde_json::from_str(&content)
        .with_context(|| format!("Cannot parse {}", path.display()))?;

    let creds = file.claude_ai_oauth;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    if creds.expires_at <= now_ms {
        bail!(
            "Claude Code access token expired (expiresAt={}). \
             Run claude to refresh it.",
            creds.expires_at
        );
    }

    Ok(creds.access_token)
}

fn credentials_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME env var not set")?;
    Ok(PathBuf::from(home).join(".claude").join(".credentials.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_token_detected() {
        // expiresAt in the past (1970)
        let json = r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"ref","expiresAt":1000}}"#;
        let file: CredentialsFile = serde_json::from_str(json).unwrap();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!(file.claude_ai_oauth.expires_at < now_ms);
    }

    #[test]
    fn future_token_not_expired() {
        // expiresAt far in the future
        let json = r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"ref","expiresAt":9999999999999}}"#;
        let file: CredentialsFile = serde_json::from_str(json).unwrap();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!(file.claude_ai_oauth.expires_at > now_ms);
    }
}
