//! Launcher layer-shell window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};
use waft_ui_gtk::widget_base::WidgetBase;
use waft_ui_gtk::widgets::app_result_row::{AppResultRowProps, ResultKind};
use waft_ui_gtk::widgets::search_pane::SearchPaneWidget;

use crate::fuzzy::build_highlight_markup;
use crate::ranking::RankedResult;

const LAUNCHER_ANIM_DURATION_MS: u32 = 100;
const LAUNCHER_SLIDE_OFFSET_PX: f64 = 8.0;

/// The main launcher window.
pub struct LauncherWindow {
    pub window: adw::ApplicationWindow,
    search_pane: SearchPaneWidget,
    /// Current ranked result list (parallel to displayed rows).
    results: Rc<RefCell<Vec<RankedResult>>>,
    #[allow(dead_code)] // Held to keep the gtk::Box alive; opacity driven via animation closure
    content: gtk::Box,
    animation: adw::TimedAnimation,
    animation_progress: Rc<Cell<f64>>,
    animating_hide: Rc<Cell<bool>>,
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
        window.set_keyboard_mode(KeyboardMode::OnDemand);
        // No anchors = centered on screen

        let search_pane = SearchPaneWidget::new("Search applications\u{2026}");

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        content.append(&search_pane.widget());
        window.set_content(Some(&content));

        // Prevent flash before first show animation
        content.set_opacity(0.0);

        let results: Rc<RefCell<Vec<RankedResult>>> = Rc::new(RefCell::new(Vec::new()));
        let animation_progress = Rc::new(Cell::new(0.0_f64));
        let animating_hide = Rc::new(Cell::new(false));

        // Create animation target that drives content opacity
        let content_for_anim = content.clone();
        let progress_for_anim = animation_progress.clone();
        let target = adw::CallbackAnimationTarget::new(move |value| {
            progress_for_anim.set(value);
            content_for_anim.set_opacity(value);
            let margin = ((1.0 - value) * LAUNCHER_SLIDE_OFFSET_PX) as i32;
            content_for_anim.set_margin_top(margin);
        });

        let animation = adw::TimedAnimation::builder()
            .widget(&content)
            .value_from(0.0)
            .value_to(1.0)
            .duration(LAUNCHER_ANIM_DURATION_MS)
            .target(&target)
            .build();

        // When animation finishes: hide window if animating hide, always resize
        let window_for_done = window.clone();
        let animating_hide_for_done = animating_hide.clone();
        animation.connect_done(move |_| {
            if animating_hide_for_done.get() {
                // Hide window BEFORE clearing animating_hide.
                // set_visible(false) may synchronously trigger is_active_notify;
                // the flag must still be true so that handler returns early.
                window_for_done.set_visible(false);
                animating_hide_for_done.set(false);
            }
            window_for_done.set_default_size(640, -1);
        });

        let widget = Self {
            window,
            search_pane,
            results,
            content,
            animation,
            animation_progress,
            animating_hide,
        };

        // Auto-hide on focus loss (hide, not quit — launcher stays in background)
        let anim_for_focus = widget.animation.clone();
        let progress_for_focus = widget.animation_progress.clone();
        let animating_hide_for_focus = widget.animating_hide.clone();
        widget.window.connect_is_active_notify(move |w| {
            if w.is_active() || animating_hide_for_focus.get() {
                return;
            }
            if !w.is_visible() {
                return;
            }
            animating_hide_for_focus.set(true);
            anim_for_focus.set_value_from(progress_for_focus.get());
            anim_for_focus.set_value_to(0.0);
            anim_for_focus.set_easing(adw::Easing::EaseInQuad);
            anim_for_focus.play();
        });

        // Keyboard navigation: Up/Down/Escape via EventControllerKey
        let controller = gtk::EventControllerKey::new();
        let pane_ref = widget.search_pane.clone();
        let anim_for_escape = widget.animation.clone();
        let progress_for_escape = widget.animation_progress.clone();
        let animating_hide_for_escape = widget.animating_hide.clone();
        let win_for_escape = widget.window.clone();
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
                if !win_for_escape.is_visible() || animating_hide_for_escape.get() {
                    return gtk::glib::Propagation::Stop;
                }
                animating_hide_for_escape.set(true);
                anim_for_escape.set_value_from(progress_for_escape.get());
                anim_for_escape.set_value_to(0.0);
                anim_for_escape.set_easing(adw::Easing::EaseInQuad);
                anim_for_escape.play();
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Page_Down => {
                pane_ref.select_next_page();
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Page_Up => {
                pane_ref.select_prev_page();
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Home => {
                pane_ref.select_first();
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::End => {
                pane_ref.select_last();
                gtk::glib::Propagation::Stop
            }
            _ => gtk::glib::Propagation::Proceed,
        });
        widget.window.add_controller(controller);

        widget
    }

    /// Show the launcher window with a fade-in animation.
    pub fn show(&self) {
        self.animating_hide.set(false);
        self.window.set_visible(true);
        self.window.present();
        self.animation.set_value_from(self.animation_progress.get());
        self.animation.set_value_to(1.0);
        self.animation.set_easing(adw::Easing::EaseOutQuad);
        self.animation.play();
    }

    /// Hide the launcher window with a fade-out animation.
    pub fn hide(&self) {
        if !self.window.is_visible() || self.animating_hide.get() {
            return;
        }
        self.animating_hide.set(true);
        self.animation.set_value_from(self.animation_progress.get());
        self.animation.set_value_to(0.0);
        self.animation.set_easing(adw::Easing::EaseInQuad);
        self.animation.play();
    }

    /// Reset search state for re-activation. Clears the entry and resets size.
    /// Does NOT set a loading state — the caller decides what to show.
    pub fn reset(&self) {
        self.search_pane.search_bar.clear();
        self.window.set_default_size(640, -1);
    }

    /// Update displayed results and resize window.
    pub fn set_results(&self, results: Vec<RankedResult>, query: &str) {
        let props: Vec<AppResultRowProps> = results
            .iter()
            .map(|r| {
                let positions = r.highlight_positions();
                let (name, icon, kind) = match r {
                    RankedResult::App { app, .. } => {
                        (app.name.clone(), app.icon.clone(), ResultKind::App)
                    }
                    RankedResult::Window { window, .. } => {
                        (window.title.clone(), window.app_id.to_lowercase(), ResultKind::Window)
                    }
                };
                let highlight_markup = if positions.is_empty() {
                    None
                } else {
                    Some(build_highlight_markup(&name, positions))
                };
                AppResultRowProps { name, icon, kind, highlight_markup }
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

    /// Get the `RankedResult` at the given result index.
    pub fn result_at(&self, index: usize) -> Option<RankedResult> {
        self.results.borrow().get(index).cloned()
    }
}
