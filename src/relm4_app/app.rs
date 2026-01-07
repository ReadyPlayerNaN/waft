//! Relm4 + libadwaita overlay host (migration step 05).
//!
//! NOTE (post-step): this component now exposes explicit show/hide/toggle inputs so an
//! external IPC server can request overlay visibility changes on the GTK thread.
//!
//! IPC behavior (requested):
//! - `sacrebleui` (no args) starts the UI and attempts to become the IPC server.
//!   If an instance is already running, it exits with a non-zero exit code.
//! - `sacrebleui toggle|show|hide|stop` acts as an IPC client: sends the command to the
//!   running instance and exits.
//!   - `stop` requests the running instance to terminate.
//!
//! Goals for this step:
//! - Build a real overlay window layout with Top / Left / Right placement areas.
//! - Mount plugin components (stubs are fine) into the correct slot bucket,
//!   preserving ordering: heavier weight goes lower.
//! - Wire overlay shown/hidden into the central router messages:
//!   - AppMsg::OverlayShown
//!   - AppMsg::OverlayHidden
//! - Add a small, testable “wiring layer” that executes RouterEffect using typed plugin handles.
//!
//! Guardrails (per `AGENTS.md`):
//! - Do not create GTK widgets before GTK is initialized. Relm4 constructs widgets during `run()`.
//! - Keep reducer/router logic GTK-free and unit-testable.
//! - Never touch GTK from background tasks/threads; schedule onto the GTK main context instead.
//! - Do not poll / run main loops in tests.
//!
//! IPC → UI routing note:
//! - Relm4 `Sender<T>` uses `emit(T)` / `send(T)` (there is no `input(...)` method).

use relm4::adw;
use relm4::gtk;

// Extension traits required for `adw::ApplicationWindow::set_content(...)` and GTK window props.
use gtk::prelude::{FrameExt, GtkWindowExt, WidgetExt};

// Needed for the IPC-enabled entrypoint:
// - `connect_startup` comes from `gio::ApplicationExt` (re-exported by gtk prelude)
// - `run()` comes from `gio::ApplicationExtManual` (also re-exported by gtk prelude)
// - `add_window` comes from `GtkApplicationExt`
use gtk::prelude::{ApplicationExt, ApplicationExtManual, GtkApplicationExt};

use relm4::adw::prelude::AdwApplicationWindowExt;
use relm4::gtk::prelude::{BoxExt, Cast};

use gtk4_layer_shell::LayerShell;

use relm4::prelude::*;

use crate::relm4_app::events::AppMsg;
use crate::relm4_app::plugin_framework::{
    PluginInitContext, PluginMountContext, PluginPlacement, PluginSpec, RelmPlugin, Slot,
};
use crate::relm4_app::plugin_registry::RelmPluginRegistry;
use crate::relm4_app::router::{RouterEffect, RouterState, reduce_router};

// IPC (kept in a separate module/file; this component only consumes commands).
use crate::ipc::net as ipc_net;
use crate::ipc::{IpcCommand, command_from_args, ipc_socket_path};

use std::sync::{Arc, Mutex};
use std::thread;

/// Notifications plugin spec placeholder for toast gating wiring.
///
/// Decision (confirmed): toast gating belongs to the notifications plugin.
///
/// NOTE:
/// This repo still contains the legacy GTK notifications plugin under
/// `src/features/notifications/`, but the Relm4 plugin registry is a new system.
/// Step 05 wires the router effect to a typed plugin input endpoint.
/// The actual notifications Relm4 component/spec will be introduced in later steps.
///
/// For now, this spec is satisfied by a local stub plugin registered by the overlay host.
pub struct NotificationsSpec;

const OVERLAY_WIDTH_PX: i32 = 920;
const OVERLAY_TOP_OFFSET_PX: i32 = 16;
const OVERLAY_BOTTOM_OFFSET_PX: i32 = 16;
const OVERLAY_CORNER_RADIUS_PX: i32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationsInput {
    SetToastGating { enabled: bool },
}

impl PluginSpec for NotificationsSpec {
    type Input = NotificationsInput;

    fn id() -> crate::relm4_app::events::PluginId {
        "plugin.notifications".into()
    }

    fn name() -> &'static str {
        "Notifications (stub)"
    }

    fn placement() -> PluginPlacement {
        // Stub: place a visible box in the left column for now.
        PluginPlacement::new(Slot::Left, 50)
    }
}

