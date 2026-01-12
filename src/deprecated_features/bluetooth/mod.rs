//! Bluetooth feature plugin (BlueZ).
//!
//! Provides one contentful FeatureToggle per Bluetooth adapter.
//! Each toggle reflects Adapter Powered (On/Off) and offers a details panel
//! listing paired devices with connect/disconnect switches plus a settings button.
//!
//! Design notes:
//! - DBus IO happens on async tasks.
//! - The details panel content is a plugin-owned GTK widget (Option 2).
//! - The submenu must remain responsive to DBus changes:
//!   - when BlueZ reports Connected/Disconnected, the UI should update immediately
//!     (without requiring an overlay rebuild).
//!
//! Threading boundary (important):
//! - Any async tasks spawned via Tokio must be `Send`.
//! - GTK widgets are NOT `Send`, so they must not be stored inside state guarded by
//!   a Tokio mutex / moved into Tokio tasks.
//! - We therefore keep a **Send-safe** state for DBus/cache, and keep GTK UI in
//!   main-thread-only structures.
//!
//! GTK initialization rule (important):
//! - `Plugin::initialize()` is called BEFORE GTK is initialized in this app.
//! - Therefore: NEVER create GTK widgets in `initialize()`.
//! - Create GTK widgets lazily in `feature_toggles()` / `widgets()` (which are invoked
//!   after GTK is initialized during UI build).
//!
//! UI update strategy (React-ish; MUST FOLLOW):
//! - Do NOT rebuild the whole submenu or re-render from `feature_toggles()` on a loop.
//! - Keep stable GTK row widgets per device (`DeviceRowWidgets`) and update only the
//!   properties that changed (switch active/sensitive, “Connecting…” label, etc.).
//! - Device order is stable (no resorting on connect/disconnect); rows are created once
//!   and updated in place.
//! - This prevents feedback loops, unnecessary repaints, and “plugin can hang the app” bugs.
//!
//! Transient device status:
//! - When the user toggles a device switch, we immediately mark it as pending
//!   (Connecting/Disconnecting) and disable the switch.
//! - The source of truth is BlueZ, not our app:
//!   - When BlueZ reports the desired Connected state, we clear pending.
//!   - If the DBus call fails immediately, we KEEP pending and force a refresh to reconcile.
//!
//! Adapter powered-off policy:
//! - When the adapter is powered off, device switches are forced OFF and disabled.
//!
//! Transitioning UI:
//! - While a device is Connecting/Disconnecting, we visually indicate that state in the row
//!   (e.g. a dim "Connecting…" / "Disconnecting…" label next to the device name) and keep the
//!   switch disabled until the pending state clears.
//!
//! Immediate menu repaint on DBus events (wake-on-demand; no polling):
//! - DBus tasks MUST NOT touch GTK directly.
//! - DBus tasks enqueue "adapter X needs repaint" into a Send-safe queue.
//! - The *GTK thread* schedules a single drain+repaint callback (coalescing bursts) and
//!   repaints only the affected adapter menus by updating existing row widgets.
//! - Additionally, the DBus task emits a UI-layer repaint request event so the GTK event pump
//!   can trigger `BluetoothPlugin::request_menu_rerender()` without polling.
//!
//! Overlay visibility integration:
//! - Repaints should only be scheduled while the overlay is visible.
//! - When the overlay transitions hidden → visible, force a drain+repaint so the user never
//!   sees stale Bluetooth state when opening the overlay.

use anyhow::{Context, Result};
use async_trait::async_trait;
use gtk::glib;
use gtk::prelude::*;
use zbus::Connection;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::{Mutex as TokioMutex, mpsc};

use crate::dbus::DbusHandle;
use crate::plugins::{FeatureToggle, Plugin, Widget};
use crate::ui::UiEvent;
use crate::ui::features::{FeatureSpec, MenuSpec};

/// BlueZ well-known name and interfaces.
const BLUEZ_DEST: &str = "org.bluez";
const IFACE_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const IFACE_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const IFACE_ADAPTER1: &str = "org.bluez.Adapter1";
const IFACE_DEVICE1: &str = "org.bluez.Device1";

/// A stable-ish key format for feature tiles.
///
/// NOTE: `FeatureSpec.key` is `&'static str`, so we must produce stable leaked strings.
fn feature_key_for_adapter(adapter_path: &str) -> &'static str {
    // Keep it readable and unique.
    // Example: "bluetooth:adapter:/org/bluez/hci0"
    Box::leak(format!("bluetooth:adapter:{adapter_path}").into_boxed_str())
}

#[derive(Clone, Debug)]
struct AdapterState {
    path: String,
    name: String,
    powered: bool,
}

