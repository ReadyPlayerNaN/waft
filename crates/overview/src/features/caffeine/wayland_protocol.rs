//! Wayland idle-inhibit protocol implementation (INCOMPLETE)
//!
//! ## Status: ⚠️ Partial Implementation
//!
//! This module establishes a connection to the Wayland compositor and binds to the
//! `zwp_idle_inhibit_manager_v1` protocol, but does NOT create actual surface-based
//! inhibitors.
//!
//! ## Why Incomplete?
//!
//! The Wayland idle-inhibit protocol requires a `wl_surface` to create an inhibitor:
//! ```c
//! zwp_idle_inhibit_manager_v1.create_inhibitor(wl_surface) -> zwp_idle_inhibitor_v1
//! ```
//!
//! The challenge is that GTK owns the window's `wl_surface`, and mixing GTK's Wayland
//! objects with `wayland-client`'s event loop is architecturally complex:
//!
//! 1. GTK's `wl_display` and `wl_surface` pointers can't be safely shared with wayland-client
//! 2. Creating a separate Wayland connection means we can't use GTK's surfaces
//! 3. The wayland-client API changed between versions, removing `from_c_ptr()` methods
//! 4. We'd need to either:
//!    - Implement raw FFI using wayland-sys (complex, unsafe)
//!    - Fork/wait for gtk4-layer-shell to support GTK 0.11, then upgrade (breaking change)
//!    - Create a separate daemon that owns surfaces (architectural overhead)
//!
//! ## Current Behavior
//!
//! - ✅ Detects Wayland environment correctly
//! - ✅ Establishes independent Wayland connection
//! - ✅ Binds to `zwp_idle_inhibit_manager_v1` protocol
//! - ⚠️ Does NOT create surface-based inhibitors
//! - ⚠️ Automatically falls back to Portal backend
//!
//! ## Recommended Alternative
//!
//! **Use the Portal backend instead** - it works perfectly on Wayland (including niri, Sway,
//! Hyprland) via xdg-desktop-portal and actually prevents screen locking.
//!
//! The probe will automatically skip this backend and use Portal, so this is transparent
//! to users.

use anyhow::{Result, bail};
use log::debug;
use std::sync::{Mutex, OnceLock};
use wayland_client::{Connection, Dispatch, QueueHandle, Proxy, globals::GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_surface::WlSurface};
use wayland_protocols::wp::idle_inhibit::zv1::client::{
    zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1,
    zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
};

/// State for Wayland protocol dispatch
struct IdleInhibitState;

impl Dispatch<WlRegistry, GlobalListContents> for IdleInhibitState {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpIdleInhibitManagerV1, ()> for IdleInhibitState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpIdleInhibitManagerV1,
        _event: <ZwpIdleInhibitManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpIdleInhibitorV1, ()> for IdleInhibitState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpIdleInhibitorV1,
        _event: <ZwpIdleInhibitorV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSurface, ()> for IdleInhibitState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSurface,
        _event: <WlSurface as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

/// Struct to hold Wayland connection state
struct WaylandConnection {
    _connection: Connection,
    manager: ZwpIdleInhibitManagerV1,
    #[allow(dead_code)]
    queue_handle: QueueHandle<IdleInhibitState>,
}

// Global storage for the Wayland connection
static WAYLAND_CONNECTION: OnceLock<Mutex<Option<WaylandConnection>>> = OnceLock::new();

fn get_connection() -> &'static Mutex<Option<WaylandConnection>> {
    WAYLAND_CONNECTION.get_or_init(|| Mutex::new(None))
}

/// Initialize the Wayland connection and bind to the idle-inhibit manager
fn ensure_connection_initialized() -> Result<()> {
    // Check if we already have a connection
    if let Ok(guard) = get_connection().lock() {
        if let Some(ref conn) = *guard {
            if conn.manager.is_alive() {
                return Ok(());
            }
        }
    }

    debug!("[wayland] Initializing Wayland connection...");

    // Connect to Wayland display
    let conn = Connection::connect_to_env()
        .map_err(|e| anyhow::anyhow!("Failed to connect to Wayland: {}", e))?;

    // Get globals
    let (globals, mut event_queue) = wayland_client::globals::registry_queue_init::<IdleInhibitState>(&conn)
        .map_err(|e| anyhow::anyhow!("Failed to init registry: {}", e))?;

    let mut state = IdleInhibitState;

    // Roundtrip to get all globals
    event_queue.roundtrip(&mut state)
        .map_err(|e| anyhow::anyhow!("Roundtrip failed: {}", e))?;

    // Bind to idle-inhibit manager
    let manager: ZwpIdleInhibitManagerV1 = globals
        .bind(&event_queue.handle(), 1..=1, ())
        .map_err(|e| anyhow::anyhow!("Failed to bind idle-inhibit manager: {}", e))?;

    debug!("[wayland] Successfully bound to idle-inhibit manager");

    // Store the connection
    let wayland_conn = WaylandConnection {
        _connection: conn,
        manager,
        queue_handle: event_queue.handle().clone(),
    };

    if let Ok(mut guard) = get_connection().lock() {
        *guard = Some(wayland_conn);
    }

    Ok(())
}

/// Create an idle inhibitor
/// Note: This creates an inhibitor without a specific surface, which inhibits idle for the whole session
/// Scaffolding for future surface-based inhibition implementation
#[allow(dead_code)]
pub fn create_idle_inhibitor() -> Result<ZwpIdleInhibitorV1> {
    ensure_connection_initialized()?;

    let guard = get_connection().lock()
        .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

    let _conn = guard.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Wayland connection not initialized"))?;

    // Get a compositor surface to inhibit against
    // For a global inhibit, we need to create a minimal surface
    // For simplicity, we'll just create the inhibitor - some compositors accept nil surface
    // Actually, the protocol requires a surface. We need to use GTK's surface.
    // This is getting complex - let's just store that we're active without the actual inhibitor

    bail!("Surface-based inhibition not yet fully implemented")
}

/// Simplified API: track inhibition state without creating actual surface inhibitors
///
/// This function ensures the Wayland protocol connection is established and
/// logs the inhibition state, but does NOT create actual `zwp_idle_inhibitor_v1`
/// objects due to the surface integration complexity described above.
///
/// In practice, this backend is skipped during probe, and the Portal backend
/// is used instead to actually prevent screen locking.
static INHIBIT_ACTIVE: OnceLock<Mutex<bool>> = OnceLock::new();

fn get_inhibit_state() -> &'static Mutex<bool> {
    INHIBIT_ACTIVE.get_or_init(|| Mutex::new(false))
}

/// Set the inhibition state (establishes protocol connection but doesn't create inhibitor)
///
/// **Note:** This does NOT actually prevent screen locking. It only establishes
/// the protocol connection to verify that idle-inhibit is available. The actual
/// inhibition is handled by the Portal backend fallback.
pub fn set_inhibit_active(active: bool) -> Result<()> {
    ensure_connection_initialized()?;

    if let Ok(mut guard) = get_inhibit_state().lock() {
        *guard = active;
        if active {
            debug!("[wayland] Idle inhibition state: active (NOTE: protocol connection only, no actual inhibitor created)");
        } else {
            debug!("[wayland] Idle inhibition state: inactive");
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn is_inhibit_active() -> bool {
    get_inhibit_state().lock()
        .map(|guard| *guard)
        .unwrap_or(false)
}
