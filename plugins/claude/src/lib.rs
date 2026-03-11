//! Claude Code usage plugin library.
//!
//! Reads OAuth credentials from ~/.claude/.credentials.json and
//! fetches rate limit data from the Anthropic API.

pub mod api;
pub mod credentials;
