//! Pure GTK4 Main Window widget.
//!
//! The main overlay window that hosts the application UI.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk4_layer_shell::LayerShell;
use log::debug;

use crate::menu_state::MenuStore;
use crate::plugin::{Slot, Widget};
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
const OVERLAY_SLIDE_OFFSET_PX: f64 = 20.0;
const OVERLAY_ANIM_DURATION_MS: u32 = 200;

/// Synchronize a GTK container's children with a new list of widgets.
///
/// Uses diffing to avoid unnecessary remounting:
/// - Widgets present in both old and new lists are kept in place
/// - Only widgets no longer present are removed
/// - Only new widgets are added
/// - Reordering uses `reorder_child_after()` to avoid remounting
fn sync_slot_widgets(container: &gtk::Box, new_widgets: &[Arc<Widget>]) {
    // Build set of new widget IDs for quick lookup
    let new_ids: HashSet<&str> = new_widgets.iter().map(|w| w.id.as_str()).collect();

    // Build map of current children by widget name (which stores the ID)
    let mut current_children: Vec<(String, gtk::Widget)> = Vec::new();
    let mut child = container.first_child();
    while let Some(widget) = child {
        let id = widget.widget_name().to_string();
        let next = widget.next_sibling();
        current_children.push((id, widget));
        child = next;
    }

    // Remove widgets that are no longer in the new list
    for (id, widget) in &current_children {
        if !new_ids.contains(id.as_str()) {
            container.remove(widget);
            debug!("[sync_slot] Removed widget: {}", id);
        }
    }

    // Build set of current IDs (after removal)
    let current_ids: HashSet<String> = current_children
        .iter()
        .filter(|(id, _)| new_ids.contains(id.as_str()))
        .map(|(id, _)| id.clone())
        .collect();

    // Add new widgets and reorder
    let mut prev_widget: Option<gtk::Widget> = None;
    for new_widget in new_widgets {
        let id = &new_widget.id;

        if current_ids.contains(id) {
            // Widget exists - get reference from container
            let mut child = container.first_child();
            while let Some(widget) = child {
                if widget.widget_name() == id.as_str() {
                    // Reorder if needed
                    if let Some(ref prev) = prev_widget {
                        if widget.prev_sibling().as_ref() != Some(prev) {
                            container.reorder_child_after(&widget, Some(prev));
                            debug!("[sync_slot] Reordered widget: {}", id);
                        }
                    } else if widget.prev_sibling().is_some() {
                        // Should be first, but isn't
                        container.reorder_child_after(&widget, None::<&gtk::Widget>);
                        debug!("[sync_slot] Moved widget to first: {}", id);
                    }
                    prev_widget = Some(widget);
                    break;
                }
                child = widget.next_sibling();
            }
        } else {
            // New widget - set widget name to ID for future diffing and append
            new_widget.el.set_widget_name(id);
            if let Some(ref prev) = prev_widget {
                container.insert_child_after(&new_widget.el, Some(prev));
            } else {
                container.prepend(&new_widget.el);
            }
            prev_widget = Some(new_widget.el.clone());
            debug!("[sync_slot] Added new widget: {}", id);
        }
    }
}

/// Input messages for the main window.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // RequestHide is part of the API for future use
pub enum MainWindowInput {
    ShowOverlay,
    HideOverlay,
    ToggleOverlay,
    StopApp,
    RequestHide,
}

/// References to the slot containers for dynamic widget synchronization.
struct SlotContainers {
    header_box: gtk::Box,
    actions_box: gtk::Box,
    info_col: gtk::Box,
    controls_col: gtk::Box,
    feature_grid: FeatureGridWidget,
}

/// Pure GTK4 main window.
pub struct MainWindowWidget {
    pub window: adw::ApplicationWindow,
    pub animation: adw::TimedAnimation,
    pub animation_progress: Rc<Cell<f64>>,
    pub animating_hide: Rc<Cell<bool>>,
    on_stop: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_hide_complete: Rc<RefCell<Option<Box<dyn Fn()>>>>,
}

impl MainWindowWidget {
    /// Create a new main window with the given registry.
    pub fn new(app: &adw::Application, registry: &Arc<PluginRegistry>) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(&crate::i18n::t("app-title"))
            .default_width(OVERLAY_WIDTH_PX)
            .build();