#[derive(Clone, Debug)]
struct DeviceState {
    path: String,
    adapter_path: String,
    name: String,
    icon: String,
    paired: bool,
    connected: bool,
    /// Local, transient state while we are awaiting BlueZ updates.
    pending: PendingState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingState {
    None,
    Connecting,
    Disconnecting,
}

impl PendingState {
    fn is_pending(self) -> bool {
        self != PendingState::None
    }
}

#[derive(Clone, Default)]
struct BluezCache {
    adapters: HashMap<String, AdapterState>, // path -> state
    devices: HashMap<String, DeviceState>,   // path -> state
}

impl BluezCache {
    fn devices_for_adapter_paired(&self, adapter_path: &str) -> Vec<DeviceState> {
        let mut out: Vec<DeviceState> = self
            .devices
            .values()
            .filter(|d| d.adapter_path == adapter_path && d.paired)
            .cloned()
            .collect();

        // Sort: connected first, then name.
        out.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        out
    }
}

#[derive(Clone)]
struct UiHandles {
    /// Model sink for tile active/status updates.
    features_model: Option<crate::ui::features::FeaturesModel>,
}

#[derive(Clone)]
struct DeviceRowWidgets {
    /// Row root widget (so we can remove it from the list when needed).
    row: gtk::Box,
    /// Switch reflecting connected state.
    sw: gtk::Switch,
    /// Secondary status label for transient state ("Connecting…"/"Disconnecting…").
    status_lbl: gtk::Label,
}

#[derive(Clone)]
struct AdapterUi {
    /// Root widget for the details panel.
    menu_root: gtk::Box,
    /// Container holding device rows.
    devices_box: gtk::Box,
    /// We might update the title row if adapter naming changes.
    heading_label: gtk::Label,

    /// Stable row registry: device object path -> widgets to update in-place.
    rows: Rc<RefCell<HashMap<String, DeviceRowWidgets>>>,

    /// Stable order of devices as first seen (paired devices only).
    order: Rc<RefCell<Vec<String>>>,
}

#[derive(Default)]
struct BluetoothDbusState {
    cache: BluezCache,

    /// If multiple adapters exist, we include adapter name in tile title.
    multiple_adapters: bool,
}

/// Main-thread-only UI state (GTK widgets live here).
///
/// This must never be accessed from Tokio tasks.
#[derive(Default)]
struct BluetoothUiState {
    /// One menu UI per adapter path.
    adapter_ui: HashMap<String, AdapterUi>,

    /// Cache of last-rendered adapter list size to decide title format.
    multiple_adapters: bool,
}

#[derive(Clone)]
struct BluetoothUiRerender {
    ui_state: Rc<RefCell<BluetoothUiState>>,
    dbus_state: Arc<TokioMutex<BluetoothDbusState>>,

    // Wake-on-demand repaint queue (Send-safe so DBus tasks can enqueue).
    repaint_q: Arc<StdMutex<VecDeque<String>>>,

    // Set to true only by the GTK thread when it actually schedules a drain callback.
    repaint_scheduled: Arc<AtomicBool>,
}

impl BluetoothUiRerender {
    fn request_render_all(&self) {
        // Convenience: repaint all currently known adapter menus (GTK thread).
        let ui_state = self.ui_state.clone();
        let dbus_state = self.dbus_state.clone();

        glib::MainContext::default().invoke_local(move || {
            if let Ok(st) = dbus_state.try_lock() {
                let ui = ui_state.borrow_mut();
                for (adapter_path, adapter_ui) in ui.adapter_ui.iter() {
                    render_devices_into(adapter_ui, &st.cache, adapter_path);
                }
            }
        });
    }

    fn enqueue_repaint_adapter(&self, adapter_path: String) {
        // This method is safe to call from DBus tasks (no GTK touched here).
        {
            let mut q = self.repaint_q.lock().unwrap();
            q.push_back(adapter_path);
        }

        // IMPORTANT:
        // Scheduling (`invoke_local`) must be initiated from the GTK thread.
        // This helper is main-thread-only; DBus tasks should only enqueue.
        self.maybe_schedule_drain_on_gtk();
    }

    fn maybe_schedule_drain_on_gtk(&self) {
        // Only schedule once per burst (coalesce).
        if self
            .repaint_scheduled
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let ui_state = self.ui_state.clone();
        let dbus_state = self.dbus_state.clone();
        let repaint_q = self.repaint_q.clone();
        let repaint_scheduled = self.repaint_scheduled.clone();

        glib::MainContext::default().invoke_local(move || {
            // Drain queue (coalesce) on GTK thread.
            let mut adapters = HashSet::<String>::new();
            {
                let mut q = repaint_q.lock().unwrap();
                while let Some(p) = q.pop_front() {
                    adapters.insert(p);
                }
            }

            // Apply latest DBus snapshot to affected menus.
            if let Ok(st) = dbus_state.try_lock() {
                let ui = ui_state.borrow_mut();
                for adapter_path in adapters {
                    if let Some(adapter_ui) = ui.adapter_ui.get(&adapter_path) {
                        render_devices_into(adapter_ui, &st.cache, &adapter_path);
                    }
                }
            }

            // Allow next burst to schedule again.
            repaint_scheduled.store(false, Ordering::SeqCst);

            // If new invalidations arrived while we were draining/repainting, schedule again.
            // This is still burst-coalesced because `repaint_scheduled` is now false.
            if let Ok(q) = repaint_q.lock() {
                if !q.is_empty() {
                    // Re-schedule by flipping scheduled and invoking another drain.
                    if repaint_scheduled
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        let ui_state = ui_state.clone();
                        let dbus_state = dbus_state.clone();
                        let repaint_q = repaint_q.clone();
                        let repaint_scheduled = repaint_scheduled.clone();

                        glib::MainContext::default().invoke_local(move || {
                            let mut adapters = HashSet::<String>::new();
                            {
                                let mut q = repaint_q.lock().unwrap();
                                while let Some(p) = q.pop_front() {
                                    adapters.insert(p);
                                }
                            }

                            if let Ok(st) = dbus_state.try_lock() {
                                let ui = ui_state.borrow_mut();
                                for adapter_path in adapters {
                                    if let Some(adapter_ui) = ui.adapter_ui.get(&adapter_path) {
                                        render_devices_into(adapter_ui, &st.cache, &adapter_path);
                                    }
                                }
                            }

                            repaint_scheduled.store(false, Ordering::SeqCst);
                        });
                    }
                }
            }
        });
    }
}

