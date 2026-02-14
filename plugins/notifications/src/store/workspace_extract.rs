//! Slack workspace extraction from notification titles.
//!
//! Slack desktop notifications prefix the title with `[workspace_name] ` when
//! the user has multiple workspaces. This module detects and extracts this
//! prefix so that notifications can be grouped per workspace.

use std::sync::Arc;

/// Result of workspace extraction.
pub struct WorkspaceExtraction {
    /// The workspace name (without brackets).
    pub workspace: Arc<str>,
    /// The title with the `[workspace] ` prefix stripped.
    pub cleaned_title: Arc<str>,
}

/// Check if the app is Slack and extract workspace from `[name] rest_of_title`.
///
/// Returns `None` if the app is not Slack or the title doesn't have the
/// `[workspace]` prefix pattern.
pub fn extract_workspace(app_name: &str, title: &str) -> Option<WorkspaceExtraction> {
    if !app_name.eq_ignore_ascii_case("slack") {
        return None;
    }

    let title_trimmed = title.trim();
    if !title_trimmed.starts_with('[') {
        return None;
    }

    let closing = title_trimmed.find(']')?;
    let workspace = &title_trimmed[1..closing];

    if workspace.trim().is_empty() {
        return None;
    }

    let rest = title_trimmed[closing + 1..].trim_start();
    let cleaned_title: Arc<str> = if rest.is_empty() {
        Arc::from(title_trimmed)
    } else {
        Arc::from(rest)
    };

    Some(WorkspaceExtraction {
        workspace: Arc::from(workspace.trim()),
        cleaned_title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_slack_workspace() {
        let result = extract_workspace("Slack", "[Engineering] New message from Alice").unwrap();
        assert_eq!(&*result.workspace, "Engineering");
        assert_eq!(&*result.cleaned_title, "New message from Alice");
    }

    #[test]
    fn case_insensitive_app_name() {
        let result = extract_workspace("slack", "[Marketing] PR update").unwrap();
        assert_eq!(&*result.workspace, "Marketing");
        assert_eq!(&*result.cleaned_title, "PR update");
    }

    #[test]
    fn non_slack_app_returns_none() {
        assert!(extract_workspace("Firefox", "[Tab] Something").is_none());
    }

    #[test]
    fn no_bracket_prefix_returns_none() {
        assert!(extract_workspace("Slack", "New message").is_none());
    }

    #[test]
    fn empty_workspace_returns_none() {
        assert!(extract_workspace("Slack", "[] New message").is_none());
    }

    #[test]
    fn workspace_only_no_title() {
        let result = extract_workspace("Slack", "[Engineering]").unwrap();
        assert_eq!(&*result.workspace, "Engineering");
        assert_eq!(&*result.cleaned_title, "[Engineering]");
    }

    #[test]
    fn workspace_with_spaces() {
        let result = extract_workspace("Slack", "[My Company] Thread reply").unwrap();
        assert_eq!(&*result.workspace, "My Company");
        assert_eq!(&*result.cleaned_title, "Thread reply");
    }

    #[test]
    fn whitespace_only_workspace_returns_none() {
        assert!(extract_workspace("Slack", "[   ] Something").is_none());
    }
}
