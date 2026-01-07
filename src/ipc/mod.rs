//! IPC utilities shared by the legacy and Relm4 app paths.
//!
//! This module is intentionally:
#![allow(dead_code)]
//! - GTK-free
//! - fast to unit-test
//! - tolerant in parsing (supports both `{"cmd":"toggle"}` and `{"command":"toggle"}`)
//!
//! It provides:
//! - socket path computation (Wayland-only by project design)
//! - command parsing for `toggle` / `show` / `hide` (plus `ping`)
//! - async networking helpers under `ipc::net`
//! - simple argv-to-command mapping for the new Relm4 CLI mode:
//!   - `sacrebleui`            => start server (if already running => error / non-zero)
//!   - `sacrebleui toggle`     => client: send toggle
//!   - `sacrebleui show|hide`  => client: send show/hide

pub mod net;

use std::path::{Path, PathBuf};

/// IPC command understood by the app.
///
/// Keep this enum stable; it defines the CLI/IPC surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcCommand {
    Show,
    Hide,
    Toggle,
    Ping,
    Stop,
}

impl IpcCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            IpcCommand::Show => "show",
            IpcCommand::Hide => "hide",
            IpcCommand::Toggle => "toggle",
            IpcCommand::Ping => "ping",
            IpcCommand::Stop => "stop",
        }
    }
}

/// Minimal error type for IPC parsing/path computation.
///
/// This stays dependency-free so it can be used by both entrypoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcError {
    /// Required environment variable is missing.
    MissingEnv(&'static str),

    /// Command was not recognized.
    UnknownCommand(String),

    /// Invalid/empty request.
    InvalidRequest(String),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::MissingEnv(k) => write!(f, "missing environment variable: {k}"),
            IpcError::UnknownCommand(c) => write!(f, "unknown ipc command: {c}"),
            IpcError::InvalidRequest(m) => write!(f, "invalid ipc request: {m}"),
        }
    }
}

impl std::error::Error for IpcError {}

/// Compute the IPC socket path.
///
/// Policy (Wayland-only by project design):
/// - Requires `XDG_RUNTIME_DIR` (Wayland session expected).
/// - Incorporates `WAYLAND_DISPLAY` for per-session uniqueness.
/// - Does NOT incorporate UID: the runtime dir is already per-user (e.g. `/run/user/<uid>`),
///   so including UID is redundant and relying on `$UID` is brittle.
pub fn ipc_socket_path() -> Result<PathBuf, IpcError> {
    let runtime_dir =
        std::env::var_os("XDG_RUNTIME_DIR").ok_or(IpcError::MissingEnv("XDG_RUNTIME_DIR"))?;

    let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());

    let filename = format!("sacrebleui.{wayland_display}.sock");
    Ok(PathBuf::from(runtime_dir).join(filename))
}

/// Parse a command keyword (case-insensitive).
pub fn parse_command_word(word: &str) -> Option<IpcCommand> {
    match word.trim().to_ascii_lowercase().as_str() {
        "show" => Some(IpcCommand::Show),
        "hide" => Some(IpcCommand::Hide),
        "toggle" => Some(IpcCommand::Toggle),
        "ping" => Some(IpcCommand::Ping),
        "stop" => Some(IpcCommand::Stop),
        _ => None,
    }
}

