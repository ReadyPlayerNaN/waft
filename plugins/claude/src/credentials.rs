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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    fn write_credentials(dir: &std::path::Path, expires_at: i64) {
        let claude_dir = dir.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let json = format!(
            r#"{{"claudeAiOauth":{{"accessToken":"test-token","refreshToken":"ref","expiresAt":{expires_at}}}}}"#
        );
        std::fs::write(claude_dir.join(".credentials.json"), json).unwrap();
    }

    #[test]
    fn expired_token_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        write_credentials(tmp.path(), 1000); // Far in the past
        // Temporarily override HOME so load_access_token reads our test file.
        // SAFETY: tests that touch env vars must not run in parallel — use
        // std::env::set_var carefully; this is acceptable in a unit test binary.
        let original_home = std::env::var("HOME").ok();
        // SAFETY: single-threaded unit test binary; no other threads read HOME concurrently.
        unsafe { std::env::set_var("HOME", tmp.path()) };
        let result = load_access_token();
        if let Some(h) = original_home {
            unsafe { std::env::set_var("HOME", h) };
        }
        assert!(result.is_err(), "expected Err for expired token, got Ok");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("expired"), "error should mention expired: {msg}");
    }

    #[test]
    fn valid_token_returns_ok() {
        let tmp = tempfile::tempdir().unwrap();
        write_credentials(tmp.path(), now_ms() + 3_600_000); // 1 hour from now
        let original_home = std::env::var("HOME").ok();
        // SAFETY: single-threaded unit test binary; no other threads read HOME concurrently.
        unsafe { std::env::set_var("HOME", tmp.path()) };
        let result = load_access_token();
        if let Some(h) = original_home {
            unsafe { std::env::set_var("HOME", h) };
        }
        assert!(result.is_ok(), "expected Ok for valid token, got {result:?}");
        assert_eq!(result.unwrap(), "test-token");
    }
}
