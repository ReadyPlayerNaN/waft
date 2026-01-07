//! App-wide event/message types for the Relm4 migration router layer.
//!
//! Updated design (Option 1.5A — typed plugin handles):
//! - Keep this module GTK-free and fast to unit-test.
//! - New code should use `AppMsg` (legacy `UiEvent` remains for old code only).
//! - Avoid centralizing plugin-specific message enums in the router.
//!   Plugin-specific `Input` enums live inside plugins and are sent via typed handles
//!   (`PluginSpec` + `PluginHandle`) exposed by the plugin registry/framework.
//!
//! As a result, the router no longer defines a generic `PluginMsg` envelope. Routing to
//! plugin endpoints becomes a concern of the app wiring layer (post-reducer), where the
//! router emits higher-level effects that can be interpreted using typed plugin handles.

use std::borrow::Cow;
use std::fmt;

/// Stable identifier for a plugin.
///
/// This is intentionally an opaque string newtype to avoid centralizing plugin
/// knowledge into the main app/router.
///
/// Formatting conventions (recommended, not enforced):
/// - lowercase
/// - `kebab-case` segments or `namespace::like::this`
///
/// Equality is exact-string equality; no normalization is applied.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PluginId(Cow<'static, str>);

impl PluginId {
    /// Create a plugin id from a static string without allocation.
    pub const fn from_static(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }

    /// Create a plugin id from an owned string.
    pub fn from_string(s: String) -> Self {
        Self(Cow::Owned(s))
    }

    /// Borrow the underlying id string.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Debug for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PluginId").field(&self.as_str()).finish()
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&'static str> for PluginId {
    fn from(value: &'static str) -> Self {
        Self::from_static(value)
    }
}

impl From<String> for PluginId {
    fn from(value: String) -> Self {
        Self::from_string(value)
    }
}

/// Domain-ish ingress events for the notifications subsystem.
///
/// This is "type plumbing" for step 03; it does not own DBus types and does not
/// imply any DBus ownership behavior changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationsIngress {
    /// Incoming notification request (domain-ish, DBus-free).
    ///
    /// This is shaped to be close to the `org.freedesktop.Notifications.Notify`
    /// method while remaining independent of DBus crates/types.
    Notify(NotifyRequest),

    /// Request to close an existing notification.
    Close { id: u32 },

    /// A user action was invoked for a notification.
    ActionInvoked { id: u32, action_key: String },
}

/// Domain-ish representation of a Notify request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyRequest {
    pub app_name: String,
    pub replaces_id: u32,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub hints: Vec<(String, HintValue)>,
    pub expire_timeout: i32,
}

impl NotifyRequest {
    /// Convenience constructor for tests and callers that don't care about every field.
    pub fn minimal(
        app_name: impl Into<String>,
        summary: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            app_name: app_name.into(),
            replaces_id: 0,
            app_icon: String::new(),
            summary: summary.into(),
            body: body.into(),
            actions: Vec::new(),
            hints: Vec::new(),
            expire_timeout: -1,
        }
    }
}

/// Domain-ish representation of DBus Notify hint values.
///
/// We keep this intentionally small for now; it can be expanded as needed when
/// DBus ingress is wired in step 07+.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintValue {
    Bool(bool),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    Str(String),
    Bytes(Vec<u8>),
}

/// Top-level application message enum for the Relm4 app/router layer.
///
/// This enum is intended to become the central routing surface:
/// - DBus ingress will translate into `AppMsg`
/// - UI-level visibility changes will map to `AppMsg`
///
/// Note: plugin-directed routing is no longer modeled as `AppMsg::ToPlugin { plugin, msg }`
/// because message typing now lives in plugins (Option 1.5A typed handles). The router instead
/// emits higher-level effects (e.g. "overlay shown/hidden") and the app wiring layer decides
/// which plugins to notify using typed handles.
///
/// Step 03 requires, at minimum:
/// - overlay visibility changes
/// - notifications DBus ingress plumbing
/// - toast-window gating events (derived from overlay shown/hidden in the reducer)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMsg {
    /// Overlay became visible/shown.
    OverlayShown,

    /// Overlay became hidden.
    OverlayHidden,

    /// Notifications ingress (domain-ish).
    NotificationsIngress(NotificationsIngress),

    /// Internal / derived events relating to toast visibility gating.
    ///
    /// Step 03: this exists so the router can output a stable "intent" surface.
    /// Later steps will decide exactly who consumes this and how it maps to UI.
    ToastGatingChanged { enabled: bool },
}
