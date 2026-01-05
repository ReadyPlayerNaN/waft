//! sacrebleui: GTK4/Adwaita quick-settings style overlay for Wayland.
//!
//! Requirements implemented:
//! - Hidden by default
//! - Wayland-only overlay via gtk4-layer-shell
//! - Auto-hide on focus-out or Escape
//! - Single-instance: subsequent runs send JSON message to the running instance
//! - JSON IPC similar in spirit to niri-style command sockets (Unix domain socket + JSON)
//! - Dummy UI: clock (hardcoded), grouped notifications w/ actions, sliders,
//!   and an accordion for Wi-Fi/Bluetooth/Network with hardcoded content.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

mod ui;

use adw::prelude::*;
use anyhow::{Context, Result};
use gtk::gdk;
use gtk::glib;
use gtk4_layer_shell::LayerShell;
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::mpsc,
};

/// Meeting item for today's agenda
#[derive(Debug, Clone)]
struct MeetingItem {
    time: String,
    title: String,
    has_google_meet: bool,
    has_zoom: bool,
    has_teams: bool,
}

/// IPC message format: accepts JSON objects or arrays; only a small command set.
///
/// Example messages:
/// {"cmd":"show"}
/// {"cmd":"hide"}
/// {"cmd":"toggle"}
/// {"cmd":"ping"}
///
/// Also supports niri-like "command + args" shape:
/// {"command":"show"} or {"command":"toggle"}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
struct IpcMessage {
    #[serde(default)]
    cmd: Option<String>,
    #[serde(default)]
    command: Option<String>,
}

impl IpcMessage {
    fn command_name(&self) -> Option<&str> {
        self.cmd.as_deref().or(self.command.as_deref())
    }
}

#[derive(Debug, Clone, Copy)]
enum UiCommand {
    Show,
    Hide,
    Toggle,
    Ping,
}

fn parse_ui_command(msg: &IpcMessage) -> Option<UiCommand> {
    let cmd = msg.command_name()?.trim().to_ascii_lowercase();
    match cmd.as_str() {
        "show" => Some(UiCommand::Show),
        "hide" => Some(UiCommand::Hide),
        "toggle" => Some(UiCommand::Toggle),
        "ping" => Some(UiCommand::Ping),
        _ => None,
    }
}

/// Return the socket path used for single-instance IPC.
///
/// Namespaced by:
/// - UID (multi-user safety)
/// - WAYLAND_DISPLAY (multi-session safety)
///
/// Uses XDG_RUNTIME_DIR so it behaves like other Wayland compositors/tools.
fn ipc_socket_path() -> Result<PathBuf> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .context("XDG_RUNTIME_DIR is not set (Wayland session expected)")?;

    let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    let uid = std::env::var("UID").unwrap_or_else(|_| "unknown".into());

    let filename = format!("sacrebleui.{uid}.{wayland_display}.sock");
    Ok(PathBuf::from(runtime_dir).join(filename))
}

async fn send_ipc_message(socket: &Path, json: &str) -> Result<String> {
    let mut stream = UnixStream::connect(socket)
        .await
        .with_context(|| format!("failed to connect to IPC socket {}", socket.display()))?;

    // Allow multiple JSON payloads separated by newline; server reads until '\n' for first request.
    stream.write_all(json.as_bytes()).await?;
    if !json.ends_with('\n') {
        stream.write_all(b"\n").await?;
    }
    stream.shutdown().await?;

    // Read response (best-effort).
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf).await;
    Ok(String::from_utf8_lossy(&buf).trim().to_string())
}

async fn try_become_server(socket: &Path) -> Result<UnixListener> {
    // Clean stale socket if present (common after crash).
    if socket.exists() {
        // If connect works, someone is running.
        if UnixStream::connect(socket).await.is_ok() {
            anyhow::bail!("already running");
        }
        let _ = std::fs::remove_file(socket);
    }
    UnixListener::bind(socket)
        .with_context(|| format!("failed to bind IPC socket {}", socket.display()))
}