/// Serialize an `IpcCommand` into a single-line JSON request.
///
/// This intentionally matches the legacy IPC schema (`{"cmd":"toggle"}`).
pub fn command_to_json_line(cmd: IpcCommand) -> String {
    format!(r#"{{"cmd":"{}"}}"#, cmd.as_str()) + "\n"
}

/// Extract the command name from a JSON payload.
///
/// Supported schemas:
/// - `{"cmd":"toggle"}`
/// - `{"command":"toggle"}`
///
/// This is deliberately tolerant and does not require a JSON library.
/// It scans for `"cmd"` or `"command"` keys with a string value.
///
/// Returns `Ok(None)` if no known key exists.
pub fn command_name_from_json(payload: &str) -> Result<Option<String>, IpcError> {
    let s = payload.trim();
    if s.is_empty() {
        return Err(IpcError::InvalidRequest("empty payload".into()));
    }

    // Very small tolerant extractor: find `"cmd"` or `"command"` and take the following string value.
    // We intentionally avoid pulling in serde here so Relm4 path can stay minimal.
    fn extract_for_key(s: &str, key: &str) -> Option<String> {
        let needle = format!(r#""{key}""#);
        let idx = s.find(&needle)?;
        let after_key = &s[idx + needle.len()..];

        // Find ':' after key
        let colon = after_key.find(':')?;
        let after_colon = after_key[colon + 1..].trim_start();

        // Must start with a quote
        let after_quote = after_colon.strip_prefix('"')?;
        let end_quote = after_quote.find('"')?;
        Some(after_quote[..end_quote].to_string())
    }

    if let Some(v) = extract_for_key(s, "cmd") {
        return Ok(Some(v));
    }
    if let Some(v) = extract_for_key(s, "command") {
        return Ok(Some(v));
    }

    Ok(None)
}

/// Parse an IPC command from a JSON payload.
///
/// Returns:
/// - `Ok(cmd)` if a known command is present
/// - `Err(IpcError::UnknownCommand)` if `cmd`/`command` is present but unknown
/// - `Err(IpcError::InvalidRequest)` if payload is empty/invalid
pub fn parse_command_from_json(payload: &str) -> Result<IpcCommand, IpcError> {
    let Some(name) = command_name_from_json(payload)? else {
        return Err(IpcError::InvalidRequest(
            r#"missing "cmd" or "command" key"#.into(),
        ));
    };

    parse_command_word(&name).ok_or_else(|| IpcError::UnknownCommand(name))
}

/// Map argv-style CLI args into an optional IPC command.
///
/// New Relm4 CLI policy (per user request):
/// - no args => `Ok(None)` (start server)
/// - `toggle|show|hide|ping` => `Ok(Some(cmd))` (client mode)
///
/// Any other args => `Err(UnknownCommand)`
pub fn command_from_args(args: &[String]) -> Result<Option<IpcCommand>, IpcError> {
    if args.len() <= 1 {
        return Ok(None);
    }

    // Join args like the legacy implementation did (tolerant).
    // If it's raw words (e.g. `toggle`), parse as word; if it's JSON, parse as JSON.
    let joined = args[1..].join(" ").trim().to_string();
    if joined.is_empty() {
        return Ok(None);
    }

    // If it looks like JSON, parse JSON schema.
    if joined.starts_with('{') {
        return Ok(Some(parse_command_from_json(&joined)?));
    }

    // Otherwise treat first token as command word.
    let token = joined.split_whitespace().next().unwrap_or("");
    let cmd = parse_command_word(token).ok_or_else(|| IpcError::UnknownCommand(token.into()))?;
    Ok(Some(cmd))
}

/// Convenience: does the IPC socket path currently exist?
pub fn socket_exists(socket: &Path) -> bool {
    socket.exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_word_is_case_insensitive() {
        assert_eq!(parse_command_word("TOGGLE"), Some(IpcCommand::Toggle));
        assert_eq!(parse_command_word(" show "), Some(IpcCommand::Show));
        assert_eq!(parse_command_word("hide"), Some(IpcCommand::Hide));
        assert_eq!(parse_command_word("ping"), Some(IpcCommand::Ping));
        assert_eq!(parse_command_word("STOP"), Some(IpcCommand::Stop));
        assert_eq!(parse_command_word("nope"), None);
    }

    #[test]
    fn json_extract_supports_cmd_and_command_keys() {
        assert_eq!(
            command_name_from_json(r#"{"cmd":"toggle"}"#).unwrap(),
            Some("toggle".to_string())
        );
        assert_eq!(
            command_name_from_json(r#"{"command":"show"}"#).unwrap(),
            Some("show".to_string())
        );
    }

    #[test]
    fn parse_command_from_json_rejects_missing_cmd() {
        let err = parse_command_from_json(r#"{"x":"toggle"}"#).unwrap_err();
        assert!(matches!(err, IpcError::InvalidRequest(_)));
    }

    #[test]
    fn parse_command_from_json_rejects_unknown_cmd() {
        let err = parse_command_from_json(r#"{"cmd":"wat"}"#).unwrap_err();
        assert!(matches!(err, IpcError::UnknownCommand(_)));
    }

    #[test]
    fn command_to_json_line_matches_legacy_schema() {
        assert_eq!(
            command_to_json_line(IpcCommand::Toggle),
            "{\"cmd\":\"toggle\"}\n"
        );
    }

    #[test]
    fn command_from_args_supports_words_and_json() {
        let args = vec!["sacrebleui".to_string(), "toggle".to_string()];
        assert_eq!(command_from_args(&args).unwrap(), Some(IpcCommand::Toggle));

        let args = vec!["sacrebleui".to_string(), r#"{"cmd":"hide"}"#.to_string()];
        assert_eq!(command_from_args(&args).unwrap(), Some(IpcCommand::Hide));

        let args = vec!["sacrebleui".to_string()];
        assert_eq!(command_from_args(&args).unwrap(), None);
    }
}
