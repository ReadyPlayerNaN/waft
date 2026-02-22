//! Layout renderer -- converts a LayoutNode tree into a GTK widget tree.
//!
//! Handles purpose-built entity components (`LayoutNode::Component`).

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use log::{debug, warn};

/// Type alias for rebuild callback slot to reduce complexity.
type RebuildSlot = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

use crate::calendar_selection::CalendarSelectionStore;
use crate::components::agenda::AgendaComponent;
use crate::components::audio_sliders::AudioSlidersComponent;
use crate::components::battery::BatteryComponent;
use crate::components::brightness_sliders::BrightnessSlidersComponent;
use crate::components::calendar_grid::CalendarComponent;
use crate::components::clock::ClockComponent;
use crate::components::events::EventsComponent;
use crate::components::keyboard_layout::KeyboardLayoutComponent;
use crate::components::settings_button::SettingsButtonComponent;
use crate::components::notification_list::NotificationsComponent;
use crate::components::session_actions::SessionActionsComponent;
use crate::components::system_actions::SystemActionsComponent;
use crate::components::toggles::backup::BackupToggle;
use crate::components::toggles::bluetooth::BluetoothToggles;
use crate::components::toggles::caffeine::caffeine_toggle;
use crate::components::toggles::dark_mode::dark_mode_toggle;
use crate::components::toggles::dnd::dnd_toggle;
use crate::components::toggles::network::NetworkManagerToggles;
use crate::components::toggles::night_light::night_light_toggle;
use crate::ui::feature_toggles::simple_toggle::SimpleToggle;
use crate::components::weather::WeatherComponent;
use crate::layout::model::LayoutNode;
use crate::layout::types::WidgetFeatureToggle;
use crate::menu_state::MenuStore;
use crate::ui::main_window::trigger_window_resize;
use waft_client::{EntityActionCallback, EntityStore};
use waft_ui_gtk::widgets::feature_grid::{FeatureGridItem, FeatureGridWidget};

/// Shared context for the layout renderer, providing access to entity store,
/// action routing, and component lifetime management.
pub struct RenderContext {
    pub store: Rc<EntityStore>,
    pub action_callback: EntityActionCallback,
    pub calendar_selection: Rc<CalendarSelectionStore>,
    /// Keeps components alive (entity subscriptions, toggle state).
    keep_alive: RefCell<Vec<Box<dyn std::any::Any>>>,
}

impl RenderContext {
    pub fn new(
        store: Rc<EntityStore>,
        action_callback: EntityActionCallback,
        calendar_selection: Rc<CalendarSelectionStore>,
    ) -> Self {
        Self {
            store,
            action_callback,
            calendar_selection,
            keep_alive: RefCell::new(Vec::new()),
        }
    }
}

/// The result of rendering a layout tree into GTK widgets.
///
/// Holds the root GTK widget and the render context that keeps entity components alive.
pub struct RenderedLayout {
    pub root: gtk::Widget,
    _context: Rc<RenderContext>,
}

/// Render a LayoutNode tree into a GTK widget tree.
pub fn render_layout(
    tree: &LayoutNode,
    ctx: &Rc<RenderContext>,
    menu_store: &Rc<MenuStore>,
) -> RenderedLayout {
    let root = render_node(tree, ctx, menu_store);

    RenderedLayout {
        root,
        _context: ctx.clone(),
    }
}

fn render_layout_box(
    orientation: gtk::Orientation,
    halign: &Option<String>,
    children: &[LayoutNode],
    ctx: &Rc<RenderContext>,
    menu_store: &Rc<MenuStore>,
) -> gtk::Widget {
    let container = gtk::Box::new(orientation, 12);

    if let Some(align_str) = halign {
        let align = match align_str.as_str() {
            "start" => gtk::Align::Start,
            "end" => gtk::Align::End,
            "center" => gtk::Align::Center,
            "fill" => gtk::Align::Fill,
            _ => gtk::Align::Fill,
        };
        container.set_halign(align);

        // When halign is "end", also set hexpand so it pushes to the right
        if align_str == "end" {
            container.set_hexpand(true);
            container.set_valign(gtk::Align::Start);
        }
    }

    for child in children {
        let widget = render_node(child, ctx, menu_store);
        container.append(&widget);
    }
    container.upcast()
}