async fn run_ipc_server(listener: UnixListener, tx: mpsc::UnboundedSender<UiCommand>) {
    loop {
        let accept = listener.accept().await;
        let (mut stream, _addr) = match accept {
            Ok(v) => v,
            Err(_) => continue,
        };

        let tx = tx.clone();
        tokio::spawn(async move {
            // Read up to first newline; keep it simple and tolerant.
            let mut buf = Vec::with_capacity(4096);
            let mut tmp = [0u8; 1024];

            loop {
                match stream.read(&mut tmp).await {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if buf.contains(&b'\n') || buf.len() > 64 * 1024 {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            let line = match std::str::from_utf8(&buf) {
                Ok(s) => s.lines().next().unwrap_or("").trim(),
                Err(_) => "",
            };

            let mut response = r#"{"ok":false,"error":"invalid_request"}"#.to_string();

            if !line.is_empty() {
                match serde_json::from_str::<IpcMessage>(line) {
                    Ok(msg) => {
                        if let Some(cmd) = parse_ui_command(&msg) {
                            // Queue command for GTK thread.
                            let _ = tx.send(cmd);
                            response = match cmd {
                                UiCommand::Ping => r#"{"ok":true,"reply":"pong"}"#.to_string(),
                                UiCommand::Show => r#"{"ok":true,"queued":"show"}"#.to_string(),
                                UiCommand::Hide => r#"{"ok":true,"queued":"hide"}"#.to_string(),
                                UiCommand::Toggle => r#"{"ok":true,"queued":"toggle"}"#.to_string(),
                            };
                        } else {
                            response = r#"{"ok":false,"error":"unknown_command"}"#.to_string();
                        }
                    }
                    Err(_) => {
                        response = r#"{"ok":false,"error":"malformed_json"}"#.to_string();
                    }
                }
            }

            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.write_all(b"\n").await;
            let _ = stream.shutdown().await;
        });
    }
}

/// Build the overlay window contents (dummy UI).
fn build_ui(app: &adw::Application) -> gtk::Window {
    // Use AdwApplicationWindow to get Adwaita styling.
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("sacrebleui")
        .default_width(520)
        .default_height(560)
        .resizable(false)
        .build();

    // Local CSS tweaks (dummy UI polish):
    // - shrink notification action buttons reliably
    // - GNOME-shell-like quick settings tiles (custom layout; NOT Adw rows)
    let css = r#"
    window {
        border-radius: 8px;
    }

    /* The class is applied to the button itself, so target it directly. */
    button.notif-action {
        font-size: 0.92em;
        padding: 3px 10px;
        min-height: 26px;
        min-width: 0px;
    }

    /* Meeting action buttons */
    button.meeting-action {
        font-size: 0.85em;
        padding: 4px 8px;
        min-height: 24px;
        min-width: 0px;
    }

    /* Destructive action button (for Clear) */
    button.destructive-action {
        background: @destructive_bg_color;
        color: @destructive_fg_color;
    }
    button.destructive-action:hover {
        background: shade(@destructive_bg_color, 1.1);
    }

    /*
     * Quick Settings tiles (GNOME Shell-inspired)
     *
     * NOTE: GNOME Shell uses St/Clutter, not GTK. This is an approximation using GTK widgets + CSS.
     */
    .qs-section-title {
        font-weight: 600;
    }

    /*
     * Quick Settings tiles (GNOME Shell-inspired, GTK approximation)
     *
     * Split tiles are implemented as TWO adjacent buttons inside a single rounded tile container,
     * with a divider between them. Content-less tiles are ONE button inside the same tile container.
     * Both MUST have identical outer dimensions.
     */
    .qs-tile {
        background: transparent;
        padding: 6px 0;
        min-height: 40px;
    }

    /* ON applies to the entire tile (both halves). */
    .qs-tile.qs-on .qs-btn-left,
    .qs-tile.qs-on .qs-btn-single {
        background: image(alpha(@accent_bg_color, 0.5));
        color: @accent_fg_color;
    }
    .qs-tile.qs-on .qs-btn-left:hover,
    .qs-tile.qs-on .qs-btn-single:hover {
        background: @accent_bg_color;
    }
    .qs-tile.qs-on .qs-btn-right {
        background: image(alpha(@accent_bg_color, 0.25));
     }

     .qs-tile.qs-on .qs-btn-right:hover {
        background: image(alpha(@accent_bg_color, 0.75));
     }

    .qs-tile.qs-on label,
    .qs-tile.qs-on image {
        color: @accent_fg_color;
    }

    /* Inner row for split tiles */
    .qs-split-row {
        border-radius: 16px;
    }

    /* Divider (visible, does not change on ON) */
    separator.qs-divider {
        min-width: 2px;
        background: transparent;
        margin-top: 4px;
        margin-bottom: 4px;
    }

    button.qs-btn-left,
    button.qs-btn-single {
      background: @card_bg_color;
      padding: 8px 18px 8px 16px;
    }

    button.qs-btn-left:hover,
    button.qs-btn-single:hover {
      background: image(alpha(@card_bg_color, 1.5));
    }

    button.qs-btn-right {
        background: image(alpha(@card_bg_color, 1.5));
        padding: 8px 0;
    }
    button.qs-btn-right:hover {
        background: image(alpha(@card_bg_color, 2));
    }

    /* Inner buttons are flat and transparent by default; hover overlays provide feedback. */
    button.qs-btn-left,
    button.qs-btn-right,
    button.qs-btn-single {
        margin: 0px;
        box-shadow: none;
        border-radius: 0px;
    }

    /* Content-less tile: single button fills tile; no chevron cap */
    button.qs-btn-single {
        border-radius: 12px;
        min-height: 40px;
    }

    /* Split tile: left button rounded on left, square on right */
    button.qs-btn-left {
        border-top-left-radius: 12px;
        border-bottom-left-radius: 12px;
        border-top-right-radius: 0px;
        border-bottom-right-radius: 0px;
    }
    /* Split tile: right button square on left, rounded on right */
    button.qs-btn-right {
        min-width: 56px; /* fixed chevron zone width */
        border-top-left-radius: 0px;
        border-bottom-left-radius: 0px;
        border-top-right-radius: 12px;
        border-bottom-right-radius: 12px;
    }

    /* Details panel: full-width */
    .qs-details {
        background: @card_bg_color;
        border-radius: 16px;
        padding: 10px 12px;
        margin: 0;
    }
    "#;

    let provider = gtk::CssProvider::new();
    provider.load_from_data(css);
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("GDK display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Hide by default; we'll show on IPC.
    window.set_visible(false);

    // Make it feel like a quick overlay.
    window.set_decorated(false);
    window.set_hide_on_close(true);
    window.set_modal(false);

    // Layer-shell positioning: centered overlay, margins, keyboard focus.
    // NOTE: Wayland-only by design.
    window.init_layer_shell();
    window.set_layer(gtk4_layer_shell::Layer::Overlay);
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);

    // Keep near the top and center horizontally.
    //
    // IMPORTANT: Anchoring both Left and Right without specifying an explicit width can cause the
    // compositor to treat the surface as "stretchable", which effectively places it at the left.
    // Instead, anchor to Top only and let GTK size the window; then set a horizontal margin so
    // it doesn't touch edges.
    window.set_anchor(gtk4_layer_shell::Edge::Top, true);
    window.set_anchor(gtk4_layer_shell::Edge::Left, false);
    window.set_anchor(gtk4_layer_shell::Edge::Right, false);

    window.set_margin(gtk4_layer_shell::Edge::Top, 16);
    window.set_margin(gtk4_layer_shell::Edge::Left, 16);
    window.set_margin(gtk4_layer_shell::Edge::Right, 16);

    // Content root: overall vertical stack with a header spanning both columns,
    // and then a two-column split below it.
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .margin_top(32)
        .margin_bottom(32)
        .margin_start(32)
        .margin_end(32)
        .build();

    // Header: two-line date/time (dummy locale-formatted strings for now).
    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .build();

    let date_label = gtk::Label::builder()
        .label("Mon, 01 Jan 2026")
        .xalign(0.0)
        .css_classes(["title-3", "dim-label"])
        .build();

    let time_label = gtk::Label::builder()
        .label("12:34")
        .xalign(0.0)
        .css_classes(["title-1"])
        .build();

    header.append(&date_label);
    header.append(&time_label);

    root.append(&header);

    // Two columns (notifications left, everything else right).
    let columns = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(16)
        .build();

    let left_col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .hexpand(true)
        .width_request(480)
        .build();

    let right_col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .hexpand(true)
        .width_request(480)
        .build();

    columns.append(&left_col);
    columns.append(&right_col);
    root.append(&columns);

    // Meeting agenda section
    let agenda_group = adw::PreferencesGroup::builder()
        .title("Today's Agenda")
        .build();

    // Mock meeting data
    let meetings = vec![
        MeetingItem {
            time: "09:00".to_string(),
            title: "Design Review - Team Sync".to_string(),
            has_google_meet: true,
            has_zoom: false,
            has_teams: false,
        },
        MeetingItem {
            time: "11:30".to_string(),
            title: "Client Call - Project Update".to_string(),
            has_google_meet: false,
            has_zoom: true,
            has_teams: true,
        },
        MeetingItem {
            time: "14:00".to_string(),
            title: "Sprint Planning".to_string(),
            has_google_meet: false,
            has_zoom: true,
            has_teams: false,
        },
        MeetingItem {
            time: "16:15".to_string(),
            title: "1:1 with Manager".to_string(),
            has_google_meet: true,
            has_zoom: false,
            has_teams: false,
        },
    ];

    // Helper to add a meeting item
    let add_meeting = |group: &adw::PreferencesGroup, meeting: &MeetingItem| {
        let row = adw::ActionRow::builder()
            .title(&meeting.title)
            .subtitle(&meeting.time)
            .build();
        row.set_activatable(false);

        // Create action buttons container
        let actions_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        // Add action buttons based on available meeting types
        if meeting.has_google_meet {
            let google_btn = gtk::Button::builder()
                .label("Open Google Meet")
                .css_classes(["pill", "meeting-action"])
                .build();
            actions_box.append(&google_btn);
        }
        if meeting.has_zoom {
            let zoom_btn = gtk::Button::builder()
                .label("Open Zoom Meeting")
                .css_classes(["pill", "meeting-action"])
                .build();
            actions_box.append(&zoom_btn);
        }
        if meeting.has_teams {
            let teams_btn = gtk::Button::builder()
                .label("Open Teams Meeting")
                .css_classes(["pill", "meeting-action"])
                .build();
            actions_box.append(&teams_btn);
        }

        row.add_suffix(&actions_box);
        group.add(&row);
    };

    for meeting in &meetings {
        add_meeting(&agenda_group, meeting);
    }

    left_col.append(&agenda_group);

    // Notifications section with controls
    let notifications = vec![
        ui::Notification {
            app_name: "Mail".to_string(),
            summary: "New message from Alex".to_string(),
            body: "Subject: Shipping update".to_string(),
            actions: vec!["Reply".to_string(), "Archive".to_string()],
        },
        ui::Notification {
            app_name: "Calendar".to_string(),
            summary: "Meeting starts in 10 minutes".to_string(),
            body: "Design review — Room 3B".to_string(),
            actions: vec!["Snooze".to_string(), "Open".to_string()],
        },
        ui::Notification {
            app_name: "Chat".to_string(),
            summary: "Mina mentioned you".to_string(),
            body: "Can you take a look at the PR?".to_string(),
            actions: vec!["Open".to_string(), "Mark as read".to_string()],
        },
    ];
    let notifications_widget = ui::build_notifications_section(notifications);
    left_col.append(&notifications_widget);

    // Sliders section.
    let sliders_group = adw::PreferencesGroup::builder().title("Controls").build();

    // Output volume: icon on the left + slider; device name under slider.
    let out_row = adw::ActionRow::builder().build();
    out_row.set_activatable(false);

    let out_icon = gtk::Image::from_icon_name("audio-volume-high-symbolic");
    out_icon.set_pixel_size(20);

    let out_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    out_scale.set_value(42.0);
    out_scale.set_hexpand(true);
    out_scale.set_draw_value(false);

    // Align the icon with the slider only (not with the slider + device label):
    // Put the icon in a "top row" next to the slider, and keep the device label below.
    let out_top = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .hexpand(true)
        .margin_start(12)
        .build();
    out_top.append(&out_icon);
    out_top.append(&out_scale);

    let out_slider_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .build();
    out_slider_box.append(&out_top);

    out_row.set_child(Some(&out_slider_box));
    sliders_group.add(&out_row);

    // Input volume: icon on the left + slider; device name under slider.
    let in_row = adw::ActionRow::builder().build();
    in_row.set_activatable(false);

    let in_icon = gtk::Image::from_icon_name("audio-input-microphone-symbolic");
    in_icon.set_pixel_size(20);

    let in_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    in_scale.set_value(65.0);
    in_scale.set_hexpand(true);
    in_scale.set_draw_value(false);

    let in_top = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .hexpand(true)
        .margin_start(12)
        .build();
    in_top.append(&in_icon);
    in_top.append(&in_scale);

    let in_slider_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .build();
    in_slider_box.append(&in_top);

    in_row.set_child(Some(&in_slider_box));
    sliders_group.add(&in_row);

    // Brightness: icon on the left + slider; device name under slider.
    let br_row = adw::ActionRow::builder().build();
    br_row.set_activatable(false);

    let br_icon = gtk::Image::from_icon_name("display-brightness-symbolic");
    br_icon.set_pixel_size(20);

    let br_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    br_scale.set_value(80.0);
    br_scale.set_hexpand(true);
    br_scale.set_draw_value(false);

    let br_top = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .hexpand(true)
        .margin_start(12)
        .build();
    br_top.append(&br_icon);
    br_top.append(&br_scale);

    let br_slider_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .build();
    br_slider_box.append(&br_top);

    br_row.set_child(Some(&br_slider_box));
    sliders_group.add(&br_row);

    right_col.append(&sliders_group);

    // Features extracted into `ui::features` and driven by declarative specs (order-based layout).
    let wifi_details = {
        let box_ = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        for ssid in ["HomeWiFi", "OfficeNet", "CoffeeShop"] {
            let row = adw::ActionRow::builder()
                .title(ssid)
                .subtitle("Known network")
                .build();
            row.set_activatable(false);
            box_.append(&row);
        }
        box_
    };

    let bt_details = {
        let box_ = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        for dev in ["Headphones Pro", "MX Master 3S", "Keyboard K2"] {
            let row = adw::ActionRow::builder()
                .title(dev)
                .subtitle("Known device")
                .build();
            row.set_activatable(false);
            box_.append(&row);
        }
        box_
    };

    // Network details, including the "Connect/Disconnect" dummy flow that updates the tile status.
    let net_status_label = gtk::Label::builder().label("Disconnected").build();
    let net_details = {
        let box_ = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();

        let action_row = adw::ActionRow::builder()
            .title("Connection")
            .subtitle("Connect / Disconnect")
            .build();
        action_row.set_activatable(false);

        let net_button = gtk::Button::builder()
            .label("Connect")
            .css_classes(["suggested-action"])
            .build();

        net_button.connect_clicked({
            let net_button = net_button.clone();
            let net_status_label = net_status_label.clone();
            move |_| {
                let current = net_button.label().unwrap_or_else(|| "Connect".into());
                if current == "Connect" {
                    net_button.set_label("Connecting…");
                    let net_button2 = net_button.clone();
                    let net_status_label2 = net_status_label.clone();
                    glib::timeout_add_local_once(Duration::from_millis(900), move || {
                        net_button2.set_label("Disconnect");
                        net_status_label2.set_label("Connected");
                    });
                } else if current == "Disconnect" {
                    net_button.set_label("Disconnecting…");
                    let net_button2 = net_button.clone();
                    let net_status_label2 = net_status_label.clone();
                    glib::timeout_add_local_once(Duration::from_millis(900), move || {
                        net_button2.set_label("Connect");
                        net_status_label2.set_label("Disconnected");
                    });
                }
            }
        });

        action_row.add_suffix(&net_button);
        box_.append(&action_row);

        let status_row = adw::ActionRow::builder()
            .title("Status")
            .subtitle("Dummy: disconnected")
            .build();
        status_row.set_activatable(false);
        box_.append(&status_row);

        box_
    };

    let specs = vec![
        ui::FeatureSpec::contentless(
            "dark_mode",
            "Dark mode",
            "weather-clear-night-symbolic",
            false,
        ),
        ui::FeatureSpec::contentless("night_light", "Night light", "night-light-symbolic", false),
        ui::FeatureSpec::contentful(
            "wifi",
            "Wi‑Fi",
            "network-wireless-signal-excellent-symbolic",
            "HomeWiFi",
            true,
            ui::features::MenuSpec::new(&wifi_details),
            false,
        ),
        ui::FeatureSpec::contentful(
            "bt",
            "Bluetooth",
            "bluetooth-active-symbolic",
            "Off",
            false,
            ui::features::MenuSpec::new(&bt_details),
            false,
        ),
        ui::FeatureSpec::contentful(
            "net",
            "Network",
            "network-wired-symbolic",
            net_status_label.label().to_string(),
            false,
            ui::features::MenuSpec::new(&net_details),
            false,
        ),
    ];

    let (features_section, features_model) = ui::build_features_section(specs);

    // Keep the network tile status in sync with the dummy connect/disconnect label updates.
    // (This is optional; you can remove it when wiring real services.)
    glib::timeout_add_local(Duration::from_millis(200), move || {
        let status = net_status_label.label().to_string();
        features_model.set_status_text("net", &status);
        glib::ControlFlow::Continue
    });

    right_col.append(&features_section);

    // Wrap in Clamp for nicer width (make both columns wider).
    let clamp = adw::Clamp::builder().maximum_size(1040).build();
    clamp.set_child(Some(&root));
    window.set_content(Some(&clamp));

    // Auto-hide behavior: Escape hides, focus-out hides.
    // Escape via EventControllerKey.
    let key = gtk::EventControllerKey::new();
    key.connect_key_pressed({
        let window = window.clone();
        move |_, keyval, _keycode, _state| {
            if keyval == gdk::Key::Escape {
                window.set_visible(false);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key);

    // Focus-out: on Wayland this tends to work as expected for overlays.
    window.connect_is_active_notify({
        let window = window.clone();
        move |w| {
            if !w.is_active() {
                window.set_visible(false);
            }
        }
    });

    window.upcast::<gtk::Window>()
}

fn show_overlay(window: &gtk::Window) {
    window.present();
    window.set_visible(true);
}

fn hide_overlay(window: &gtk::Window) {
    window.set_visible(false);
}

fn toggle_overlay(window: &gtk::Window) {
    if window.is_visible() {
        hide_overlay(window);
    } else {
        show_overlay(window);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // If a JSON message is provided as argv[1..], behave as a client and exit.
    // This is how you can implement single-instance control from the CLI:
    //   sacrebleui '{"cmd":"toggle"}'
    let socket = ipc_socket_path()?;

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let json = args[1..].join(" ");
        if socket.exists() {
            let reply = send_ipc_message(&socket, &json).await?;
            if !reply.is_empty() {
                println!("{reply}");
            }
            return Ok(());
        } else {
            anyhow::bail!("overlay is not running; start without args first");
        }
    }

    // Become the server (single-instance).
    let listener = match try_become_server(&socket).await {
        Ok(l) => l,
        Err(e) => {
            // If already running, try toggling as a convenience.
            if format!("{e:#}").contains("already running") {
                let _ = send_ipc_message(&socket, r#"{"cmd":"toggle"}"#).await;
                return Ok(());
            }
            return Err(e);
        }
    };

    // Channel from IPC task -> GTK main thread.
    let (tx, rx) = mpsc::unbounded_channel::<UiCommand>();

    // Spawn IPC server task.
    tokio::spawn(run_ipc_server(listener, tx));

    // GTK/Adwaita app.
    let app = adw::Application::builder()
        .application_id("dev.sacrebleui.Overlay")
        .build();

    // `connect_activate` requires an `Fn` closure; we can't move the receiver into an `FnOnce`.
    // Wrap it so we can borrow it mutably from the GTK thread.
    let rx = Rc::new(RefCell::new(rx));

    app.connect_activate(move |app| {
        let window = build_ui(app);

        // Process incoming IPC commands on GTK main context.
        // We bridge from tokio via a periodic poll (simple and robust).
        // This avoids sending GTK objects across threads.
        let window_for_poll = window.clone();
        let rx_for_poll = rx.clone();

        glib::timeout_add_local(Duration::from_millis(50), move || {
            // Drain all pending commands quickly.
            let mut rx = rx_for_poll.borrow_mut();
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    UiCommand::Show => show_overlay(&window_for_poll),
                    UiCommand::Hide => hide_overlay(&window_for_poll),
                    UiCommand::Toggle => toggle_overlay(&window_for_poll),
                    UiCommand::Ping => {
                        // no-op in UI
                    }
                }
            }
            glib::ControlFlow::Continue
        });

        // Important: don't present on start; hidden by default.
        // window is kept alive by the application.
    });

    // Run GTK main loop (this blocks until exit).
    app.run();

    Ok(())
}