/// Pure, testable wiring layer: interpret router effects and notify plugins via typed handles.
///
/// Important:
/// - This must stay GTK-free so it can be unit-tested without initializing GTK.
/// - Missing plugins are normal (config-driven / migration-in-progress): do not panic.
pub fn execute_router_effects(registry: &RelmPluginRegistry, effects: &[RouterEffect]) {
    for eff in effects {
        match eff {
            RouterEffect::SetToastGating { enabled } => {
                if let Some(handle) = registry.get::<NotificationsSpec>() {
                    // Typed send: compile-time checked against `NotificationsInput`.
                    let _ = handle.send(&NotificationsInput::SetToastGating { enabled: *enabled });
                }
            }
            RouterEffect::InvalidateToastLayout => {
                // Not used yet in step 05; keep as a stable placeholder.
            }
        }
    }
}

/// Small stub plugin that produces a visible widget and a typed input endpoint.
/// This is used so you can immediately verify slot/weight placement and wiring.
struct NotificationsStubPlugin {
    last_gating_enabled: std::sync::Arc<std::sync::Mutex<Option<bool>>>,
}

impl NotificationsStubPlugin {
    fn new() -> Self {
        Self {
            last_gating_enabled: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl RelmPlugin for NotificationsStubPlugin {
    fn id(&self) -> crate::relm4_app::events::PluginId {
        NotificationsSpec::id()
    }

    fn name(&self) -> &'static str {
        NotificationsSpec::name()
    }

    fn placement(&self) -> PluginPlacement {
        NotificationsSpec::placement()
    }

    fn init(
        &mut self,
        _ctx: PluginInitContext,
    ) -> Result<(), crate::relm4_app::plugin_framework::PluginInitError> {
        // MUST remain GTK-free.
        Ok(())
    }

    fn mount(
        &mut self,
        _ctx: PluginMountContext,
    ) -> Result<
        crate::relm4_app::plugin_framework::MountedPlugin,
        crate::relm4_app::plugin_framework::PluginMountError,
    > {
        // NOTE: Step 04 registry mounting currently stores endpoints and metadata.
        // Step 05 will mount actual components into the window by reading metadata and constructing widgets.
        //
        // We implement a typed endpoint that records the last toast gating value for manual sanity checks.
        use crate::relm4_app::plugin_framework::{
            MountedPlugin, MountedPluginMeta, PluginEndpoint, PluginRouteError,
        };

        struct CaptureEndpoint {
            plugin: crate::relm4_app::events::PluginId,
            shared: std::sync::Arc<std::sync::Mutex<Option<bool>>>,
        }

        impl PluginEndpoint for CaptureEndpoint {
            fn plugin_id(&self) -> crate::relm4_app::events::PluginId {
                self.plugin.clone()
            }

            fn input_type_id(&self) -> std::any::TypeId {
                std::any::TypeId::of::<NotificationsInput>()
            }

            fn send_any(&self, msg: &dyn std::any::Any) -> Result<(), PluginRouteError> {
                let m = msg.downcast_ref::<NotificationsInput>().ok_or(
                    PluginRouteError::WrongMsgType {
                        plugin: self.plugin.clone(),
                        expected: std::any::type_name::<NotificationsInput>(),
                        got: "unknown",
                    },
                )?;

                match m {
                    NotificationsInput::SetToastGating { enabled } => {
                        *self.shared.lock().unwrap() = Some(*enabled);
                    }
                }

                Ok(())
            }
        }

        let id = self.id();
        Ok(MountedPlugin {
            meta: MountedPluginMeta {
                id: id.clone(),
                name: self.name(),
                placement: self.placement(),
            },
            endpoint: Box::new(CaptureEndpoint {
                plugin: id,
                shared: self.last_gating_enabled.clone(),
            }),
        })
    }
}

/// A lightweight “mounted widget” descriptor used by the overlay host.
///
/// In this step we mount stub widgets; later steps will mount real Relm4 components.
#[derive(Debug, Clone)]
struct MountedWidget {
    plugin_id: String,
    slot: Slot,
    weight: i32,
    widget: gtk::Widget,
}

impl MountedWidget {
    fn new(plugin_id: String, slot: Slot, weight: i32, widget: gtk::Widget) -> Self {
        Self {
            plugin_id,
            slot,
            weight,
            widget,
        }
    }
}

/// Build a visible stub widget for a plugin placement.
///
/// This is intentionally simple and self-identifying, so you can verify placement quickly.
fn build_plugin_stub_widget(plugin_id: &str, slot: Slot, weight: i32) -> gtk::Widget {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
    root.set_margin_top(8);
    root.set_margin_bottom(8);
    root.set_margin_start(8);
    root.set_margin_end(8);
    root.add_css_class("card");

    let title = gtk::Label::new(Some(&format!("Plugin: {}", plugin_id)));
    title.set_xalign(0.0);
    title.add_css_class("title-4");

    let meta = gtk::Label::new(Some(&format!("slot={:?} weight={}", slot, weight)));
    meta.set_xalign(0.0);
    meta.add_css_class("dim-label");

    root.append(&title);
    root.append(&meta);

    root.upcast::<gtk::Widget>()
}

#[derive(Debug)]
struct AppModel {
    window: adw::ApplicationWindow,

    registry: RelmPluginRegistry,
    router_state: RouterState,

    // Layout containers. IMPORTANT: these are the actual widgets mounted into the UI tree.
    // We must not clone and re-parent GTK widgets (it triggers `gtk_box_append` assertions).
    top_box: gtk::Box,
    left_col: gtk::Box,
    right_col: gtk::Box,

    // Keep mounted widgets stable (mounted once).
    mounted_widgets: Vec<MountedWidget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppInput {
    Router(AppMsg),

    // Internal: triggered once during init to mount plugin widgets.
    MountPlugins,

    // Internal: relay window visibility changes.
    WindowMapped,
    WindowUnmapped,

    // External/public-ish: allow an IPC layer (or future global hotkey handler) to control
    // overlay visibility on the GTK thread via Relm4 message passing.
    ShowOverlay,
    HideOverlay,
    ToggleOverlay,

    // External/public-ish: request app termination (used by IPC `stop` command).
    StopApp,

    // Internal: request hiding the overlay (Escape / unfocus).
    RequestHide,
}

#[relm4::component]
impl SimpleComponent for AppModel {
    type Init = ();
    type Input = AppInput;
    type Output = ();

    view! {
        root = adw::ApplicationWindow {
            // This is a layer-shell overlay surface (Wayland-only by project design).
            // We keep a title for debugging, but it is not shown when undecorated.
            set_title: Some("sacrebleui (Relm4 overlay host)"),

            // Fixed width per requirement; height is content-driven.
            // The maximum height constraint is handled via layer-shell margins + a scrolled window.
            set_default_width: OVERLAY_WIDTH_PX,

            // We connect layer-shell + visibility signals in `init()` where we have access to `sender`.
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Configure window as an overlay-layer surface using layer-shell.
        //
        // Requirements:
        // - overlay layer
        // - horizontally centered
        // - vertically anchored to top with 16px offset
        // - content-driven height, but never exceeding display height minus (top+bottom offsets)
        // - focusable (keyboard mode on-demand)
        //
        // NOTE: gtk4-layer-shell is Wayland-only by project design.
        root.set_decorated(false);
        root.set_hide_on_close(true);
        root.set_modal(false);

        root.init_layer_shell();
        root.set_layer(gtk4_layer_shell::Layer::Overlay);
        root.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);

        // Anchor to top; do not anchor left/right to avoid compositor "stretch-to-left" behavior.
        // We rely on fixed width + auto centering.
        root.set_anchor(gtk4_layer_shell::Edge::Top, true);
        root.set_anchor(gtk4_layer_shell::Edge::Left, false);
        root.set_anchor(gtk4_layer_shell::Edge::Right, false);
        root.set_anchor(gtk4_layer_shell::Edge::Bottom, false);

        root.set_margin(gtk4_layer_shell::Edge::Top, OVERLAY_TOP_OFFSET_PX);
        root.set_margin(gtk4_layer_shell::Edge::Bottom, OVERLAY_BOTTOM_OFFSET_PX);

        // Styling: use Adwaita window background color and rounded corners.
        //
        // We style a dedicated content root widget (not the toplevel) so the compositor surface
        // remains a single layer-shell window, but visually looks like an overlay panel.
        let css = format!(
            r#"
            /* Make the toplevel surface itself fully transparent so the compositor doesn't
             * paint an opaque rectangular background that masks our rounded corners (this
             * tends to show up in light mode more than dark mode).
             */
            window,
            .background {{
                background: transparent;
            }}

            /* The visible overlay panel. */
            .relm4-overlay-surface {{
                background: @window_bg_color;
                border-radius: {}px;
            }}
            "#,
            OVERLAY_CORNER_RADIUS_PX
        );
        let provider = gtk::CssProvider::new();
        provider.load_from_data(&css);
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        // Build the registry (init: GTK-free; mount: GTK-safe).
        //
        // For step 05, we only register stub plugins.
        let mut registry = RelmPluginRegistry::new();

        // Stub notifications plugin so we have a target for toast gating wiring.
        registry.register(Box::new(NotificationsStubPlugin::new()));

        // init_all must remain GTK-free.
        registry
            .init_all(PluginInitContext::default())
            .expect("plugin init_all() should succeed");

        // mount_all happens after GTK is initialized (we are inside Relm4 init => safe).
        registry
            .mount_all(PluginMountContext::default())
            .expect("plugin mount_all() should succeed");

        // Router initial state: overlay initially visible once the window is mapped;
        // but we keep this conservative (start hidden, update on map/unmap).
        let top_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        top_box.set_hexpand(true);

        let left_col = gtk::Box::new(gtk::Orientation::Vertical, 12);
        left_col.set_hexpand(true);
        left_col.set_vexpand(true);

        let right_col = gtk::Box::new(gtk::Orientation::Vertical, 12);
        right_col.set_hexpand(true);
        right_col.set_vexpand(true);

        // Temporary headers to make the layout obvious in the smoke test.
        let top_hdr = gtk::Label::new(Some("Top slot"));
        top_hdr.set_xalign(0.0);
        top_hdr.add_css_class("dim-label");

        let left_hdr = gtk::Label::new(Some("Left slot"));
        left_hdr.set_xalign(0.0);
        left_hdr.add_css_class("dim-label");

        let right_hdr = gtk::Label::new(Some("Right slot"));
        right_hdr.set_xalign(0.0);
        right_hdr.add_css_class("dim-label");

        top_box.append(&top_hdr);
        left_col.append(&left_hdr);
        right_col.append(&right_hdr);

        let model = AppModel {
            window: root.clone(),

            registry,
            router_state: RouterState {
                overlay_visible: false,
                toast_gating_enabled: true,
            },
            top_box: top_box.clone(),
            left_col: left_col.clone(),
            right_col: right_col.clone(),
            mounted_widgets: Vec::new(),
        };

        // Build base widget tree (Top / Left / Right areas).
        //
        // Structure:
        // - scroller (caps height to available display height via vexpand + layer-shell margins)
        //   - main_vbox
        //       - top_box (horizontal)
        //       - content_row (horizontal)
        //           - left_col (vertical)
        //           - spacer
        //           - right_col (vertical)
        let main_vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
        main_vbox.set_margin_top(16);
        main_vbox.set_margin_bottom(16);
        main_vbox.set_margin_start(16);
        main_vbox.set_margin_end(16);

        // IMPORTANT: append the actual widgets (and only once).
        main_vbox.append(&top_box);

        let content_row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        content_row.set_hexpand(true);
        content_row.set_vexpand(true);

        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spacer.set_hexpand(true);
        spacer.set_vexpand(true);

        content_row.append(&left_col);
        content_row.append(&spacer);
        content_row.append(&right_col);

        main_vbox.append(&content_row);

        // Cap height to available space: the scroller will take at most the window height,
        // which layer-shell constrains implicitly to the display minus margins.
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_vscrollbar_policy(gtk::PolicyType::Automatic);
        scroller.set_propagate_natural_height(true);
        scroller.set_propagate_natural_width(true);
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&main_vbox));

        // Clipping root. We rely on `gtk::Overflow::Hidden` (set in code) instead of the CSS
        // `overflow` property (not supported by GTK CSS parser).
        let clip = gtk::Frame::new(None);
        clip.add_css_class("relm4-overlay-surface");
        clip.set_hexpand(true);
        clip.set_vexpand(true);
        clip.set_overflow(gtk::Overflow::Hidden);

        // Parent the scroller exactly once: as the child of the clipping frame.
        clip.set_child(Some(&scroller));

        // Ensure the extension trait is in scope via relm4::adw prelude; `adw::ApplicationWindow` supports `set_content`.
        root.set_content(Some(&clip));

        // Dismissal UX:
        // - Escape closes/hides the overlay
        // - Loss of focus closes/hides the overlay (more reliable than “click outside” on layer-shell)
        //
        // NOTE: We keep it focusable (keyboard mode on-demand) so Escape and focus transitions are reliable.
        {
            let sender = sender.clone();
            let controller = gtk::EventControllerKey::new();
            controller.connect_key_pressed(move |_c, key, _code, _state| {
                if key == gtk::gdk::Key::Escape {
                    sender.input(AppInput::RequestHide);
                    return gtk::glib::Propagation::Stop;
                }
                gtk::glib::Propagation::Proceed
            });
            root.add_controller(controller);
        }
        {
            let sender = sender.clone();
            root.connect_is_active_notify(move |w| {
                // If the window loses focus/activeness, hide it.
                if !w.is_active() {
                    sender.input(AppInput::RequestHide);
                }
            });
        }

        // Wire window visibility signals.
        //
        // We use map/unmap to detect compositor-level visibility transitions.
        // These can be noisy on some platforms; for this step we only need to
        // demonstrate message plumbing without panics.
        // GTK4: use `connect_map` / `connect_unmap` (no `*_event` variants), and no `gtk::Inhibit`.
        {
            let sender = sender.clone();
            root.connect_map(move |_w| {
                sender.input(AppInput::WindowMapped);
            });
        }
        {
            let sender = sender.clone();
            root.connect_unmap(move |_w| {
                sender.input(AppInput::WindowUnmapped);
            });
        }

        // Trigger plugin widget mounting once (after view is constructed).
        sender.input(AppInput::MountPlugins);

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppInput::MountPlugins => {
                if !self.mounted_widgets.is_empty() {
                    // Ensure we mount exactly once.
                    return;
                }

                // Compute the deterministic ordering and mount into GTK containers.
                //
                // NOTE: We intentionally mount stub widgets here rather than actual plugin Relm4 components.
                // Step 06+ will migrate real plugins to Relm4 components and expose their widget roots.
                let mounted = self.registry.mounted_sorted().to_vec();

                for m in mounted {
                    let id = m.id.to_string();
                    let slot = m.placement.slot;
                    let weight = m.placement.weight;

                    let w = build_plugin_stub_widget(&id, slot, weight);
                    self.mounted_widgets
                        .push(MountedWidget::new(id, slot, weight, w));
                }

                // Append widgets into the correct container using stored widget refs.
                for mw in &self.mounted_widgets {
                    match mw.slot {
                        Slot::Top => self.top_box.append(&mw.widget),
                        Slot::Left => self.left_col.append(&mw.widget),
                        Slot::Right => self.right_col.append(&mw.widget),
                    }
                }
            }

            AppInput::WindowMapped => {
                let (state1, effects) =
                    reduce_router(self.router_state.clone(), AppMsg::OverlayShown);
                self.router_state = state1;
                execute_router_effects(&self.registry, &effects);

                // Optional: make this visible in logs for the smoke test without forcing UI.
                // eprintln!("[relm4-app] OverlayShown => effects={effects:?}");
                let _ = sender; // keep sender available for later steps
            }

            AppInput::WindowUnmapped => {
                let (state1, effects) =
                    reduce_router(self.router_state.clone(), AppMsg::OverlayHidden);
                self.router_state = state1;
                execute_router_effects(&self.registry, &effects);

                // eprintln!("[relm4-app] OverlayHidden => effects={effects:?}");
                let _ = sender;
            }

            AppInput::ShowOverlay => {
                // Show + focus (required by user). `present()` is the idiomatic way to raise/activate.
                self.window.set_visible(true);
                self.window.present();
                let _ = sender;
            }

            AppInput::HideOverlay => {
                self.window.set_visible(false);
                let _ = sender;
            }

            AppInput::ToggleOverlay => {
                if self.window.is_visible() {
                    self.window.set_visible(false);
                } else {
                    self.window.set_visible(true);
                    self.window.present();
                }
                let _ = sender;
            }

            AppInput::StopApp => {
                // Terminate from the GTK thread.
                //
                // Use the global main application so this works regardless of how the app
                // was started (and avoids touching GTK from the IPC thread).
                relm4::main_application().quit();
                let _ = sender;
            }

            AppInput::RequestHide => {
                // Hide the overlay and rely on the unmap signal to produce `OverlayHidden`
                // (and therefore toast gating re-enable) via the existing plumbing.
                self.window.set_visible(false);
                let _ = sender;
            }

            AppInput::Router(_m) => {
                // Reserved for future: real app routing surface.
            }
        }
    }
}

