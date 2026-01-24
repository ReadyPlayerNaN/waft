//! Pure GTK4 Main Window widget.
//!
//! The main overlay window that hosts the application UI.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::AdwApplicationWindowExt;
use gtk::prelude::*;
use gtk4_layer_shell::LayerShell;
use log::debug;

use crate::plugin::Slot;
use crate::plugin_registry::PluginRegistry;
use crate::ui::feature_grid::FeatureGridWidget;

const OVERLAY_WIDTH_PX: i32 = 920;

// Thread-local callback for triggering window resize from anywhere in the app
thread_local! {
    static WINDOW_RESIZE_CALLBACK: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
}

/// Set the callback that will be invoked when window resize is needed.
pub fn set_window_resize_callback<F: Fn() + 'static>(callback: F) {
    WINDOW_RESIZE_CALLBACK.with(|cb| {
        *cb.borrow_mut() = Some(Rc::new(callback));
    });
}

/// Trigger window resize. Call this when content changes to recalculate layer-shell window size.
/// Uses idle_add to defer the resize until after the current event processing completes.
pub fn trigger_window_resize() {
    WINDOW_RESIZE_CALLBACK.with(|cb| {
        if let Some(ref callback) = *cb.borrow() {
            let callback = callback.clone();
            gtk::glib::idle_add_local_once(move || {
                debug!("[main_window] Triggering window resize");
                callback();
            });
        }
    });
}
const OVERLAY_TOP_OFFSET_PX: i32 = 16;
const OVERLAY_BOTTOM_OFFSET_PX: i32 = 16;
const OVERLAY_CORNER_RADIUS_PX: i32 = 8;

/// Input messages for the main window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainWindowInput {
    ShowOverlay,
    HideOverlay,
    ToggleOverlay,
    StopApp,
    RequestHide,
}

/// Pure GTK4 main window.
pub struct MainWindowWidget {
    pub window: adw::ApplicationWindow,
    on_stop: Rc<RefCell<Option<Box<dyn Fn()>>>>,
}

impl MainWindowWidget {
    /// Create a new main window with the given registry.
    pub fn new(app: &adw::Application, registry: &Arc<PluginRegistry>) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("sacrebleui (overlay host)")
            .default_width(OVERLAY_WIDTH_PX)
            .build();

        // Configure layer shell
        Self::configure_layer_shell(&window);

        // Apply CSS
        Self::apply_css();

        // Build content
        Self::build_content(&window, registry);

