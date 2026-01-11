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

use crate::relm4_app::plugins::notifications::types::NotificationDisplay;

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

/// Parsed notification action pair: (action_key, label).
#[derive(Clone, Debug)]
pub struct ActionSpec {
    pub key: String,
    pub label: String,
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
    Notify { notification: NotificationDisplay },

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
