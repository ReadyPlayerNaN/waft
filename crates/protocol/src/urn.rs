//! URN (Uniform Resource Name) type for entity identification.
//!
//! URN format: `{plugin}/{entity-type}/{id}` with optional nesting via
//! additional `/{entity-type}/{id}` segments.
//!
//! Examples:
//! - `clock/clock/default`
//! - `blueman/bluetooth-adapter/hci0`
//! - `blueman/bluetooth-adapter/hci0/bluetooth-device/AA:BB:CC:DD:EE:FF`

use serde::{Deserialize, Serialize};
use std::fmt;

/// Error type for URN parsing failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrnError {
    /// URN string is empty.
    Empty,
    /// URN must have at least 3 segments (plugin/entity-type/id).
    TooFewSegments,
    /// URN has an even number of segments after the plugin prefix,
    /// meaning an entity-type is missing its id.
    IncompleteSegment,
    /// A segment within the URN is empty (e.g. `clock//default`).
    EmptySegment,
}

impl fmt::Display for UrnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrnError::Empty => write!(f, "URN is empty"),
            UrnError::TooFewSegments => {
                write!(
                    f,
                    "URN must have at least 3 segments: plugin/entity-type/id"
                )
            }
            UrnError::IncompleteSegment => {
                write!(f, "URN has incomplete segment: entity-type without id")
            }
            UrnError::EmptySegment => write!(f, "URN contains an empty segment"),
        }
    }
}

impl std::error::Error for UrnError {}

/// A structured entity identifier.
///
/// Format: `{plugin}/{entity-type}/{id}[/{entity-type}/{id}]*`
///
/// URNs uniquely identify entities within the waft protocol. They support
/// nesting for parent-child relationships (e.g. a Bluetooth device under
/// an adapter).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Urn {
    raw: String,
}

impl Urn {
    /// Create a new URN from plugin name, entity type, and id.
    pub fn new(plugin: &str, entity_type: &str, id: &str) -> Self {
        Self {
            raw: format!("{plugin}/{entity_type}/{id}"),
        }
    }

    /// Create a child URN by appending a nested entity-type/id pair.
    pub fn child(&self, entity_type: &str, id: &str) -> Self {
        Self {
            raw: format!("{}/{entity_type}/{id}", self.raw),
        }
    }

    /// Parse a URN from a string.
    ///
    /// Validates that the URN has at least 3 segments and that
    /// entity-type/id pairs are complete.
    pub fn parse(s: &str) -> Result<Self, UrnError> {
        if s.is_empty() {
            return Err(UrnError::Empty);
        }

        let segments: Vec<&str> = s.split('/').collect();

        // Check for empty segments
        for seg in &segments {
            if seg.is_empty() {
                return Err(UrnError::EmptySegment);
            }
        }

        // Need at least: plugin, entity-type, id
        if segments.len() < 3 {
            return Err(UrnError::TooFewSegments);
        }

        // After the plugin prefix, segments come in pairs (entity-type, id).
        // So total segments must be odd: 1 (plugin) + 2*N (entity-type/id pairs).
        let after_plugin = segments.len() - 1;
        if !after_plugin.is_multiple_of(2) {
            return Err(UrnError::IncompleteSegment);
        }

        Ok(Self { raw: s.to_string() })
    }

    /// The plugin name (first segment).
    pub fn plugin(&self) -> &str {
        self.raw
            .split('/')
            .next()
            .expect("URN always has a plugin segment")
    }

    /// The root entity type (first entity-type after plugin).
    /// This is the subscription target for entity updates.
    pub fn root_entity_type(&self) -> &str {
        self.raw
            .split('/')
            .nth(1)
            .expect("URN always has a root entity type")
    }

    /// The leaf entity type (last entity-type in the URN).
    /// For non-nested URNs, this equals `root_entity_type()`.
    pub fn entity_type(&self) -> &str {
        let segments: Vec<&str> = self.raw.split('/').collect();
        // Entity types are at odd indices (1, 3, 5, ...) counting from 0
        // The last entity-type is at index segments.len() - 2
        segments[segments.len() - 2]
    }