        let on_stop: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));

        // Setup keyboard controller for Escape
        let window_ref = window.clone();
        let controller = gtk::EventControllerKey::new();
        controller.connect_key_pressed(move |_c, key, _code, _state| {
            if key == gtk::gdk::Key::Escape {
                window_ref.set_visible(false);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        window.add_controller(controller);

        // Hide on focus loss
        let window_ref = window.clone();
        window.connect_is_active_notify(move |w| {
            if !w.is_active() {
                window_ref.set_visible(false);
            }
        });

        // Set up resize callback for layer-shell window resizing
        let window_clone = window.clone();
        set_window_resize_callback(move || {
            // For layer-shell windows, setting default height to -1 triggers
            // GTK to recalculate size based on content.
            window_clone.set_default_size(OVERLAY_WIDTH_PX, -1);
        });

        debug!("Created main window");

        Self { window, on_stop }
    }

    /// Set the callback for app stop requests.
    pub fn connect_stop<F>(&self, callback: F)
    where
        F: Fn() + 'static,
    {
        *self.on_stop.borrow_mut() = Some(Box::new(callback));
    }

    fn configure_layer_shell(window: &adw::ApplicationWindow) {
        window.set_decorated(false);
        window.set_hide_on_close(true);
        window.set_modal(false);

        window.init_layer_shell();
        window.set_layer(gtk4_layer_shell::Layer::Overlay);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);

        window.set_anchor(gtk4_layer_shell::Edge::Top, true);
        window.set_anchor(gtk4_layer_shell::Edge::Left, false);
        window.set_anchor(gtk4_layer_shell::Edge::Right, false);
        window.set_anchor(gtk4_layer_shell::Edge::Bottom, false);

        window.set_margin(gtk4_layer_shell::Edge::Top, OVERLAY_TOP_OFFSET_PX);
        window.set_margin(gtk4_layer_shell::Edge::Bottom, OVERLAY_BOTTOM_OFFSET_PX);
    }

    fn apply_css() {
        let css = format!(
            r#"
            window,
            .background {{
                background: transparent;
            }}

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

            .feature-toggle {{
                background: @card_bg_color;
                border-radius: 28px;
                min-height: 48px;
                padding: 2px 20px 2px 12px;
                margin: 8px 0;
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

            .toast {{
              background-color: @window_bg_color;
              margin-top: 8px;
            }}

            .toast:hover {{
              background-color: color-mix(
                in srgb,
                @accent_bg_color 20%,
                @window_bg_color
              );
            }}

            .notification-progress {{
                min-height: 2px;
                margin: 0 16px;
            }}

            .notification-progress trough {{
                background: transparent;
                min-height: 2px;
            }}

            .notification-progress progress {{
                background: alpha(@window_fg_color, 0.2);
                min-height: 2px;
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
    }

    fn build_content(window: &adw::ApplicationWindow, registry: &Arc<PluginRegistry>) {
        let top_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        top_box.set_hexpand(true);

        let top_box_divider = gtk::Separator::new(gtk::Orientation::Horizontal);
        top_box_divider.set_hexpand(true);

        let left_col = gtk::Box::builder()
            .hexpand(true)
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .width_request(480)
            .build();

        let right_col = gtk::Box::builder()
            .hexpand(true)
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .width_request(480)
            .build();

        // Add header widgets
        let header_widgets = registry.get_widgets_for_slot(Slot::Header);
        for w in &header_widgets {
            top_box.append(&w.el);
        }
        debug!("Appended header widgets {:?}", header_widgets.len());

        // Add info widgets
        let info_widgets = registry.get_widgets_for_slot(Slot::Info);
        for w in &info_widgets {
            left_col.append(&w.el);
        }
        debug!("Appended info widgets {:?}", info_widgets.len());

        // Add feature toggles grid
        let toggles = registry.get_all_feature_toggles();
        let grid = FeatureGridWidget::new(toggles);
        right_col.append(grid.widget());
        debug!("Appended feature toggles widgets");

        let main_vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
        main_vbox.set_margin_start(0);
        main_vbox.set_margin_end(0);
        main_vbox.set_margin_top(0);
        main_vbox.set_margin_bottom(0);

        main_vbox.append(&top_box);
        main_vbox.append(&top_box_divider);

        let content_row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        content_row.set_hexpand(true);

        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spacer.set_hexpand(true);

        content_row.append(&left_col);
        content_row.append(&spacer);
        content_row.append(&right_col);

        main_vbox.append(&content_row);

        // Calculate max height based on monitor size
        let max_height = if let Some(display) = gtk::gdk::Display::default() {
            if let Some(monitor) = display.monitors().item(0) {
                if let Some(monitor) = monitor.downcast_ref::<gtk::gdk::Monitor>() {
                    let geometry = monitor.geometry();
                    // Max height = screen height - top margin - bottom margin - some padding
                    geometry.height() - OVERLAY_TOP_OFFSET_PX - OVERLAY_BOTTOM_OFFSET_PX - 48
                } else {
                    800 // fallback
                }
            } else {
                800 // fallback
            }
        } else {
            800 // fallback
        };

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_vscrollbar_policy(gtk::PolicyType::Automatic);
        scroller.set_propagate_natural_height(true);
        scroller.set_propagate_natural_width(true);
        scroller.set_max_content_height(max_height);
        scroller.set_hexpand(true);
        scroller.set_child(Some(&main_vbox));

        let clip = gtk::Frame::new(None);
        clip.add_css_class("relm4-overlay-surface");
        clip.set_hexpand(true);
        clip.set_overflow(gtk::Overflow::Hidden);
        clip.set_child(Some(&scroller));

        window.set_content(Some(&clip));
    }
}
