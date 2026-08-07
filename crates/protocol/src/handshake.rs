use serde::{Deserialize, Serialize};

use crate::error::ProtocolError;

pub const CAP_HANDSHAKE: &str = "handshake";
pub const CAP_STRUCTURED_ERRORS: &str = "structured-errors";
pub const CAP_DERIVED_ENTITY_TYPE: &str = "derived-entity-type";
pub const CAP_SCHEMA_METADATA: &str = "schema-metadata";
pub const CAP_STATUS_COMPLETE: &str = "status-complete";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PeerRole {
    App,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hello {
    pub role: PeerRole,
    pub implementation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_name: Option<String>,
    pub min_version: u32,
    pub max_version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloAck {
    pub negotiated_version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HelloError {
    pub error: ProtocolError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum HandshakeMessage {
    Hello(Hello),
    HelloAck(HelloAck),
    HelloError(HelloError),
}

impl Hello {
    pub fn app(implementation: impl Into<String>, version: u32, capabilities: Vec<String>) -> Self {
        Self {
            role: PeerRole::App,
            implementation: implementation.into(),
            plugin_name: None,
            min_version: version,
            max_version: version,
            capabilities,
        }
    }

    pub fn plugin(
        plugin_name: impl Into<String>,
        implementation: impl Into<String>,
        version: u32,
        capabilities: Vec<String>,
    ) -> Self {
        Self {
            role: PeerRole::Plugin,
            implementation: implementation.into(),
            plugin_name: Some(plugin_name.into()),
            min_version: version,
            max_version: version,
            capabilities,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(value: &T) {
        let json = serde_json::to_string(value).expect("serialize");
        let decoded: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, *value);
    }

    #[test]
    fn hello_roundtrip() {
        roundtrip(&HandshakeMessage::Hello(Hello::app(
            "waft-client",
            2,
            vec![CAP_HANDSHAKE.to_string()],
        )));
    }

    #[test]
    fn hello_ack_roundtrip() {
        roundtrip(&HandshakeMessage::HelloAck(HelloAck {
            negotiated_version: 2,
            capabilities: vec![CAP_DERIVED_ENTITY_TYPE.to_string()],
        }));
    }

    #[test]
    fn hello_error_roundtrip() {
        roundtrip(&HandshakeMessage::HelloError(HelloError {
            error: ProtocolError::validation("bad hello"),
        }));
    }
}