        // Must be set before layer shell init and content build so the window
        // is never realized in a visible state.  Previously the weather
        // plugin's busy-poll starved the glib main loop, hiding this race.
        window.set_visible(false);

        // Configure layer shell
        Self::configure_layer_shell(&window);

        // Build content
        let menu_store = registry.menu_store();
        let (clip, containers) = Self::build_content(&window, registry, menu_store.clone());

        // Subscribe to widget changes for dynamic updates
        let header_box = containers.header_box.clone();
        let actions_box = containers.actions_box.clone();
        let info_col = containers.info_col.clone();
        let controls_col = containers.controls_col.clone();
        let feature_grid = Rc::new(containers.feature_grid);
        let registry_for_sync = registry.clone();
        registry.subscribe_widgets(move || {
            debug!("[main_window] Widget change detected, syncing slots");
            let header_widgets = registry_for_sync.get_widgets_for_slot(Slot::Header);
            let actions_widgets = registry_for_sync.get_widgets_for_slot(Slot::Actions);
            let info_widgets = registry_for_sync.get_widgets_for_slot(Slot::Info);
            let controls_widgets = registry_for_sync.get_widgets_for_slot(Slot::Controls);

            sync_slot_widgets(&header_box, &header_widgets);
            sync_slot_widgets(&actions_box, &actions_widgets);
            sync_slot_widgets(&info_col, &info_widgets);
            sync_slot_widgets(&controls_col, &controls_widgets);

            // Sync feature toggles
            let toggles = registry_for_sync.get_all_feature_toggles();
            feature_grid.sync_toggles(&toggles);

            trigger_window_resize();
        });

        // Start in hidden state (fully transparent)
        clip.set_opacity(0.0);

        let on_stop: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let on_hide_complete: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let animating_hide = Rc::new(Cell::new(false));
        let animation_progress = Rc::new(Cell::new(0.0_f64));

        // Create animation callback that drives opacity + layer-shell margin slide.
        // Animating the layer-shell top margin moves the entire window surface at the
        // compositor level — no GTK layout recalculation, no size warnings.
        let clip_for_anim = clip.clone();
        let window_for_anim = window.clone();
        let progress_for_anim = animation_progress.clone();
        let target = adw::CallbackAnimationTarget::new(move |value| {
            progress_for_anim.set(value);
            clip_for_anim.set_opacity(value);
            let margin = OVERLAY_TOP_OFFSET_PX as f64 - (1.0 - value) * OVERLAY_SLIDE_OFFSET_PX;
            window_for_anim.set_margin(gtk4_layer_shell::Edge::Top, margin as i32);
        });

        let animation = adw::TimedAnimation::builder()
            .widget(&clip)
            .value_from(0.0)
            .value_to(1.0)
            .duration(OVERLAY_ANIM_DURATION_MS)
            .target(&target)
            .build();

        // When animation finishes, hide the window if we were animating a hide
        let window_ref = window.clone();
        let animating_hide_ref = animating_hide.clone();
        let on_hide_complete_ref = on_hide_complete.clone();
        animation.connect_done(move |_| {
            if animating_hide_ref.get() {
                animating_hide_ref.set(false);
                window_ref.set_visible(false);
                if let Some(ref cb) = *on_hide_complete_ref.borrow() {
                    cb();
                }
            }
            trigger_window_resize();
        });