pub struct BluetoothPlugin {
    dbus: Arc<DbusHandle>,

    /// Central UI event sender (optional in this app today).
    ui_event_tx: Option<mpsc::UnboundedSender<UiEvent>>,

    /// Send-safe DBus/cache state used by async tasks.
    dbus_state: Arc<TokioMutex<BluetoothDbusState>>,

    /// GTK UI state (main-thread only).
    ui_state: Rc<RefCell<BluetoothUiState>>,

    /// Main-thread-only UI handle for immediate menu re-render.
    ///
    /// IMPORTANT: this must never be moved into `tokio::spawn` tasks (not `Send`).
    ///
    /// `feature_toggles()` takes `&self`, so this needs interior mutability.
    ui_rerender: RefCell<Option<BluetoothUiRerender>>,

    /// Wake-on-demand repaint queue shared with DBus tasks (Send-safe).
    repaint_q: Arc<StdMutex<VecDeque<String>>>,
    repaint_scheduled: Arc<AtomicBool>,

    /// Simple flag to avoid repeatedly re-installing the rerender helper.
    ui_rerender_installed: Cell<bool>,

    /// Whether overlay is currently visible. Used to avoid scheduling GTK work while hidden.
    ///
    /// NOTE: This is best-effort; it must only be mutated from the GTK thread.
    overlay_visible: Cell<bool>,

    initialized: bool,
}

impl BluetoothPlugin {
    /// Construct a new Bluetooth plugin using the system-bus DBus handle.
    pub fn new(dbus: Arc<DbusHandle>) -> Self {
        let ui_state: Rc<RefCell<BluetoothUiState>> =
            Rc::new(RefCell::new(BluetoothUiState::default()));

        let repaint_q = Arc::new(StdMutex::new(VecDeque::new()));
        let repaint_scheduled = Arc::new(AtomicBool::new(false));

        Self {
            dbus,
            ui_event_tx: None,
            dbus_state: Arc::new(TokioMutex::new(BluetoothDbusState::default())),
            ui_state,
            ui_rerender: RefCell::new(None),
            repaint_q,
            repaint_scheduled,
            ui_rerender_installed: Cell::new(false),
            overlay_visible: Cell::new(false),
            initialized: false,
        }
    }

    /// Best-effort request to repaint Bluetooth submenu widgets.
    ///
    /// Call this only from the GTK thread.
    /// This will not rebuild feature toggles; it only applies the latest cached DBus snapshot
    /// to the already-created menu widgets.
    ///
    /// Repaint scheduling is gated by overlay visibility to avoid doing GTK work while hidden.
    pub fn request_menu_rerender(&self) {
        if !self.overlay_visible.get() {
            return;
        }
        if let Some(r) = self.ui_rerender.borrow().as_ref() {
            r.maybe_schedule_drain_on_gtk();
        }
    }

    /// Notify the plugin that the overlay became visible.
    ///
    /// Call this from the GTK thread at the point where the overlay is shown.
    /// This forces a drain+repaint so the menu is not stale when opened.
    ///
    /// Additionally, schedule an idle-time drain so that if DBus invalidations arrived
    /// while the overlay was hidden, we repaint as soon as the main loop is free.
    pub fn on_overlay_shown(&self) {
        self.overlay_visible.set(true);

        // Force an immediate drain/repaint (best-effort).
        self.request_menu_rerender();

        // Schedule an idle-time repaint without using unsafe pointers.
        //
        // We can't capture `&self` into a `'static` closure, so we schedule the idle drain
        // through the already-installed repaint helper (which is `Rc`-backed and main-thread-only).
        //
        // If the helper isn't installed yet (menu widgets not created yet), this is a no-op and
        // the first menu build will render fresh state anyway.
        if let Some(r) = self.ui_rerender.borrow().as_ref().cloned() {
            glib::MainContext::default().invoke_local(move || {
                r.maybe_schedule_drain_on_gtk();
            });
        }
    }

    /// Notify the plugin that the overlay was hidden.
    ///
    /// Call this from the GTK thread at the point where the overlay is hidden.
    pub fn on_overlay_hidden(&self) {
        self.overlay_visible.set(false);
    }

    pub fn with_ui_event_tx(mut self, tx: mpsc::UnboundedSender<UiEvent>) -> Self {
        self.ui_event_tx = Some(tx);
        self
    }

    fn emit(&self, ev: UiEvent) {
        if let Some(tx) = &self.ui_event_tx {
            let _ = tx.send(ev);
        }
    }

