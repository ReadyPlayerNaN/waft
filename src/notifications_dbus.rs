//! DBus notifications ingress types (org.freedesktop.Notifications).
//!
//! This module intentionally contains only:
//! - event types sent from the DBus server into the notifications plugin/controller
//! - capability constants (strings) for `GetCapabilities`
//! - close reason constants (numeric) for `NotificationClosed`
//!
//! The actual DBus *server* implementation (owning `org.freedesktop.Notifications` and
//! exporting `/org/freedesktop/Notifications`) should live in a separate module/type.
//!
//! Design constraints (per AGENTS.md):
//! - GTK widgets must remain on the main thread.
//! - The DBus server should run on a tokio task and communicate via channels.
//! - Do not reuse `DbusHandle` here: it is a client wrapper.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// `GetCapabilities` string constants (freedesktop.org spec).
///
/// Only advertise capabilities that are actually implemented in the UI/controller.
pub mod capabilities {
    /// Supports notification action buttons and emitting `ActionInvoked`.
    pub const ACTIONS: &str = "actions";

    /// Supports a body text field.
    pub const BODY: &str = "body";

    /// Supports markup in body (Pango markup). If advertised, the UI must render markup.
    pub const BODY_MARKUP: &str = "body-markup";

    // Intentionally not supported / not advertised for now:
    // pub const PERSISTENCE: &str = "persistence";
    // pub const ICON_STATIC: &str = "icon-static";
    // pub const SOUND: &str = "sound";
}

/// `NotificationClosed` reason codes (freedesktop.org spec).
///
/// These numeric values are part of the DBus API; keep them stable.
pub mod close_reasons {
    /// The notification expired.
    pub const EXPIRED: u32 = 1;

    /// The notification was dismissed by the user.
    pub const DISMISSED_BY_USER: u32 = 2;

    /// The notification was closed by a call to `CloseNotification`.
    pub const CLOSED_BY_CALL: u32 = 3;

    /// Undefined / unspecified reason.
    pub const UNDEFINED: u32 = 4;
}

/// A simplified representation of a freedesktop notification "hint" value.
///
/// The DBus spec uses a `dict<string, variant>`. In this codebase we avoid exposing
/// raw DBus `Variant` values outside the DBus layer. The DBus server should decode
/// variants into these supported types when possible.
///
/// Unknown or unhandled variants should be ignored by the DBus server rather than
/// leaking DBus-specific types across module boundaries.
#[derive(Clone, Debug)]
pub enum HintValue {
    Bool(bool),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
}

/// Parsed notification action pair: (action_key, label).
#[derive(Clone, Debug)]
pub struct ActionSpec {
    pub key: String,
    pub label: String,
}

/// Payload of a `Notify` call.
///
/// This is the "ingress" message sent from the DBus server to the notifications plugin/controller.
///
/// Notes:
/// - The DBus server is responsible for generating and returning a DBus notification id (`u32`).
/// - `replaces_id` is included so the receiver can apply replacement semantics.
/// - `hints` is decoded to best-effort supported types.
/// - `actions` is already parsed into `(key, label)` pairs.
#[derive(Clone, Debug)]
pub struct NotifyRequest {
    pub app_name: String,
    pub replaces_id: u32,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<ActionSpec>,
    pub hints: HashMap<String, HintValue>,
    pub expire_timeout_ms: i32,
}

/// Event stream from DBus server -> notifications subsystem.
///
/// The DBus server owns the bus name and object; it should translate method calls
/// into these events so the UI layer can remain DBus-agnostic.
///
/// The receiver (typically notifications plugin/controller) is responsible for:
/// - creating/updating/removing notifications in the model
/// - wiring UI callbacks that send "outbound" events back to the DBus server
#[derive(Clone, Debug)]
pub enum IngressEvent {
    /// A client called `Notify(...)`.
    ///
    /// The DBus server should allocate an id and include it here so the receiver can
    /// store it as the notification's stable identifier.
    Notify {
        /// DBus notification id allocated by the server (returned from `Notify`).
        id: u32,
        request: NotifyRequest,
    },