        // Setup keyboard controller for Escape
        let animation_ref = animation.clone();
        let progress_ref = animation_progress.clone();
        let animating_hide_ref = animating_hide.clone();
        let controller = gtk::EventControllerKey::new();
        controller.connect_key_pressed(move |_c, key, _code, _state| {
            if key == gtk::gdk::Key::Escape {
                animating_hide_ref.set(true);
                animation_ref.set_value_from(progress_ref.get());
                animation_ref.set_value_to(0.0);
                animation_ref.set_easing(adw::Easing::EaseInCubic);
                animation_ref.play();
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        window.add_controller(controller);

        // Track when a popover recently closed - this prevents is_active_notify from
        // hiding immediately when the popover close callback hasn't finished processing.
        let popover_recently_closed = Rc::new(Cell::new(false));

        // Subscribe to menu store for popover close events.
        // When a popover closes, we defer the hide decision to let focus settle.
        let animation_for_popover = animation.clone();
        let progress_for_popover = animation_progress.clone();
        let animating_hide_for_popover = animating_hide.clone();
        let window_for_popover = window.clone();
        let menu_store_for_popover = menu_store.clone();
        let popover_recently_closed_for_sub = popover_recently_closed.clone();

        // Track previous popover state to detect closes
        let prev_had_popover = Rc::new(Cell::new(false));

        menu_store.subscribe(move || {
            let state = menu_store_for_popover.get_state();
            let has_popover = state.active_popover_id.is_some();
            let had_popover = prev_had_popover.get();
            prev_had_popover.set(has_popover);

            // A popover just closed
            if had_popover && !has_popover {
                // Set flag so is_active_notify knows to wait
                popover_recently_closed_for_sub.set(true);

                let window_ref = window_for_popover.clone();
                let animating_hide = animating_hide_for_popover.clone();
                let animation = animation_for_popover.clone();
                let progress = progress_for_popover.clone();
                let recently_closed_flag = popover_recently_closed_for_sub.clone();

                // Defer to let focus settle
                gtk::glib::idle_add_local_once(move || {
                    // Clear the flag - we're now handling the deferred decision
                    recently_closed_flag.set(false);

                    // If window regained focus, don't hide
                    if window_ref.is_active() || animating_hide.get() {
                        return;
                    }
                    // Window lost focus to external app, hide overlay
                    animating_hide.set(true);
                    animation.set_value_from(progress.get());
                    animation.set_value_to(0.0);
                    animation.set_easing(adw::Easing::EaseInCubic);
                    animation.play();
                });
            }
        });

        // Hide on focus loss, but not when:
        // 1. A popover is currently open
        // 2. A popover just closed (let the deferred handler deal with it)
        let animation_ref = animation.clone();
        let progress_ref = animation_progress.clone();
        let animating_hide_ref = animating_hide.clone();
        let menu_store_for_focus = menu_store;
        let popover_recently_closed_for_focus = popover_recently_closed;
        window.connect_is_active_notify(move |w| {
            if w.is_active() {
                return;
            }

            if animating_hide_ref.get() {
                return;
            }

            // Check if any popover is open - if so, let the popover close handler deal with it
            let state = menu_store_for_focus.get_state();
            if state.active_popover_id.is_some() {
                return;
            }

            // Check if a popover just closed - let the deferred handler deal with it
            if popover_recently_closed_for_focus.get() {
                return;
            }

            // No popovers involved, hide immediately
            animating_hide_ref.set(true);
            animation_ref.set_value_from(progress_ref.get());
            animation_ref.set_value_to(0.0);
            animation_ref.set_easing(adw::Easing::EaseInCubic);
            animation_ref.play();
        });

        // Set up resize callback for layer-shell window resizing
        let window_clone = window.clone();
        set_window_resize_callback(move || {
            // For layer-shell windows, setting default height to -1 triggers
            // GTK to recalculate size based on content.
            window_clone.set_default_size(OVERLAY_WIDTH_PX, -1);
        });

        debug!("Created main window");

        Self {
            window,
            animation,
            animation_progress,
            animating_hide,
            on_stop,
            on_hide_complete,
        }
    }

    /// Set the callback for app stop requests.
    pub fn connect_stop<F>(&self, callback: F)
    where
        F: Fn() + 'static,
    {
        *self.on_stop.borrow_mut() = Some(Box::new(callback));
    }

    /// Set the callback invoked after the hide animation completes.
    pub fn connect_hide_complete<F>(&self, callback: F)
    where
        F: Fn() + 'static,
    {
        *self.on_hide_complete.borrow_mut() = Some(Box::new(callback));
    }

    /// Handle session lock: stop animations and hide window immediately.
    #[allow(dead_code)] // API for future session lock detection
    pub fn on_session_lock(&self) {
        // Stop any running animation immediately
        self.animation.pause();

        // Force window to hidden state without animation
        self.animating_hide.set(false);
        self.window.set_visible(false);

        debug!("[main_window] Session locked, window hidden");
    }

    /// Handle session unlock: reset animation state to clean values.
    #[allow(dead_code)] // API for future session lock detection
    pub fn on_session_unlock(&self) {
        // Reset animation state to initial values
        self.animation_progress.set(0.0);
        self.animating_hide.set(false);

        // Ensure window stays hidden (clean state after unlock)
        self.window.set_visible(false);

        debug!("[main_window] Session unlocked, state reset");
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

    pub fn apply_css() {
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

            .clock-btn.clickable {{
                padding: 8px;
            }}

            .clock-btn.clickable:hover {{
                background-color: alpha(@window_fg_color, 0.1);
            }}

            .clock-btn.clickable:active {{
                background-color: alpha(@window_fg_color, 0.2);
            }}

            .feature-toggle,
            .feature-toggle-expandable {{
              margin: 8px 0 4px;
            }}

            /* Unified feature toggle - default (non-expandable) state */
            .feature-toggle .toggle-main {{
                background: @card_bg_color;
                border-radius: 28px;
                min-height: 48px;
                padding: 2px 20px 2px 12px;
            }}

            .feature-toggle .toggle-main:hover {{
              background-color: color-mix(
                in srgb,
                @window_fg_color 10%,
                @card_bg_color
              );
            }}

            .feature-toggle .toggle-main .title {{
              font-weight: 600;
            }}

            .feature-toggle .toggle-main .details {{
              font-size: 14px;
              margin: 0;
              padding: 0;
            }}

            .feature-toggle.active .toggle-main {{
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 33%,
                  @card_bg_color
                );
                color: var(--button_bg_color);
            }}

            .feature-toggle.active .toggle-main:hover {{
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 66%,
                  @card_bg_color
                );
            }}

            /* Busy/loading state - use outline to avoid layout jump */
            .feature-toggle.busy .toggle-main {{
                outline: 2px solid alpha(@accent_bg_color, 0.6);
                outline-offset: -2px;
            }}

            .feature-toggle-expandable .toggle-main,
            .feature-toggle-expandable .toggle-expand {{
                background: @card_bg_color;
                min-height: 48px;
                border-radius: 0;
            }}

            .feature-toggle-expandable .toggle-main {{
                border-radius: 28px 0 0 28px;
                padding: 2px 12px 2px 12px;
            }}

            .feature-toggle-expandable .toggle-expand {{
                border-radius: 0 28px 28px 0;
                padding: 2px 16px 2px 8px;
                min-width: 32px;
                border-left: 1px solid alpha(@window_fg_color, 0.1);
            }}

            .feature-toggle-expandable .toggle-main:hover,
            .feature-toggle-expandable .toggle-expand:hover {{
                background-color: color-mix(in srgb, @card_bg_color 80%, @window_fg_color);
            }}

            .feature-toggle-expandable.active .toggle-main,
            .feature-toggle-expandable.active .toggle-expand {{
                background-color: @accent_bg_color;
                color: var(--button_bg_color);
            }}

            .feature-toggle-expandable.active .toggle-main {{
              background-color: color-mix(
                in srgb,
                @accent_bg_color 15%,
                @card_bg_color
              );
            }}

            .feature-toggle-expandable.active .toggle-expand {{
                border-left-color: color-mix(in srgb, @accent_bg_color 50%, @card_bg_color);
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 20%,
                  @card_bg_color
                );
            }}

            .feature-toggle-expandable.active .toggle-main:hover {{
              background-color: color-mix(
                in srgb,
                @accent_bg_color 66%,
                @card_bg_color
              );
            }}

            .feature-toggle-expandable.active .toggle-expand:hover {{
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 50%,
                  @card_bg_color
                );
            }}

