//! Launcher layer-shell window.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};
use waft_protocol::entity::app::App;
use waft_protocol::urn::Urn;
use waft_ui_gtk::widget_base::WidgetBase;
use waft_ui_gtk::widgets::app_result_row::AppResultRowProps;
use waft_ui_gtk::widgets::search_pane::SearchPaneWidget;

use crate::ranking::RankedApp;

/// The main launcher window.
pub struct LauncherWindow {
    pub window: adw::ApplicationWindow,
    search_pane: SearchPaneWidget,
    /// Current ranked result list (parallel to displayed rows).
    results: Rc<RefCell<Vec<RankedApp>>>,
}

impl LauncherWindow {
    pub fn new(app: &adw::Application) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .default_width(640)
            .default_height(-1)
            .css_classes(["launcher-window"])
            .build();

        // Layer shell setup
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(KeyboardMode::Exclusive);
        // No anchors = centered on screen

        let search_pane = SearchPaneWidget::new("Search applications\u{2026}");

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        content.append(&search_pane.widget());
        window.set_content(Some(&content));

        let results: Rc<RefCell<Vec<RankedApp>>> = Rc::new(RefCell::new(Vec::new()));

        let widget = Self {
            window,
            search_pane,
            results,
        };

        // Auto-hide on focus loss (hide, not quit — launcher stays in background)
        widget.window.connect_is_active_notify(|w| {
            if !w.is_active() {
                w.set_visible(false);
            }
        });

        // Keyboard navigation: Up/Down/Escape via EventControllerKey
        let controller = gtk::EventControllerKey::new();
        let pane_ref = widget.search_pane.clone();
        let win_ref = widget.window.clone();
        controller.connect_key_pressed(move |_c, key, _code, _mods| match key {
            gtk::gdk::Key::Up => {
                pane_ref.select_prev();
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Down => {
                pane_ref.select_next();
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Escape => {
                // Fallback: Escape when focus is not inside the search entry.
                // When focus is in the entry, stop-search fires first and reaches
                // SearchPaneOutput::Stopped before this handler.
                win_ref.set_visible(false);
                gtk::glib::Propagation::Stop
            }
            _ => gtk::glib::Propagation::Proceed,
        });
        widget.window.add_controller(controller);

        widget
    }

    /// Reset search state for re-activation. Clears the entry and resets size.
    /// Does NOT set a loading state — the caller decides what to show.
    pub fn reset(&self) {
        self.search_pane.search_bar.clear();
        self.window.set_default_size(640, -1);
    }

    /// Update displayed results and resize window.
    pub fn set_results(&self, results: Vec<RankedApp>, query: &str) {
        let props: Vec<AppResultRowProps> = results
            .iter()
            .map(|r| AppResultRowProps {
                name: r.app.name.clone(),
                icon: r.app.icon.clone(),
                description: r.app.description.clone(),
            })
            .collect();
        *self.results.borrow_mut() = results;
        self.search_pane.set_results(props, query);
        // Trigger layer-shell resize
        self.window.set_default_size(640, -1);
    }

    /// Get the search pane (to connect output callbacks).
    pub fn search_pane(&self) -> &SearchPaneWidget {
        &self.search_pane
    }

    /// Grab focus into the search entry.
    pub fn grab_focus(&self) {
        self.search_pane.grab_focus();
    }

    /// Get the `RankedApp` at the given result index.
    pub fn result_at(&self, index: usize) -> Option<(Urn, App)> {
        self.results
            .borrow()
            .get(index)
            .map(|r| (r.urn.clone(), r.app.clone()))
    }
}