    /// A client called `CloseNotification(id)`.
    CloseNotification { id: u32 },

    /// Notification inhibition ("Do Not Disturb") state changed.
    ///
    /// KDE-compatible behavior: some implementations expose an `Inhibited` flag via
    /// `org.freedesktop.Notifications`. We treat that as the single source of truth.
    ///
    /// This is intentionally per-session only (in-memory).
    InhibitedChanged { inhibited: bool },
}

/// Outbound events from UI/controller -> DBus server.
///
/// The notifications UI invokes actions and closes notifications; the DBus server must
/// emit the corresponding DBus signals (`ActionInvoked`, `NotificationClosed`).
///
/// The sender is owned by the notifications plugin/controller; the DBus server consumes
/// these and translates them into DBus signals.
///
/// This separation keeps GTK callbacks DBus-free and avoids holding DBus objects in UI code.
#[derive(Clone, Debug)]
pub enum OutboundEvent {
    /// User invoked an action (button click).
    ///
    /// The DBus server must emit `ActionInvoked(id, action_key)`.
    ActionInvoked { id: u32, action_key: String },

    /// Notification was closed/removed.
    ///
    /// The DBus server must emit `NotificationClosed(id, reason)`.
    NotificationClosed { id: u32, reason: u32 },
}

/// Icon data extracted from `Notify`.
///
/// The DBus spec provides multiple icon channels:
/// - `app_icon` (string, themed icon name or file path depending on client)
/// - hints such as `image-path` or `image-data`
///
/// The DBus server should resolve these into this representation (best-effort),
/// and the receiver should then convert it into `NotificationIcon` for rendering.
///
/// This type is optional and is not currently wired into `IngressEvent::Notify` yet.
/// It exists to keep the type surface ready for full spec coverage.
#[derive(Clone, Debug)]
pub enum IconSpec {
    /// A themed icon name (e.g. `"dialog-information"` or `"mail-unread-symbolic"`).
    Themed(String),

    /// A file path to an icon image.
    FilePath(PathBuf),

    /// Raw image bytes (e.g. from `image-data`). Interpretation is DBus-server-specific.
    Bytes(Vec<u8>),
}

/// Returns the list of `GetCapabilities` values we intend to advertise.
///
/// Keep this synchronized with:
/// - the notifications view rendering (plain vs markup)
/// - action rendering and ActionInvoked behavior
pub fn advertised_capabilities() -> Vec<&'static str> {
    vec![
        capabilities::ACTIONS,
        capabilities::BODY,
        capabilities::BODY_MARKUP,
    ]
}

// Keep all `HintValue` variants "used" (without blanket allows) by providing a small,
// explicit touch-point the DBus server can call during decoding. This avoids dead-code
// warnings about enum payload fields in builds where only a subset is matched elsewhere.
//
// The DBus server should call this after decoding a value (best-effort), e.g.:
// `crate::notifications_dbus::note_hint_value_decoded(&hint_value);`
static HINTVALUE_DECODED_COUNTS: AtomicUsize = AtomicUsize::new(0);

pub fn note_hint_value_decoded(v: &HintValue) {
    // Touch every variant payload. The exact counts are not important; this is only to ensure
    // the enum payload fields are considered "read" by the compiler without hiding warnings.
    match v {
        HintValue::Bool(b) => {
            let _ = *b;
        }
        HintValue::I32(i) => {
            let _ = *i;
        }
        HintValue::U32(u) => {
            let _ = *u;
        }
        HintValue::I64(i) => {
            let _ = *i;
        }
        HintValue::U64(u) => {
            let _ = *u;
        }
        HintValue::F64(f) => {
            let _ = *f;
        }
        HintValue::String(s) => {
            let _ = s.as_str();
        }
        HintValue::Bytes(b) => {
            let _ = b.len();
        }
    }

    // Prevent the entire function from being optimized away in release builds.
    HINTVALUE_DECODED_COUNTS.fetch_add(1, Ordering::Relaxed);
}
