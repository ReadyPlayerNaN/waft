//! Preserve machine-readable action failures through the anyhow boundary.
use std::fmt;
use waft_protocol::error::ProtocolError;

#[derive(Debug)]
pub struct PluginActionError(pub ProtocolError);

impl fmt::Display for PluginActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.message.fmt(f)
    }
}
impl std::error::Error for PluginActionError {}

pub(crate) fn action_error_details(error: &anyhow::Error) -> ProtocolError {
    error.downcast_ref::<PluginActionError>().map_or_else(
        || ProtocolError::action(error.to_string()),
        |error| error.0.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_error_survives_context_and_legacy_errors_still_work() {
        let details = ProtocolError::not_found("account no longer exists");
        let error = anyhow::Error::new(PluginActionError(details.clone())).context("action");
        assert_eq!(action_error_details(&error), details);
        assert_eq!(
            action_error_details(&anyhow::anyhow!("legacy")).code,
            "action.execution"
        );
    }
}