    /// The leaf id (last id in the URN).
    pub fn id(&self) -> &str {
        self.raw
            .split('/')
            .next_back()
            .expect("URN always has an id")
    }

    /// The raw URN string.
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl fmt::Display for Urn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_simple_urn() {
        let urn = Urn::new("clock", "clock", "default");
        assert_eq!(urn.as_str(), "clock/clock/default");
        assert_eq!(urn.plugin(), "clock");
        assert_eq!(urn.entity_type(), "clock");
        assert_eq!(urn.root_entity_type(), "clock");
        assert_eq!(urn.id(), "default");
    }

    #[test]
    fn child_creates_nested_urn() {
        let parent = Urn::new("blueman", "bluetooth-adapter", "hci0");
        let child = parent.child("bluetooth-device", "AA:BB:CC:DD:EE:FF");

        assert_eq!(
            child.as_str(),
            "blueman/bluetooth-adapter/hci0/bluetooth-device/AA:BB:CC:DD:EE:FF"
        );
        assert_eq!(child.plugin(), "blueman");
        assert_eq!(child.root_entity_type(), "bluetooth-adapter");
        assert_eq!(child.entity_type(), "bluetooth-device");
        assert_eq!(child.id(), "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn parse_simple_urn() {
        let urn = Urn::parse("battery/battery/BAT0").unwrap();
        assert_eq!(urn.plugin(), "battery");
        assert_eq!(urn.entity_type(), "battery");
        assert_eq!(urn.id(), "BAT0");
    }

    #[test]
    fn parse_nested_urn() {
        let urn = Urn::parse("blueman/bluetooth-adapter/hci0/bluetooth-device/AA:BB:CC").unwrap();
        assert_eq!(urn.plugin(), "blueman");
        assert_eq!(urn.root_entity_type(), "bluetooth-adapter");
        assert_eq!(urn.entity_type(), "bluetooth-device");
        assert_eq!(urn.id(), "AA:BB:CC");
    }

    #[test]
    fn parse_empty_string() {
        assert_eq!(Urn::parse(""), Err(UrnError::Empty));
    }

    #[test]
    fn parse_too_few_segments() {
        assert_eq!(Urn::parse("clock"), Err(UrnError::TooFewSegments));
        assert_eq!(Urn::parse("clock/clock"), Err(UrnError::TooFewSegments));
    }

    #[test]
    fn parse_incomplete_segment() {
        // 4 segments = 1 plugin + 3 after = odd after plugin -> incomplete
        assert_eq!(
            Urn::parse("blueman/bluetooth-adapter/hci0/bluetooth-device"),
            Err(UrnError::IncompleteSegment)
        );
    }

    #[test]
    fn parse_empty_segment() {
        assert_eq!(Urn::parse("clock//default"), Err(UrnError::EmptySegment));
        assert_eq!(Urn::parse("/clock/default"), Err(UrnError::EmptySegment));
        assert_eq!(Urn::parse("clock/clock/"), Err(UrnError::EmptySegment));
    }

    #[test]
    fn display_roundtrip() {
        let urn = Urn::new("audio", "audio-device", "speakers");
        let displayed = urn.to_string();
        let parsed = Urn::parse(&displayed).unwrap();
        assert_eq!(urn, parsed);
    }

    #[test]
    fn display_roundtrip_nested() {
        let urn =
            Urn::new("blueman", "bluetooth-adapter", "hci0").child("bluetooth-device", "AA:BB:CC");
        let displayed = urn.to_string();
        let parsed = Urn::parse(&displayed).unwrap();
        assert_eq!(urn, parsed);
    }

    #[test]
    fn serde_roundtrip() {
        let urn = Urn::new("audio", "audio-device", "speakers");
        let json = serde_json::to_string(&urn).unwrap();
        let deserialized: Urn = serde_json::from_str(&json).unwrap();
        assert_eq!(urn, deserialized);
    }

    #[test]
    fn urn_equality() {
        let a = Urn::new("clock", "clock", "default");
        let b = Urn::parse("clock/clock/default").unwrap();
        assert_eq!(a, b);
    }
}
