//! Layer-shell window for displaying notification toasts.

use adw::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use waft_config::ToastPosition;

pub struct ToastWindow {
    pub window: adw::ApplicationWindow,
    pub container: gtk::Box,
}

impl ToastWindow {
    pub fn new(app: &adw::Application, position: ToastPosition) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Waft Toasts")
            .default_width(400)
            .default_height(0)
            .build();

        // CRITICAL: set_visible(false) BEFORE layer-shell init
        window.set_visible(false);

        // Make window background transparent
        window.add_css_class("transparent-window");

        Self::configure_layer_shell(&window, position);

        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();

        window.set_content(Some(&container));

        Self { window, container }
    }

    fn configure_layer_shell(window: &adw::ApplicationWindow, position: ToastPosition) {
        window.set_decorated(false);
        window.set_hide_on_close(true);
        window.set_modal(false);

        window.init_layer_shell();
        window.set_layer(Layer::Top); // Below main overlay (which uses Layer::Overlay)
        window.set_keyboard_mode(KeyboardMode::None);

        let (top, bottom, left, right) = position.anchors();
        window.set_anchor(Edge::Top, top);
        window.set_anchor(Edge::Bottom, bottom);
        window.set_anchor(Edge::Left, left);
        window.set_anchor(Edge::Right, right);
    }

    pub fn trigger_resize(&self) {
        let window = self.window.clone();
        gtk::glib::idle_add_local_once(move || {
            window.set_default_size(400, -1);
        });
    }

    pub fn update_visibility(&self, has_toasts: bool) {
        if has_toasts {
            self.window.set_visible(true);
            // Let window grow with content
            self.trigger_resize();
        } else {
            // Reset to 0 height when hiding
            self.window.set_default_size(400, 0);
            self.window.set_visible(false);
        }
    }
}