    fn schedule_on_main<F: FnOnce() + 'static>(f: F) {
        glib::MainContext::default().spawn_local(async move {
            f();
        });
    }

    async fn refresh_device_from_object_manager(
        dbus: &DbusHandle,
        dbus_state: &Arc<TokioMutex<BluetoothDbusState>>,
        device_path: &str,
    ) -> Result<()> {
        // Conservative but robust: refresh full snapshot.
        // (We can optimize to per-device reads later if needed.)
        let _ = device_path;
        BluetoothPlugin::refresh_all_from_object_manager(dbus, dbus_state).await?;
        Ok(())
    }

    async fn set_device_pending(
        dbus_state: &Arc<TokioMutex<BluetoothDbusState>>,
        device_path: &str,
        pending: PendingState,
    ) {
        let mut st = dbus_state.lock().await;
        if let Some(d) = st.cache.devices.get_mut(device_path) {
            d.pending = pending;
        }
    }

    async fn clear_device_pending_if_reconciled(
        dbus_state: &Arc<TokioMutex<BluetoothDbusState>>,
        device_path: &str,
        desired_connected: bool,
    ) {
        let mut st = dbus_state.lock().await;
        if let Some(d) = st.cache.devices.get_mut(device_path) {
            // If we've reached the desired state, clear pending.
            if d.connected == desired_connected {
                d.pending = PendingState::None;
            }
        }
    }

    fn adapter_tile_title(multiple: bool, adapter_name: &str) -> String {
        if multiple {
            format!("Bluetooth - {adapter_name}")
        } else {
            "Bluetooth".to_string()
        }
    }

    fn adapter_status_text(powered: bool) -> String {
        if powered {
            "On".to_string()
        } else {
            "Off".to_string()
        }
    }

    fn device_icon_heuristic(bluez_icon: Option<&str>) -> String {
        // BlueZ `Icon` values can be things like: "phone", "audio-card", "input-keyboard", ...
        // We map a few common ones to icon theme names (symbolic preferred).
        match bluez_icon.unwrap_or_default() {
            "phone" => "phone-symbolic".to_string(),
            "computer" => "computer-symbolic".to_string(),
            "audio-card" => "audio-card-symbolic".to_string(),
            "input-keyboard" => "input-keyboard-symbolic".to_string(),
            "input-mouse" => "input-mouse-symbolic".to_string(),
            "camera-video" => "camera-video-symbolic".to_string(),
            "headset" => "audio-headset-symbolic".to_string(),
            "headphones" => "audio-headphones-symbolic".to_string(),
            "printer" => "printer-symbolic".to_string(),
            _ => "bluetooth-symbolic".to_string(),
        }
    }

    fn build_adapter_menu_widget(_adapter_name: &str) -> AdapterUi {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();

        // Heading: icon + label
        let heading_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .build();

        let icon = gtk::Image::from_icon_name("bluetooth-symbolic");
        icon.set_pixel_size(20);

        let heading_label = gtk::Label::builder()
            .label("Bluetooth")
            .xalign(0.0)
            .css_classes(["title-4"])
            .build();

        heading_row.append(&icon);
        heading_row.append(&heading_label);
        root.append(&heading_row);

        // Devices list container
        let devices_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();
        root.append(&devices_box);

        // Separator + settings button
        let sep = gtk::Separator::builder()
            .orientation(gtk::Orientation::Horizontal)
            .margin_top(4)
            .build();
        root.append(&sep);

        let settings_btn = gtk::Button::builder()
            .label("Bluetooth Settings")
            .css_classes(["pill"])
            .halign(gtk::Align::Start)
            .build();
        settings_btn.connect_clicked(|_| {
            println!("Not implemented");
        });
        root.append(&settings_btn);

        AdapterUi {
            menu_root: root,
            devices_box,
            heading_label,
            rows: Rc::new(RefCell::new(HashMap::new())),
            order: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn ensure_device_row(
        adapter_ui: &AdapterUi,
        device: &DeviceState,
        on_toggle: impl Fn(String, bool) + 'static,
    ) {
        // Create the row once and keep it stable in the registry.
        if adapter_ui.rows.borrow().contains_key(&device.path) {
            return;
        }

        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["qs-row"])
            .build();

        let icon = gtk::Image::from_icon_name(&device.icon);
        icon.set_pixel_size(18);

        let text_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .build();

        let name_lbl = gtk::Label::builder()
            .label(device.name.clone())
            .xalign(0.0)
            .hexpand(true)
            .build();

        let status_lbl = gtk::Label::builder()
            .label("")
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        status_lbl.set_visible(false);

        text_box.append(&name_lbl);
        text_box.append(&status_lbl);

        let sw = gtk::Switch::builder().active(false).build();

        let device_path = device.path.clone();
        sw.connect_state_set(move |_sw, desired| {
            on_toggle(device_path.clone(), desired);
            glib::Propagation::Stop
        });

        row.append(&icon);
        row.append(&text_box);
        row.append(&sw);

        adapter_ui.rows.borrow_mut().insert(
            device.path.clone(),
            DeviceRowWidgets {
                row: row.clone(),
                sw,
                status_lbl,
            },
        );

        adapter_ui.order.borrow_mut().push(device.path.clone());
        adapter_ui.devices_box.append(&row);
    }

    fn apply_device_row_state(adapter_ui: &AdapterUi, device: &DeviceState, adapter_powered: bool) {
        let Some(w) = adapter_ui.rows.borrow().get(&device.path).cloned() else {
            return;
        };

        // Adapter off policy: force switch OFF and disable.
        let effective_connected = if adapter_powered {
            device.connected
        } else {
            false
        };

        // Pending text.
        let transitioning = match device.pending {
            PendingState::Connecting => Some("Connecting…"),
            PendingState::Disconnecting => Some("Disconnecting…"),
            PendingState::None => None,
        };

        w.status_lbl.set_label(transitioning.unwrap_or(""));
        w.status_lbl.set_visible(transitioning.is_some());

        // Apply switch state without emitting state_set (set_active is programmatic).
        w.sw.set_active(effective_connected);

        // Disable if adapter off, or while pending.
        w.sw.set_sensitive(adapter_powered && !device.pending.is_pending());
    }

    async fn refresh_all_from_object_manager(
        dbus: &DbusHandle,
        dbus_state: &Arc<TokioMutex<BluetoothDbusState>>,
    ) -> Result<()> {
        // Call ObjectManager.GetManagedObjects on /.
        // BlueZ exposes it at "/" with org.freedesktop.DBus.ObjectManager.
        let conn: Arc<Connection> = dbus.connection();
        let proxy = zbus::Proxy::new(&*conn, BLUEZ_DEST, "/", IFACE_OBJECT_MANAGER)
            .await
            .context("Failed to create BlueZ ObjectManager proxy")?;

        // GetManagedObjects() -> a{oa{sa{sv}}}
        // We'll deserialize into a generic map.
        let (objects,): (
            std::collections::HashMap<
                zvariant::OwnedObjectPath,
                std::collections::HashMap<
                    String,
                    std::collections::HashMap<String, zvariant::OwnedValue>,
                >,
            >,
        ) = proxy
            .call("GetManagedObjects", &())
            .await
            .context("BlueZ GetManagedObjects failed")?;

        let mut next_cache = BluezCache::default();

        for (path, ifaces) in objects {
            let path_str = path.to_string();

            if let Some(props) = ifaces.get(IFACE_ADAPTER1) {
                let name = props
                    .get("Name")
                    .and_then(|v| owned_value_to_string(v.clone()))
                    .unwrap_or_else(|| adapter_name_from_path(&path_str));

                let powered = props
                    .get("Powered")
                    .and_then(|v| owned_value_to_bool(v.clone()))
                    .unwrap_or(false);

                next_cache.adapters.insert(
                    path_str.clone(),
                    AdapterState {
                        path: path_str.clone(),
                        name,
                        powered,
                    },
                );
            }

            if let Some(props) = ifaces.get(IFACE_DEVICE1) {
                // We only care about paired devices (your requirement).
                let paired = props
                    .get("Paired")
                    .and_then(|v| owned_value_to_bool(v.clone()))
                    .unwrap_or(false);

                // Adapter path
                let adapter_path = props
                    .get("Adapter")
                    .and_then(|v| owned_value_to_object_path_string(v.clone()))
                    .unwrap_or_default();

                let name = props
                    .get("Alias")
                    .and_then(|v| owned_value_to_string(v.clone()))
                    .or_else(|| {
                        props
                            .get("Name")
                            .and_then(|v| owned_value_to_string(v.clone()))
                    })
                    .unwrap_or_else(|| "Unknown".to_string());

                let connected = props
                    .get("Connected")
                    .and_then(|v| owned_value_to_bool(v.clone()))
                    .unwrap_or(false);

                let bluez_icon = props
                    .get("Icon")
                    .and_then(|v| owned_value_to_string(v.clone()));

                let icon = Self::device_icon_heuristic(bluez_icon.as_deref());

                // Pending state is local; preserve if already present.
                let pending = {
                    let st = dbus_state.lock().await;
                    st.cache
                        .devices
                        .get(&path_str)
                        .map(|d| d.pending)
                        .unwrap_or(PendingState::None)
                };

                next_cache.devices.insert(
                    path_str.clone(),
                    DeviceState {
                        path: path_str.clone(),
                        adapter_path,
                        name,
                        icon,
                        paired,
                        connected,
                        pending,
                    },
                );
            }
        }

        let mut st = dbus_state.lock().await;
        st.cache = next_cache;
        st.multiple_adapters = st.cache.adapters.len() > 1;
        Ok(())
    }

    async fn install_signal_subscriptions(
        &self,
        dbus_state: Arc<TokioMutex<BluetoothDbusState>>,
        ui_event_tx: Option<mpsc::UnboundedSender<UiEvent>>,
    ) -> Result<()> {
        // ObjectManager signals for dynamic add/remove.
        let mut rx_added = self
            .dbus
            .listen_signals(
                "type='signal',sender='org.bluez',path='/',interface='org.freedesktop.DBus.ObjectManager',member='InterfacesAdded'",
            )
            .await
            .context("Failed to listen for BlueZ InterfacesAdded")?;

        let mut rx_removed = self
            .dbus
            .listen_signals(
                "type='signal',sender='org.bluez',path='/',interface='org.freedesktop.DBus.ObjectManager',member='InterfacesRemoved'",
            )
            .await
            .context("Failed to listen for BlueZ InterfacesRemoved")?;

        // PropertiesChanged signals (BlueZ emits on object paths; interface org.freedesktop.DBus.Properties).
        let mut rx_props = self
            .dbus
            .listen_signals(
                "type='signal',sender='org.bluez',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'",
            )
            .await
            .context("Failed to listen for BlueZ PropertiesChanged")?;

        let dbus = self.dbus.clone();

        // On any signal, refresh cache and:
        // - update adapter tile state via UiEvents
        // - repaint submenu widgets on the GTK thread (without blocking the main loop)
        //
        // NOTE:
        // This task MUST remain Send-safe, so it cannot capture GTK state, `Rc`, etc.
        // To repaint menus, it performs a best-effort lookup of the installed rerender helper
        // via UiEvents + a GTK-thread callback.
        // NOTE:
        // This task must remain Send-safe and must not touch GTK.
        //
        // Wake-on-demand repaint:
        // - After refreshing cache, enqueue affected adapter(s) into a Send-safe queue.
        // - If a rerender helper has been installed (after GTK init), it will schedule exactly
        //   one GTK callback to drain and repaint those adapters.
        let repaint_q = self.repaint_q.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = rx_added.recv() => {},
                    _ = rx_removed.recv() => {},
                    _ = rx_props.recv() => {},
                }

                if BluetoothPlugin::refresh_all_from_object_manager(&dbus, &dbus_state)
                    .await
                    .is_err()
                {
                    continue;
                }

                // Update adapter tiles (active/status) best-effort.
                if let Some(tx) = &ui_event_tx {
                    let st = dbus_state.lock().await;
                    for adapter in st.cache.adapters.values() {
                        let key = feature_key_for_adapter(&adapter.path).to_string();
                        let _ = tx.send(UiEvent::FeatureActiveChanged {
                            key: key.clone(),
                            active: adapter.powered,
                        });
                        let _ = tx.send(UiEvent::FeatureStatusTextChanged {
                            key,
                            text: BluetoothPlugin::adapter_status_text(adapter.powered),
                        });
                    }

                    // Ask the UI layer to trigger a repaint pass (no polling).
                    // The UI pump can call `BluetoothPlugin::request_menu_rerender()` from the GTK thread.
                    let _ = tx.send(UiEvent::RepaintRequested {
                        scope: "bluetooth".to_string(),
                    });
                }

                // Enqueue repaint requests (coalesced later on GTK thread).
                // NOTE: do NOT set `repaint_scheduled` here. Only the GTK thread may decide
                // whether a drain callback is scheduled.
                {
                    let st = dbus_state.lock().await;
                    let mut q = repaint_q.lock().unwrap();
                    for adapter_path in st.cache.adapters.keys() {
                        q.push_back(adapter_path.clone());
                    }
                }
            }
        });

        Ok(())
    }

    async fn set_adapter_powered(
        dbus: Arc<DbusHandle>,
        adapter_path: String,
        powered: bool,
    ) -> Result<()> {
        // org.freedesktop.DBus.Properties.Set("org.bluez.Adapter1", "Powered", <bool>)
        let conn: Arc<Connection> = dbus.connection();
        let adapter_path = zvariant::OwnedObjectPath::try_from(adapter_path)
            .context("Invalid adapter object path")?;

        let proxy = zbus::Proxy::new(&*conn, BLUEZ_DEST, adapter_path, IFACE_PROPERTIES)
            .await
            .context("Failed to create Properties proxy for adapter")?;

        let v = zvariant::Value::from(powered);
        let call_res: std::result::Result<(), _> =
            proxy.call("Set", &(IFACE_ADAPTER1, "Powered", v)).await;
        call_res.context("Failed to set adapter Powered")?;
        Ok(())
    }

    async fn device_connect(dbus: Arc<DbusHandle>, device_path: String) -> Result<()> {
        let conn: Arc<Connection> = dbus.connection();
        let device_path = zvariant::OwnedObjectPath::try_from(device_path)
            .context("Invalid device object path")?;

        let proxy = zbus::Proxy::new(&*conn, BLUEZ_DEST, device_path, IFACE_DEVICE1)
            .await
            .context("Failed to create Device1 proxy")?;

        let call_res: std::result::Result<(), _> = proxy.call("Connect", &()).await;
        call_res.context("Device Connect failed")?;
        Ok(())
    }

    async fn device_disconnect(dbus: Arc<DbusHandle>, device_path: String) -> Result<()> {
        let conn: Arc<Connection> = dbus.connection();
        let device_path = zvariant::OwnedObjectPath::try_from(device_path)
            .context("Invalid device object path")?;

        let proxy = zbus::Proxy::new(&*conn, BLUEZ_DEST, device_path, IFACE_DEVICE1)
            .await
            .context("Failed to create Device1 proxy")?;

        let call_res: std::result::Result<(), _> = proxy.call("Disconnect", &()).await;
        call_res.context("Device Disconnect failed")?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl Plugin for BluetoothPlugin {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn name(&self) -> &str {
        "BluetoothPlugin"
    }

    async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        // Initial snapshot (Send-safe).
        BluetoothPlugin::refresh_all_from_object_manager(&self.dbus, &self.dbus_state).await?;

        // IMPORTANT:
        // `initialize()` runs before GTK is initialized in this app, so we must NOT
        // create any GTK widgets here.
        //
        // We only initialize DBus state + subscriptions here. GTK widget construction
        // happens lazily in `feature_toggles()` once the UI is being built.

        // Subscribe for updates (Tokio task; Send-safe only).
        self.install_signal_subscriptions(self.dbus_state.clone(), self.ui_event_tx.clone())
            .await?;

        self.initialized = true;
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    fn feature_toggles(&self) -> Vec<FeatureToggle> {
        // NOTE: This is called synchronously during UI build.
        // We must not block on async locks here; use try_lock and best-effort snapshots.
        let dbus = self.dbus.clone();

        let st_guard = match self.dbus_state.try_lock() {
            Ok(g) => g,
            Err(_) => return vec![],
        };

        // IMPORTANT:
        // Do NOT repaint menus from `feature_toggles()`.
        //
        // `feature_toggles()` is invoked during UI build, and repainting GTK widgets here can
        // easily create a feedback loop (render → events → render) and starve the main loop,
        // leading to high CPU usage and an unresponsive overlay.
        //
        // Menu repaint must be triggered only by DBus-driven tasks scheduling GTK callbacks,
        // never by the synchronous spec generation path.

        // Ensure UI exists for any newly discovered adapters (best-effort, GTK thread).
        {
            let mut ui = self.ui_state.borrow_mut();

            ui.multiple_adapters = st_guard.multiple_adapters;

            // Insert any missing adapter UIs (iterate over a collected list to avoid borrow issues).
            let adapters: Vec<AdapterState> = st_guard.cache.adapters.values().cloned().collect();
            for adapter in adapters {
                if !ui.adapter_ui.contains_key(&adapter.path) {
                    let ui_for_adapter = BluetoothPlugin::build_adapter_menu_widget(&adapter.name);
                    ui.adapter_ui.insert(adapter.path.clone(), ui_for_adapter);
                }
            }

            // Initial render during UI build (so the menu isn't empty).
            for (adapter_path, adapter_ui) in ui.adapter_ui.iter() {
                render_devices_into(adapter_ui, &st_guard.cache, adapter_path);
            }

            // Install the main-thread rerender helper now that GTK is initialized.
            // This is used (indirectly) by the DBus listener task to repaint menus on the GTK thread,
            // without polling and without touching GTK from DBus tasks.
            if !self.ui_rerender_installed.get() {
                *self.ui_rerender.borrow_mut() = Some(BluetoothUiRerender {
                    ui_state: self.ui_state.clone(),
                    dbus_state: self.dbus_state.clone(),
                    repaint_q: self.repaint_q.clone(),
                    repaint_scheduled: self.repaint_scheduled.clone(),
                });
                self.ui_rerender_installed.set(true);
            }
        }

        let multiple = st_guard.multiple_adapters;
        let mut toggles = Vec::new();

        // Build toggles.
        // IMPORTANT: clone what we need per-iteration so we can move it into the closure safely.
        let adapters: Vec<AdapterState> = st_guard.cache.adapters.values().cloned().collect();
        for adapter in adapters {
            let ui = match self.ui_state.borrow().adapter_ui.get(&adapter.path) {
                Some(u) => u.clone(),
                None => continue,
            };

            // Wire interactive device rows for this adapter menu using stable per-device rows.
            // We create rows once and then update their properties on DBus changes (React-ish).
            {
                let adapter_path = adapter.path.clone();
                let adapter_ui = ui.clone();
                let adapter_powered = adapter.powered;
                let cache_snapshot = st_guard.cache.clone();
                let dbus_state = self.dbus_state.clone();
                let dbus_for_actions = dbus.clone();

                // Ensure rows exist for paired devices, and wire per-device callbacks.
                for dev in cache_snapshot.devices_for_adapter_paired(&adapter_path) {
                    let dbus_state_for_row = dbus_state.clone();
                    let dbus_for_row = dbus_for_actions.clone();

                    BluetoothPlugin::ensure_device_row(
                        &adapter_ui,
                        &dev,
                        move |device_path, desired_connected| {
                            // Adapter off policy: disallow actions and keep switches off+disabled.
                            if !adapter_powered {
                                return;
                            }

                            let dbus_state = dbus_state_for_row.clone();
                            let dbus = dbus_for_row.clone();

                            glib::MainContext::default().spawn_local(async move {
                                let pending = if desired_connected {
                                    PendingState::Connecting
                                } else {
                                    PendingState::Disconnecting
                                };
                                BluetoothPlugin::set_device_pending(
                                    &dbus_state,
                                    &device_path,
                                    pending,
                                )
                                .await;

                                let _action_res = if desired_connected {
                                    BluetoothPlugin::device_connect(
                                        dbus.clone(),
                                        device_path.clone(),
                                    )
                                    .await
                                } else {
                                    BluetoothPlugin::device_disconnect(
                                        dbus.clone(),
                                        device_path.clone(),
                                    )
                                    .await
                                };

                                // Always refresh to reconcile; source of truth is BlueZ.
                                let _ = BluetoothPlugin::refresh_device_from_object_manager(
                                    &dbus,
                                    &dbus_state,
                                    &device_path,
                                )
                                .await;

                                BluetoothPlugin::clear_device_pending_if_reconciled(
                                    &dbus_state,
                                    &device_path,
                                    desired_connected,
                                )
                                .await;
                            });
                        },
                    );

                    // Apply current state to row (includes pending + adapter off policy).
                    BluetoothPlugin::apply_device_row_state(&adapter_ui, &dev, adapter_powered);
                }
            }

            let key = feature_key_for_adapter(&adapter.path);
            let title = BluetoothPlugin::adapter_tile_title(multiple, &adapter.name);
            let status_text = BluetoothPlugin::adapter_status_text(adapter.powered);

            let menu = MenuSpec::new(&ui.menu_root);

            let adapter_path = adapter.path.clone();
            let ui_event_tx = self.ui_event_tx.clone();
            let dbus_for_closure = dbus.clone();

            let mut spec = FeatureSpec::contentful(
                key,
                title,
                "bluetooth-symbolic",
                status_text,
                adapter.powered,
                menu,
                false,
            );

            spec.on_toggle = Some(Rc::new(move |key, current_active| {
                let adapter_path = adapter_path.clone();
                let ui_event_tx = ui_event_tx.clone();
                let dbus = dbus_for_closure.clone();

                Box::pin(async move {
                    let desired = !current_active;

                    if let Some(tx) = &ui_event_tx {
                        let _ = tx.send(UiEvent::FeatureStatusTextChanged {
                            key: key.to_string(),
                            text: BluetoothPlugin::adapter_status_text(desired),
                        });
                    }

                    let res =
                        BluetoothPlugin::set_adapter_powered(dbus, adapter_path, desired).await;

                    if res.is_ok() {
                        if let Some(tx) = &ui_event_tx {
                            let _ = tx.send(UiEvent::FeatureActiveChanged {
                                key: key.to_string(),
                                active: desired,
                            });
                            let _ = tx.send(UiEvent::FeatureStatusTextChanged {
                                key: key.to_string(),
                                text: BluetoothPlugin::adapter_status_text(desired),
                            });
                        }
                    }
                })
            }));

            toggles.push(FeatureToggle {
                el: spec,
                weight: 50,
            });
        }

        toggles
    }

    fn widgets(&self) -> Vec<Widget> {
        vec![]
    }
}

// ---------- GTK rendering helpers ----------

fn render_devices_into(ui: &AdapterUi, cache: &BluezCache, adapter_path: &str) {
    let adapter_powered = cache
        .adapters
        .get(adapter_path)
        .map(|a| a.powered)
        .unwrap_or(false);

    // Ensure we have rows for currently paired devices, but DO NOT rebuild everything.
    // Device order is stable (first seen).
    let mut seen_now: HashMap<String, DeviceState> = HashMap::new();
    for dev in cache.devices_for_adapter_paired(adapter_path) {
        seen_now.insert(dev.path.clone(), dev);
    }

    // Create missing rows in stable order (append at end).
    for dev in seen_now.values() {
        BluetoothPlugin::ensure_device_row(ui, dev, |_device_path, _desired| {});
    }

    // Apply state updates for all known rows (including adapter off policy).
    for dev in seen_now.values() {
        BluetoothPlugin::apply_device_row_state(ui, dev, adapter_powered);
    }

    // Remove rows that are no longer paired/visible.
    // (Keep order stable for remaining rows.)
    {
        let mut rows = ui.rows.borrow_mut();
        let mut order = ui.order.borrow_mut();
        order.retain(|path| {
            if seen_now.contains_key(path) {
                true
            } else {
                if let Some(w) = rows.remove(path) {
                    ui.devices_box.remove(&w.row);
                }
                false
            }
        });
    }

    // Empty state: if there are no rows, show a label.
    if ui.order.borrow().is_empty() {
        // Ensure an empty label exists as the only child.
        while let Some(child) = ui.devices_box.first_child() {
            ui.devices_box.remove(&child);
        }
        let empty = gtk::Label::builder()
            .label("No paired devices")
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        ui.devices_box.append(&empty);
    }
}

// ---------- zvariant decoding helpers ----------

fn owned_value_to_string(v: zvariant::OwnedValue) -> Option<String> {
    let val: zvariant::Value = v.into();
    match val {
        zvariant::Value::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

fn owned_value_to_bool(v: zvariant::OwnedValue) -> Option<bool> {
    let val: zvariant::Value = v.into();
    match val {
        zvariant::Value::Bool(b) => Some(b),
        _ => None,
    }
}

fn owned_value_to_object_path_string(v: zvariant::OwnedValue) -> Option<String> {
    let val: zvariant::Value = v.into();
    match val {
        zvariant::Value::ObjectPath(p) => Some(p.to_string()),
        _ => None,
    }
}

fn adapter_name_from_path(path: &str) -> String {
    // /org/bluez/hci0 -> hci0
    path.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("adapter")
        .to_string()
}
