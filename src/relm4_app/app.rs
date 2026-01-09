use glib::object::Cast;
use relm4::adw;
use relm4::gtk;

// Extension traits required for `adw::ApplicationWindow::set_content(...)` and GTK window props.
use anyhow::Result;
use gtk::prelude::{FrameExt, GtkWindowExt, WidgetExt};

// Needed for the IPC-enabled entrypoint:
// - `connect_startup` comes from `gio::ApplicationExt` (re-exported by gtk prelude)
// - `run()` comes from `gio::ApplicationExtManual` (also re-exported by gtk prelude)
// - `add_window` comes from `GtkApplicationExt`
use gtk::prelude::{ApplicationExt, ApplicationExtManual, GtkApplicationExt};

use relm4::adw::prelude::AdwApplicationWindowExt;
use relm4::gtk::prelude::BoxExt;

use gtk4_layer_shell::LayerShell;

use relm4::prelude::*;

use super::plugin_registry::PluginRegistry;

// IPC (kept in a separate module/file; this component only consumes commands).
use crate::dbus::DbusHandle;
use crate::ipc::net as ipc_net;
use crate::ipc::{IpcCommand, command_from_args, ipc_socket_path};
use crate::relm4_app::plugin::Slot;
use crate::relm4_app::plugin::WidgetFeatureToggle;
use crate::relm4_app::plugins::clock::ClockPlugin;
use crate::relm4_app::plugins::darkman::DarkmanPlugin;
use crate::relm4_app::plugins::sunsetr::SunsetrPlugin;
use crate::relm4_app::ui::feature_grid::FeatureGrid;
use crate::relm4_app::ui::feature_grid::FeatureGridInit;

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

const OVERLAY_WIDTH_PX: i32 = 920;
const OVERLAY_TOP_OFFSET_PX: i32 = 16;
const OVERLAY_BOTTOM_OFFSET_PX: i32 = 16;
const OVERLAY_CORNER_RADIUS_PX: i32 = 8;

struct AppModel {
    window: adw::ApplicationWindow,
    #[allow(dead_code)]
    registry: Arc<PluginRegistry>,
    #[allow(dead_code)]
    top_box: gtk::Box,
    #[allow(dead_code)]
    left_col: gtk::Box,
    #[allow(dead_code)]
    right_col: gtk::Box,
    #[allow(dead_code)]
    toggles: Vec<Arc<WidgetFeatureToggle>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppInput {
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

struct AppContext {
    registry: Arc<PluginRegistry>,
}

#[relm4::component]
impl SimpleComponent for AppModel {
    type Init = AppContext;
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
        init: Self::Init,
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
                padding: 24px;
            }}

            .clock-btn {{
                background: transparent;
                border-radius: 12px;
                margin: 0;
                padding: 0;
            }}

            /* Feature toggles */
            .feature-toggle {{
                background: @card_bg_color;
                border-radius: 28px;
                min-height: 44px;
                padding: 2px 20px 2px 12px;
                margin: 0;
            }}

            .feature-toggle:hover {{
              background-color: color-mix(
                in srgb,
                @card_bg_color 80%,
                @window_fg_color
              );
            }}

            .feature-toggle .title {{
              font-weight: 600;
            }}

            .feature-toggle .details {{
              font-size: 14px;
              margin: 0;
              padding: 0;
            }}

            /* ON applies to the entire tile (both halves). */
            .feature-toggle.active {{
                background-color: @accent_bg_color;
                color: var(--button_bg_color);
            }}

            .feature-toggle.active:hover {{
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 80%,
                  @window_fg_color
                );
            }}
            .qs-tile.qs-on .qs-btn-right {{
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 90%,
                  @window_fg_color
                );
            }}

            .qs-tile.qs-on .qs-btn-right:hover {{
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 75%,
                  @window_fg_color
                );
            }}

            .qs-tile.qs-on .qs-btn-left:hover {{
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 75%,
                  @window_fg_color
                );
            }}

            .qs-tile.qs-on .qs-btn-left:hover {{
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 75%,
                  @window_fg_color
                );
            }}

            .qs-tile.qs-on .qs-btn-left:hover {{
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 75%,
                  @window_fg_color
                );
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

        // Router initial state: overlay initially visible once the window is mapped;
        // but we keep this conservative (start hidden, update on map/unmap).
        let top_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        top_box.set_hexpand(true);

        let top_box_divider = gtk::Separator::new(gtk::Orientation::Horizontal);
        top_box_divider.set_hexpand(true);

        let left_col = gtk::Box::builder()
            .hexpand(true)
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .vexpand(true)
            .width_request(480)
            .build();

        let right_col = gtk::Box::builder()
            .hexpand(true)
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .vexpand(true)
            .width_request(480)
            .build();

        // Temporary headers to make the layout obvious in the smoke test.
        let left_hdr = gtk::Label::new(Some("Left slot"));
        left_hdr.set_xalign(0.0);
        left_hdr.add_css_class("dim-label");

        left_col.append(&left_hdr);

        let header_widgets = init.registry.get_widgets_for_slot(Slot::Header);
        for w in &header_widgets {
            top_box.append(&w.el);
        }

        let toggles = init.registry.get_all_feature_toggles();
        let grid = FeatureGrid::builder()
            .launch(FeatureGridInit {
                items: toggles.clone(),
            })
            .detach();
        let gridget = grid.widget().clone().upcast::<gtk::Widget>();
        right_col.append(&gridget);
        // for toggle in &toggles {
        //     right_col.append(&toggle.el);
        // }

        let model = AppModel {
            window: root.clone(),
            registry: init.registry.clone(),
            top_box: top_box.clone(),
            left_col: left_col.clone(),
            right_col: right_col.clone(),
            toggles: toggles,
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
        main_vbox.set_margin_all(0);

        // IMPORTANT: append the actual widgets (and only once).
        main_vbox.append(&top_box);
        main_vbox.append(&top_box_divider);

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

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
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
        }
    }
}

/// Run the Relm4 overlay host app (feature-gated entrypoint from `main.rs`).
pub async fn run() -> Result<()> {
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
        let res: Result<String, ipc_net::IpcNetError> = ipc_net::send_command(&socket, cmd).await;

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

    let listener = match ipc_net::try_become_server(&socket).await {
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

    let dbus = Arc::new(DbusHandle::connect().await?);
    let mut registry = PluginRegistry::new();

    registry.register(ClockPlugin::new());
    registry.register(DarkmanPlugin::new(dbus));
    registry.register(SunsetrPlugin::new());
    // init_all must remain GTK-free.
    registry.initialize_all().await?;

    let sender_slot = sender_slot.clone();
    let payload = std::cell::Cell::new(Some(()));
    let app_ref = relm4::main_application();

    let registry_arc = Arc::new(registry);

    app_ref.connect_startup(move |app: &gtk::Application| {
        if let Some(_payload) = payload.take() {
            let registry = registry_arc.clone();
            let app = app.clone();
            let sender_slot = sender_slot.clone();

            glib::MainContext::default().spawn_local(async move {
                let _ = registry.create_elements().await;
                let connector = relm4::ComponentBuilder::<AppModel>::default().launch(AppContext {
                    registry: registry.clone(),
                });
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
            });
        }
    });

    // Run the GTK application main loop.
    relm4::main_application().run();
    Ok(())
}