fn render_node(
    node: &LayoutNode,
    ctx: &Rc<RenderContext>,
    menu_store: &Rc<MenuStore>,
) -> gtk::Widget {
    match node {
        LayoutNode::Overview { children } => {
            let vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
            for child in children {
                let widget = render_node(child, ctx, menu_store);
                vbox.append(&widget);
            }
            vbox.upcast()
        }

        LayoutNode::Header { children } => {
            let hbox = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(16)
                .hexpand(true)
                .build();
            for child in children {
                let widget = render_node(child, ctx, menu_store);
                hbox.append(&widget);
            }
            hbox.upcast()
        }

        LayoutNode::TwoColumns { children } => {
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 24);
            hbox.set_hexpand(true);

            let mut col_iter = children.iter();

            // First column
            if let Some(child) = col_iter.next() {
                let widget = render_node(child, ctx, menu_store);
                if let Some(w) = widget.downcast_ref::<gtk::Box>() {
                    w.set_hexpand(true);
                    w.set_width_request(480);
                }
                hbox.append(&widget);
            }

            // Spacer between columns
            let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
            spacer.set_hexpand(true);
            hbox.append(&spacer);

            // Second column
            if let Some(child) = col_iter.next() {
                let widget = render_node(child, ctx, menu_store);
                if let Some(w) = widget.downcast_ref::<gtk::Box>() {
                    w.set_hexpand(true);
                    w.set_width_request(480);
                }
                hbox.append(&widget);
            }

            hbox.upcast()
        }

        LayoutNode::Box { halign, children } => render_layout_box(
            gtk::Orientation::Vertical,
            halign,
            children,
            ctx,
            menu_store,
        ),

        LayoutNode::Row { halign, children } => render_layout_box(
            gtk::Orientation::Horizontal,
            halign,
            children,
            ctx,
            menu_store,
        ),

        LayoutNode::Col { halign, children } => render_layout_box(
            gtk::Orientation::Vertical,
            halign,
            children,
            ctx,
            menu_store,
        ),

        LayoutNode::Divider => {
            let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
            sep.set_hexpand(true);
            sep.upcast()
        }

        LayoutNode::Component { name } => render_component(name, ctx, menu_store),

        LayoutNode::FeatureToggleGrid { children } => {
            render_feature_toggle_grid(children, ctx, menu_store)
        }

        // Legacy Widget and Unmatched nodes are no longer supported
        LayoutNode::Widget { id } => {
            warn!(
                "[renderer] Legacy <Widget id=\"{}\"> elements are no longer supported",
                id
            );
            gtk::Box::new(gtk::Orientation::Vertical, 0).upcast()
        }

        LayoutNode::Unmatched => {
            // Legacy unmatched catch-all no longer functional
            gtk::Box::new(gtk::Orientation::Vertical, 0).upcast()
        }
    }
}

/// Create a purpose-built entity component by name.
fn render_component(
    name: &str,
    ctx: &Rc<RenderContext>,
    menu_store: &Rc<MenuStore>,
) -> gtk::Widget {
    let mut keep = ctx.keep_alive.borrow_mut();
    match name {
        "Clock" => {
            let c = ClockComponent::new(&ctx.store);
            let w = c.widget();
            keep.push(Box::new(c));
            w
        }
        "Battery" => {
            let c = BatteryComponent::new(&ctx.store);
            let w = c.widget();
            keep.push(Box::new(c));
            w
        }
        "Weather" => {
            let c = WeatherComponent::new(&ctx.store);
            let w = c.widget();
            keep.push(Box::new(c));
            w
        }
        "KeyboardLayout" => {
            let c = KeyboardLayoutComponent::new(&ctx.store, &ctx.action_callback);
            let w = c.widget();
            keep.push(Box::new(c));
            w
        }
        "SettingsButton" => {
            let c = SettingsButtonComponent::new(&ctx.store, &ctx.action_callback);
            let w = c.widget();
            keep.push(Box::new(c));
            w
        }
        "SessionActions" => {
            let c = SessionActionsComponent::new(&ctx.action_callback);
            let w = c.widget();
            keep.push(Box::new(c));
            w
        }
        "SystemActions" => {
            let c = SystemActionsComponent::new(&ctx.action_callback);
            let w = c.widget();
            keep.push(Box::new(c));
            w
        }
        "Calendar" => {
            let c = CalendarComponent::new(&ctx.store, &ctx.calendar_selection);
            let w = c.widget().clone();
            keep.push(Box::new(c));
            w
        }
        "Agenda" => {
            let c = AgendaComponent::new(&ctx.store, menu_store, &ctx.calendar_selection, true);
            let w = c.widget().clone();
            keep.push(Box::new(c));
            w
        }
        "Events" => {
            let c = EventsComponent::new(&ctx.store, menu_store, &ctx.calendar_selection);
            let w = c.widget().clone();
            keep.push(Box::new(c));
            w
        }
        "NotificationList" => {
            let c = NotificationsComponent::new(&ctx.store, &ctx.action_callback, menu_store);
            let w = c.widget().clone();
            keep.push(Box::new(c));
            w
        }
        "AudioSliders" => {
            let c = AudioSlidersComponent::new(&ctx.store, &ctx.action_callback, menu_store);
            let w = c.widget().clone();
            keep.push(Box::new(c));
            w
        }
        "BrightnessSliders" => {
            let c = BrightnessSlidersComponent::new(&ctx.store, &ctx.action_callback);
            let w = c.widget().clone();
            keep.push(Box::new(c));
            w
        }
        _ => {
            warn!("[renderer] Unknown component: {name}");
            gtk::Box::new(gtk::Orientation::Vertical, 0).upcast()
        }
    }
}