            .feature-toggle-expandable.busy .toggle-main,
            .feature-toggle-expandable.busy .toggle-expand {{
                outline: 2px solid alpha(@accent_bg_color, 0.6);
                outline-offset: -2px;
            }}

            /* Unified feature toggle - expandable state */
            .feature-toggle.expandable .toggle-main {{
                border-radius: 28px 0 0 28px;
                padding: 2px 12px 2px 12px;
            }}

            .feature-toggle.expandable .toggle-expand {{
                background: @card_bg_color;
                border-radius: 0 28px 28px 0;
                padding: 2px 16px 2px 8px;
                min-height: 48px;
                min-width: 32px;
                border-left: 1px solid alpha(@window_fg_color, 0.1);
            }}

            .feature-toggle.expandable .toggle-expand:hover {{
                background-color: color-mix(in srgb, @card_bg_color 80%, @window_fg_color);
            }}

            .feature-toggle.expandable.active .toggle-expand {{
                border-left-color: color-mix(in srgb, @accent_bg_color 50%, @card_bg_color);
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 20%,
                  @card_bg_color
                );
            }}

            .feature-toggle.expandable.active .toggle-expand:hover {{
                background-color: color-mix(
                  in srgb,
                  @accent_bg_color 50%,
                  @card_bg_color
                );
            }}

            .feature-toggle-expandable .toggle-main .title {{
                font-weight: 600;
            }}

            /* Menu chevron styling */
            .menu-chevron {{
                -gtk-icon-transform: rotate(-90deg);
                transition: -gtk-icon-transform 200ms;
            }}

            .menu-chevron.expanded {{
                -gtk-icon-transform: rotate(0deg);
            }}

            /* Menu row styling */
            .feature-grid-menu-row {{
                background: @card_bg_color;
                border-radius: 0 0 16px 16px;
                padding: 0;
                margin: 0 0 8px 0;
            }}

            /* Device menu styling */
            .device-menu {{
                padding: 0 0;
            }}

            .device-row {{
                padding: 8px 12px;
                border-radius: 8px;
            }}

            .device-row:hover {{
                background-color: alpha(@window_fg_color, 0.05);
            }}

            .device-switch {{
                margin: 0;
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

            /* Slider control styling */
            .slider-row {{
                background: @card_bg_color;
                border-radius: 28px;
                min-height: 48px;
                padding: 0;
                margin: 0;
            }}

            .slider-row:hover {{
            }}

            .slider-icon {{
                background: transparent;
                border-radius: 50%;
                min-width: 48px;
                min-height: 48px;
                padding: 0;
            }}

            .slider-icon:hover {{
                background-color: alpha(@window_fg_color, 0.1);
            }}

            .slider-scale {{
                min-width: 120px;
                margin: 0 8px;
            }}

            .slider-scale trough {{
                min-height: 6px;
                border-radius: 3px;
                background: alpha(@window_fg_color, 0.15);
            }}

            .slider-scale highlight {{
                min-height: 6px;
                border-radius: 3px;
                background: @accent_bg_color;
            }}

            .slider-scale slider {{
                min-width: 18px;
                min-height: 18px;
                border-radius: 50%;
                background: @window_bg_color;
                box-shadow: 0 1px 3px alpha(black, 0.3);
            }}

            .slider-expand {{
                background: transparent;
                border-radius: 50%;
                min-width: 48px;
                min-height: 48px;
                padding: 0;
            }}

            .slider-expand:hover {{
                background-color: alpha(@window_fg_color, 0.1);
            }}

            .slider-row.muted {{
                opacity: 0.7;
            }}

            .slider-row.muted .slider-icon {{
                opacity: 0.5;
            }}

            /* Audio device menu styling */
            .audio-device-menu {{
                padding: 4px 0;
            }}

            .audio-device-row {{
                background: transparent;
                border-radius: 8px;
                padding: 8px 12px;
                margin: 2px 0;
            }}

            .audio-device-row:hover {{
                background-color: alpha(@window_fg_color, 0.05);
            }}

            .audio-device-row.default {{
                background-color: alpha(@accent_bg_color, 0.15);
            }}

            .audio-device-row.default:hover {{
                background-color: alpha(@accent_bg_color, 0.25);
            }}

            .audio-device-icon {{
                opacity: 0.8;
            }}

            .audio-device-name {{
                font-weight: 400;
            }}

            .audio-device-secondary-icon {{
                opacity: 0.6;
            }}

            .audio-device-check {{
                color: @accent_bg_color;
            }}

            /* Agenda event cards */
            .agenda-event-card {{
                background: @card_bg_color;
                border-radius: 12px;
                padding: 6px 12px;
                margin: 2px 0;
            }}

            /* Dim past events */
            .agenda-event-past {{
                opacity: 0.5;
            }}

            /* Ongoing event accent border */
            .agenda-event-ongoing {{
                border-left: 3px solid @accent_bg_color;
            }}

            /* Now divider */
            .agenda-divider-now {{
                margin: 6px 0;
                min-height: 2px;
                background: @accent_bg_color;
            }}

            /* Period separator */
            .agenda-period-separator {{
                margin: 8px 0 4px 0;
                opacity: 0.7;
            }}

            /* Meeting link buttons */
            .agenda-meeting-btn {{
                background: alpha(@accent_bg_color, 0.15);
                border-radius: 8px;
                padding: 2px 8px;
                min-height: 24px;
                font-size: 12px;
            }}

            .agenda-meeting-btn:hover {{
                background: alpha(@accent_bg_color, 0.3);
            }}

            .agenda-more-btn {{
                background: alpha(@accent_bg_color, 0.15);
                border-radius: 8px;
                padding: 2px 6px;
                min-height: 24px;
                min-width: 24px;
            }}

            .agenda-more-btn:hover {{
                background: alpha(@accent_bg_color, 0.3);
            }}

            .agenda-meeting-popover {{
                padding: 4px;
            }}

            .agenda-expand-btn {{
                min-width: 24px;
                min-height: 24px;
                padding: 0;
                opacity: 0.6;
            }}

            .agenda-expand-btn:hover {{
                opacity: 1.0;
            }}

            .agenda-event-details {{
                padding: 4px 12px 8px 12px;
                margin-left: 12px;
            }}

            .agenda-show-past-pill {{
                background: alpha(@window_fg_color, 0.1);
                border-radius: 8px;
                padding: 2px 8px;
                min-height: 24px;
                font-size: 12px;
                opacity: 0.5;
                border: none;
            }}

            .agenda-show-past-pill:checked {{
                background: alpha(@accent_bg_color, 0.15);
                color: @accent_bg_color;
                opacity: 1.0;
            }}

            .agenda-show-past-pill:hover {{
                background: alpha(@accent_bg_color, 0.3);
                opacity: 1.0;
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

    fn build_content(
        window: &adw::ApplicationWindow,
        registry: &Arc<PluginRegistry>,
        menu_store: Arc<MenuStore>,
    ) -> (gtk::Frame, SlotContainers) {
        let top_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(16)
            .build();
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

        // Add header widgets (set widget_name to ID for diffing)
        let header_widgets = registry.get_widgets_for_slot(Slot::Header);
        for w in &header_widgets {
            w.el.set_widget_name(&w.id);
            top_box.append(&w.el);
        }
        debug!("Appended header widgets {:?}", header_widgets.len());

        // Create actions box (right-aligned in header)
        let actions_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk::Align::End)
            .hexpand(true)
            .vexpand(false)
            .valign(gtk::Align::Start)
            .build();

        // Add actions widgets (set widget_name to ID for diffing)
        let actions_widgets = registry.get_widgets_for_slot(Slot::Actions);
        for w in &actions_widgets {
            w.el.set_widget_name(&w.id);
            actions_box.append(&w.el);
        }
        debug!("Appended actions widgets {:?}", actions_widgets.len());

        top_box.append(&actions_box);

        // Add info widgets (set widget_name to ID for diffing)
        let info_widgets = registry.get_widgets_for_slot(Slot::Info);
        for w in &info_widgets {
            w.el.set_widget_name(&w.id);
            left_col.append(&w.el);
        }
        debug!("Appended info widgets {:?}", info_widgets.len());

        // Add controls widgets (set widget_name to ID for diffing)
        let controls_widgets = registry.get_widgets_for_slot(Slot::Controls);
        for w in &controls_widgets {
            w.el.set_widget_name(&w.id);
            right_col.append(&w.el);
        }
        debug!("Appended controls widgets {:?}", controls_widgets.len());

        // Add feature toggles grid
        let toggles = registry.get_all_feature_toggles();
        let grid = FeatureGridWidget::new(toggles, menu_store);
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
        let max_height = match gtk::gdk::Display::default() {
            Some(display) => {
                match display.monitors().item(0) {
                    Some(monitor) => {
                        if let Some(monitor) = monitor.downcast_ref::<gtk::gdk::Monitor>() {
                            let geometry = monitor.geometry();
                            // Max height = screen height - top margin - bottom margin - some padding
                            geometry.height()
                                - OVERLAY_TOP_OFFSET_PX
                                - OVERLAY_BOTTOM_OFFSET_PX
                                - 48
                        } else {
                            800 // fallback
                        }
                    }
                    _ => {
                        800 // fallback
                    }
                }
            }
            _ => {
                800 // fallback
            }
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
        clip.set_overflow(gtk::Overflow::Visible);
        clip.set_child(Some(&scroller));

        window.set_content(Some(&clip));

        let containers = SlotContainers {
            header_box: top_box,
            actions_box,
            info_col: left_col,
            controls_col: right_col,
            feature_grid: grid,
        };

        (clip, containers)
    }
}
