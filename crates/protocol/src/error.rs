use serde::{Deserialize, Serialize};

/// Machine-readable protocol error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolError {
    /// Stable machine-readable error code.
    pub code: String,
    /// Concise English developer-facing message.
    pub message: String,
    /// Error classification scope.
    pub scope: ProtocolErrorScope,
    /// Whether retrying the same operation may succeed.
    pub retryable: bool,
    /// Optional machine-readable details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProtocolErrorScope {
    Transport,
    Handshake,
    Validation,
    Capability,
    NotFound,
    Timeout,
    Action,
}

impl ProtocolError {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        scope: ProtocolErrorScope,
        retryable: bool,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            scope,
            retryable,
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn handshake_incompatible(message: impl Into<String>, details: serde_json::Value) -> Self {
        Self::new(
            "handshake.incompatible-version",
            message,
            ProtocolErrorScope::Handshake,
            false,
        )
        .with_details(details)
    }

    pub fn capability_not_negotiated(message: impl Into<String>) -> Self {
        Self::new(
            "capability.not-negotiated",
            message,
            ProtocolErrorScope::Capability,
            false,
        )
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(
            "protocol.validation",
            message,
            ProtocolErrorScope::Validation,
            false,
        )
    }

    pub fn action(message: impl Into<String>) -> Self {
        Self::new(
            "action.execution",
            message,
            ProtocolErrorScope::Action,
            false,
        )
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new("action.timeout", message, ProtocolErrorScope::Timeout, true)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(
            "entity.not-found",
            message,
            ProtocolErrorScope::NotFound,
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_constructors_set_expected_codes_and_scope() {
        let handshake =
            ProtocolError::handshake_incompatible("bad version", serde_json::json!({"want": 3}));
        assert_eq!(handshake.code, "handshake.incompatible-version");
        assert_eq!(handshake.scope, ProtocolErrorScope::Handshake);
        assert!(handshake.details.is_some());

        let capability = ProtocolError::capability_not_negotiated("missing capability");
        assert_eq!(capability.code, "capability.not-negotiated");
        assert_eq!(capability.scope, ProtocolErrorScope::Capability);

        let validation = ProtocolError::validation("bad request");
        assert_eq!(validation.code, "protocol.validation");
        assert_eq!(validation.scope, ProtocolErrorScope::Validation);

        let action = ProtocolError::action("boom");
        assert_eq!(action.code, "action.execution");
        assert_eq!(action.scope, ProtocolErrorScope::Action);

        let timeout = ProtocolError::timeout("slow");
        assert_eq!(timeout.code, "action.timeout");
        assert_eq!(timeout.scope, ProtocolErrorScope::Timeout);
        assert!(timeout.retryable);

        let not_found = ProtocolError::not_found("gone");
        assert_eq!(not_found.code, "entity.not-found");
        assert_eq!(not_found.scope, ProtocolErrorScope::NotFound);
    }
}