/// Render a FeatureToggleGrid with Component children.
fn render_feature_toggle_grid(
    children: &[LayoutNode],
    ctx: &Rc<RenderContext>,
    menu_store: &Rc<MenuStore>,
) -> gtk::Widget {
    // Create toggle components and wire into a grid
    let resize_cb: Rc<dyn Fn()> = Rc::new(trigger_window_resize);
    let grid = Rc::new(FeatureGridWidget::new(
        Vec::new(),
        menu_store.clone(),
        Some(resize_cb),
    ));

    // Two-phase init for dynamic toggle rebuild cycle
    let rebuild_slot: RebuildSlot = Rc::new(RefCell::new(None));

    let slot_for_dynamic = rebuild_slot.clone();
    let dynamic_rebuild: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(ref rebuild) = *slot_for_dynamic.borrow() {
            rebuild();
        }
    });

    let mut dynamic_sources: Vec<Rc<dyn DynamicToggleSource>> = Vec::new();

    {
        let mut keep = ctx.keep_alive.borrow_mut();

        for child in children {
            match child {
                LayoutNode::Component { name } => match name.as_str() {
                    "DndToggle" => {
                        let t = Rc::new(dnd_toggle(
                            &ctx.store,
                            &ctx.action_callback,
                            dynamic_rebuild.clone(),
                        ));
                        dynamic_sources.push(t.clone());
                        keep.push(Box::new(t));
                    }
                    "CaffeineToggle" => {
                        let t = Rc::new(caffeine_toggle(
                            &ctx.store,
                            &ctx.action_callback,
                            dynamic_rebuild.clone(),
                        ));
                        dynamic_sources.push(t.clone());
                        keep.push(Box::new(t));
                    }
                    "DarkModeToggle" => {
                        let t = Rc::new(dark_mode_toggle(
                            &ctx.store,
                            &ctx.action_callback,
                            dynamic_rebuild.clone(),
                        ));
                        dynamic_sources.push(t.clone());
                        keep.push(Box::new(t));
                    }
                    "NightLightToggle" => {
                        let t = Rc::new(night_light_toggle(
                            &ctx.store,
                            &ctx.action_callback,
                            dynamic_rebuild.clone(),
                        ));
                        dynamic_sources.push(t.clone());
                        keep.push(Box::new(t));
                    }
                    "BluetoothToggles" => {
                        let bt = Rc::new(BluetoothToggles::new(
                            &ctx.store,
                            &ctx.action_callback,
                            menu_store,
                            dynamic_rebuild.clone(),
                        ));
                        dynamic_sources.push(bt.clone());
                        keep.push(Box::new(bt));
                    }
                    "NetworkToggles" => {
                        let net = Rc::new(NetworkManagerToggles::new(
                            &ctx.store,
                            &ctx.action_callback,
                            menu_store,
                            dynamic_rebuild.clone(),
                        ));
                        dynamic_sources.push(net.clone());
                        keep.push(Box::new(net));
                    }
                    "BackupToggle" => {
                        let t = Rc::new(BackupToggle::new(
                            &ctx.store,
                            &ctx.action_callback,
                            menu_store,
                            dynamic_rebuild.clone(),
                        ));
                        dynamic_sources.push(t.clone());
                        keep.push(Box::new(t));
                    }
                    _ => warn!("[renderer] Unknown toggle component in FeatureToggleGrid: {name}"),
                },
                LayoutNode::Widget { id } => {
                    debug!("[renderer] FeatureToggleGrid legacy Widget child: {}", id);
                    // Legacy Widget children in a mixed grid are not supported.
                    // They should use a separate FeatureToggleGrid with only Widget children.
                    warn!(
                        "[renderer] <Widget> inside component-based FeatureToggleGrid is not supported: {id}"
                    );
                }
                _ => {}
            }
        }
    }

    // Wire the real rebuild closure
    let grid_ref = grid.clone();
    let rebuild: Rc<dyn Fn()> = Rc::new(move || {
        let mut all: Vec<Rc<WidgetFeatureToggle>> = Vec::new();
        for source in &dynamic_sources {
            all.extend(source.as_feature_toggles());
        }
        all.sort_by_key(|t| t.weight);
        let grid_items: Vec<FeatureGridItem> = all
            .iter()
            .map(|t| FeatureGridItem {
                id: t.id.clone(),
                toggle: t.toggle.clone(),
                menu: t.menu.clone(),
            })
            .collect();
        grid_ref.sync_toggles(&grid_items);
    });

    *rebuild_slot.borrow_mut() = Some(rebuild.clone());

    // Initial sync
    rebuild();

    grid.widget().clone().upcast()
}

/// Trait for dynamic toggle sources that produce a variable number of feature toggles.
trait DynamicToggleSource {
    fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>>;
}

impl DynamicToggleSource for BluetoothToggles {
    fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        BluetoothToggles::as_feature_toggles(self)
    }
}

impl DynamicToggleSource for NetworkManagerToggles {
    fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        NetworkManagerToggles::as_feature_toggles(self)
    }
}

impl DynamicToggleSource for SimpleToggle {
    fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        SimpleToggle::as_feature_toggles(self)
    }
}

impl DynamicToggleSource for BackupToggle {
    fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        BackupToggle::as_feature_toggles(self)
    }
}