/// Run the Relm4 overlay host app (feature-gated entrypoint from `main.rs`).
pub fn run() {
    // CLI/IPC policy (requested):
    // - `sacrebleui` (no args): start UI + become server; if already running => exit non-zero
    // - `sacrebleui toggle|show|hide`: IPC client command and exit
    //
    // IMPORTANT: Do not create any GTK widgets before a display exists.
    // We therefore launch the Relm4 component during GTK application startup.
    let args: Vec<String> = std::env::args().collect();
    let socket = match ipc_socket_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    // Client mode: `toggle|show|hide` (or legacy JSON form) => send to running instance and exit.
    if let Ok(Some(cmd)) = command_from_args(&args) {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("failed to create tokio runtime: {e}");
                std::process::exit(2);
            }
        };

        let res: Result<String, ipc_net::IpcNetError> =
            rt.block_on(async { ipc_net::send_command(&socket, cmd).await });

        match res {
            Ok(reply) => {
                if !reply.is_empty() {
                    println!("{reply}");
                }
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        }
    }

    // Server mode: try to become server; if already running => exit non-zero (requested).
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to create tokio runtime: {e}");
            std::process::exit(2);
        }
    };

    let listener = match rt.block_on(async { ipc_net::try_become_server(&socket).await }) {
        Ok(l) => l,
        Err(ipc_net::IpcNetError::AlreadyRunning) => {
            eprintln!("already running");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    // Shared sender slot filled on GTK startup.
    let sender_slot: Arc<Mutex<Option<relm4::Sender<AppInput>>>> = Arc::new(Mutex::new(None));

    // Spawn IPC server thread (Tokio runtime) that forwards commands onto the GTK main context.
    // It will only emit Relm4 inputs once the sender is available.
    {
        let sender_slot = sender_slot.clone();
        thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("failed to create tokio runtime for ipc server: {e}");
                    return;
                }
            };

            let on_command = move |cmd: IpcCommand| {
                let sender_opt = sender_slot.lock().ok().and_then(|g| g.clone());
                let Some(sender) = sender_opt else {
                    // UI not ready yet; ignore (best-effort).
                    return;
                };

                // IMPORTANT:
                // This IPC server runs on a non-GLib/GTK thread.
                // We must schedule the UI work onto the GLib main context thread.
                //
                // `invoke()` is safe to call from other threads; it marshals the closure
                // onto the owning main context.
                gtk::glib::MainContext::default().invoke(move || match cmd {
                    IpcCommand::Show => sender.emit(AppInput::ShowOverlay),
                    IpcCommand::Hide => sender.emit(AppInput::HideOverlay),
                    IpcCommand::Toggle => sender.emit(AppInput::ToggleOverlay),
                    IpcCommand::Stop => sender.emit(AppInput::StopApp),
                    IpcCommand::Ping => {}
                });
            };

            let _ = rt.block_on(async { ipc_net::run_server(listener, on_command).await });
        });
    }

    // We do not instantiate a `RelmApp<M>` here because we are manually wiring startup and
    // running the global GTK application. Creating a `RelmApp` would require selecting an
    // `M` (top-level message type) and is redundant for this entrypoint.

    {
        let sender_slot = sender_slot.clone();
        let payload = std::cell::Cell::new(Some(()));
        let app_ref = relm4::main_application();

        app_ref.connect_startup(move |app: &gtk::Application| {
            if let Some(payload) = payload.take() {
                let connector = relm4::ComponentBuilder::<AppModel>::default().launch(payload);
                let sender = connector.sender().clone();

                // Store sender for IPC.
                if let Ok(mut g) = sender_slot.lock() {
                    *g = Some(sender);
                }

                // Add window to the application and start hidden (requested).
                let window = connector.widget().clone();
                app.add_window(&window);
                window.set_visible(false);

                // Keep the component runtime alive.
                let mut controller = connector.detach();
                controller.detach_runtime();
            }
        });
    }

    // Run the GTK application main loop.
    relm4::main_application().run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relm4_app::plugin_framework::{
        MountedPlugin, MountedPluginMeta, PluginEndpoint, PluginInitError, PluginMountError,
        PluginRouteError,
    };
    use std::any::TypeId;

    /// Registry wiring test: when notifications plugin is present, SetToastGating sends typed input.
    ///
    /// This is a pure test (no GTK), exercising only the registry + endpoint type plumbing.
    #[test]
    fn wiring_sends_toast_gating_to_notifications_when_present() {
        // IMPORTANT:
        // The production wiring targets `NotificationsSpec` (and its `NotificationsInput`).
        // Therefore the test plugin must use the *same* input type; otherwise
        // `registry.get::<NotificationsSpec>()` will return `None` due to `TypeId` mismatch.
        struct CaptureSpec;
        impl PluginSpec for CaptureSpec {
            type Input = NotificationsInput;
            fn id() -> crate::relm4_app::events::PluginId {
                NotificationsSpec::id()
            }
            fn name() -> &'static str {
                "capture"
            }
            fn placement() -> PluginPlacement {
                PluginPlacement::new(Slot::Left, 0)
            }
        }

        struct CapturePlugin {
            shared: std::sync::Arc<std::sync::Mutex<Vec<NotificationsInput>>>,
        }

        impl RelmPlugin for CapturePlugin {
            fn id(&self) -> crate::relm4_app::events::PluginId {
                NotificationsSpec::id()
            }
            fn name(&self) -> &'static str {
                "capture"
            }
            fn placement(&self) -> PluginPlacement {
                PluginPlacement::new(Slot::Left, 0)
            }
            fn init(&mut self, _ctx: PluginInitContext) -> Result<(), PluginInitError> {
                Ok(())
            }
            fn mount(
                &mut self,
                _ctx: PluginMountContext,
            ) -> Result<MountedPlugin, PluginMountError> {
                struct Ep {
                    plugin: crate::relm4_app::events::PluginId,
                    shared: std::sync::Arc<std::sync::Mutex<Vec<NotificationsInput>>>,
                }
                impl PluginEndpoint for Ep {
                    fn plugin_id(&self) -> crate::relm4_app::events::PluginId {
                        self.plugin.clone()
                    }
                    fn input_type_id(&self) -> TypeId {
                        TypeId::of::<NotificationsInput>()
                    }
                    fn send_any(&self, msg: &dyn std::any::Any) -> Result<(), PluginRouteError> {
                        let m = msg.downcast_ref::<NotificationsInput>().ok_or(
                            PluginRouteError::WrongMsgType {
                                plugin: self.plugin.clone(),
                                expected: std::any::type_name::<NotificationsInput>(),
                                got: "unknown",
                            },
                        )?;

                        self.shared.lock().unwrap().push(m.clone());
                        Ok(())
                    }
                }

                let id = self.id();
                Ok(MountedPlugin {
                    meta: MountedPluginMeta {
                        id: id.clone(),
                        name: self.name(),
                        placement: self.placement(),
                    },
                    endpoint: Box::new(Ep {
                        plugin: id,
                        shared: self.shared.clone(),
                    }),
                })
            }
        }

        // Build a registry with our capture plugin.
        let shared = std::sync::Arc::new(std::sync::Mutex::new(Vec::<NotificationsInput>::new()));
        let mut reg = RelmPluginRegistry::new();
        reg.register(Box::new(CapturePlugin {
            shared: shared.clone(),
        }));

        reg.init_all(PluginInitContext::default()).unwrap();
        reg.mount_all(PluginMountContext::default()).unwrap();

        // Execute effect: should send typed message.
        execute_router_effects(&reg, &[RouterEffect::SetToastGating { enabled: false }]);

        let got = shared.lock().unwrap().clone();
        assert_eq!(
            got,
            vec![NotificationsInput::SetToastGating { enabled: false }]
        );

        // Also verify it doesn't panic on another call.
        execute_router_effects(&reg, &[RouterEffect::SetToastGating { enabled: true }]);
        let got = shared.lock().unwrap().clone();
        assert_eq!(
            got,
            vec![
                NotificationsInput::SetToastGating { enabled: false },
                NotificationsInput::SetToastGating { enabled: true }
            ]
        );

        // Ensure typed handle acquisition works.
        let h = reg.get::<CaptureSpec>().expect("should have typed handle");
        h.send(&NotificationsInput::SetToastGating { enabled: true })
            .unwrap();
    }

    /// Wiring must not panic when the notifications plugin is absent/disabled.
    #[test]
    fn wiring_does_not_panic_when_notifications_plugin_missing() {
        let reg = RelmPluginRegistry::new();
        execute_router_effects(&reg, &[RouterEffect::SetToastGating { enabled: false }]);
    }
}
