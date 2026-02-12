//! Layout renderer -- converts a LayoutNode tree into a GTK widget tree with bindings.
//!
//! Handles purpose-built entity components (`LayoutNode::Component`) and
//! legacy Widget/Unmatched nodes for backward compatibility.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gtk::prelude::*;
use log::{debug, warn};

use crate::components::agenda::AgendaComponent;
use crate::components::audio_sliders::AudioSlidersComponent;
use crate::components::battery::BatteryComponent;
use crate::components::brightness_sliders::BrightnessSlidersComponent;
use crate::components::clock::ClockComponent;
use crate::components::keyboard_layout::KeyboardLayoutComponent;
use crate::components::notification_list::NotificationsComponent;
use crate::components::session_actions::SessionActionsComponent;
use crate::components::system_actions::SystemActionsComponent;
use crate::components::toggles::bluetooth::BluetoothToggles;
use crate::components::toggles::caffeine::CaffeineToggle;
use crate::components::toggles::dark_mode::DarkModeToggle;
use crate::components::toggles::dnd::DoNotDisturbToggle;
use crate::components::toggles::network::NetworkManagerToggles;
use crate::components::toggles::night_light::NightLightToggle;
use crate::components::weather::WeatherComponent;
use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::layout::compositor::{
    FeatureToggleGridCompositor, FragmentCompositor, WidgetCompositor,
};
use crate::layout::model::LayoutNode;
use crate::layout::parser::glob_match;
use crate::menu_state::MenuStore;
use crate::plugin::WidgetFeatureToggle;
use crate::plugin_registry::{PluginRegistry, SlotItem};
use crate::ui::feature_grid::FeatureGridWidget;
use crate::ui::main_window::trigger_window_resize;

/// Shared context for the layout renderer, providing access to entity store,
/// action routing, and component lifetime management.
pub struct RenderContext {
    pub store: Rc<EntityStore>,
    pub action_callback: EntityActionCallback,
    /// Keeps components alive (entity subscriptions, toggle state).
    keep_alive: RefCell<Vec<Box<dyn std::any::Any>>>,
}

impl RenderContext {
    pub fn new(store: Rc<EntityStore>, action_callback: EntityActionCallback) -> Self {
        Self {
            store,
            action_callback,
            keep_alive: RefCell::new(Vec::new()),
        }
    }
}

/// A binding between widget ID patterns and a compositor.
struct WidgetBinding {
    patterns: Vec<String>,
    compositor: Box<dyn WidgetCompositor>,
}

/// The result of rendering a layout tree into GTK widgets.
///
/// Holds the root GTK widget, the bindings that map widget IDs to compositors,
/// and the render context that keeps entity components alive.
/// Call `sync()` to update the layout with current registry contents.
pub struct RenderedLayout {
    pub root: gtk::Widget,
    bindings: Vec<WidgetBinding>,
    unmatched: Option<Box<dyn WidgetCompositor>>,
    _context: Rc<RenderContext>,
}

impl RenderedLayout {
    /// Synchronize all bindings with the current registry state.
    ///
    /// Each binding's patterns are matched against all items. First match wins.
    /// Unmatched items go to the `<Unmatched>` compositor if present.
    pub fn sync(&self, registry: &PluginRegistry) {
        let all_items = registry.all_items();
        let mut matched_ids: HashSet<String> = HashSet::new();

        for binding in &self.bindings {
            let mut items: Vec<SlotItem> = all_items
                .iter()
                .filter(|item: &&SlotItem| {
                    binding
                        .patterns
                        .iter()
                        .any(|p| glob_match(item.id(), p))
                })
                .filter(|item: &&SlotItem| !matched_ids.contains(item.id()))
                .cloned()
                .collect();

            // Sort by weight within each binding
            items.sort_by_key(|item: &SlotItem| item.weight());

            for item in &items {
                matched_ids.insert(item.id().to_string());
            }
            binding.compositor.sync(&items);
        }

        if let Some(ref unmatched) = self.unmatched {
            let mut remaining: Vec<SlotItem> = all_items
                .iter()
                .filter(|item: &&SlotItem| !matched_ids.contains(item.id()))
                .cloned()
                .collect();
            remaining.sort_by_key(|item: &SlotItem| item.weight());
            unmatched.sync(&remaining);
        }

        trigger_window_resize();
    }
}

/// Render a LayoutNode tree into a GTK widget tree with bindings.
pub fn render_layout(
    tree: &LayoutNode,
    ctx: &Rc<RenderContext>,
    menu_store: &Rc<MenuStore>,
) -> RenderedLayout {
    let mut bindings = Vec::new();
    let mut unmatched: Option<Box<dyn WidgetCompositor>> = None;

    let root = render_node(tree, ctx, menu_store, &mut bindings, &mut unmatched);

    RenderedLayout {
        root,
        bindings,
        unmatched,
        _context: ctx.clone(),
    }
}

fn render_layout_box(
    orientation: gtk::Orientation,
    halign: &Option<String>,
    children: &[LayoutNode],
    ctx: &Rc<RenderContext>,
    menu_store: &Rc<MenuStore>,
    bindings: &mut Vec<WidgetBinding>,
    unmatched: &mut Option<Box<dyn WidgetCompositor>>,
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
        let widget = render_node(child, ctx, menu_store, bindings, unmatched);
        container.append(&widget);
    }
    container.upcast()
}

fn render_node(
    node: &LayoutNode,
    ctx: &Rc<RenderContext>,
    menu_store: &Rc<MenuStore>,
    bindings: &mut Vec<WidgetBinding>,
    unmatched: &mut Option<Box<dyn WidgetCompositor>>,
) -> gtk::Widget {
    match node {
        LayoutNode::Overview { children } => {
            let vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
            for child in children {
                let widget = render_node(child, ctx, menu_store, bindings, unmatched);
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
                let widget = render_node(child, ctx, menu_store, bindings, unmatched);
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
                let widget = render_node(child, ctx, menu_store, bindings, unmatched);
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
                let widget = render_node(child, ctx, menu_store, bindings, unmatched);
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
            bindings,
            unmatched,
        ),

        LayoutNode::Row { halign, children } => render_layout_box(
            gtk::Orientation::Horizontal,
            halign,
            children,
            ctx,
            menu_store,
            bindings,
            unmatched,
        ),

        LayoutNode::Col { halign, children } => render_layout_box(
            gtk::Orientation::Vertical,
            halign,
            children,
            ctx,
            menu_store,
            bindings,
            unmatched,
        ),

        LayoutNode::Divider => {
            let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
            sep.set_hexpand(true);
            sep.upcast()
        }

        LayoutNode::Component { name } => render_component(name, ctx, menu_store),

        LayoutNode::FeatureToggleGrid { children } => {
            render_feature_toggle_grid(children, ctx, menu_store, bindings)
        }

        LayoutNode::Widget { id } => {
            let compositor = FragmentCompositor::new();
            let widget = compositor.widget().clone();

            debug!("[renderer] Widget placeholder for pattern: {}", id);

            bindings.push(WidgetBinding {
                patterns: vec![id.clone()],
                compositor: Box::new(compositor),
            });

            widget
        }

        LayoutNode::Unmatched => {
            let compositor = FragmentCompositor::new();
            let widget = compositor.widget().clone();

            debug!("[renderer] Unmatched catch-all");

            *unmatched = Some(Box::new(compositor));

            widget
        }
    }
}

/// Create a purpose-built entity component by name.
fn render_component(name: &str, ctx: &Rc<RenderContext>, menu_store: &Rc<MenuStore>) -> gtk::Widget {
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
        "Agenda" => {
            let c = AgendaComponent::new(&ctx.store, menu_store);
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
            let c = AudioSlidersComponent::new(&ctx.store, &ctx.action_callback);
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

/// Render a FeatureToggleGrid that may contain Component children, Widget children, or both.
fn render_feature_toggle_grid(
    children: &[LayoutNode],
    ctx: &Rc<RenderContext>,
    menu_store: &Rc<MenuStore>,
    bindings: &mut Vec<WidgetBinding>,
) -> gtk::Widget {
    // Separate component children from legacy Widget children
    let has_components = children
        .iter()
        .any(|c| matches!(c, LayoutNode::Component { .. }));
    let has_widgets = children
        .iter()
        .any(|c| matches!(c, LayoutNode::Widget { .. }));

    if !has_components {
        // Pure legacy mode: all Widget children — use FeatureToggleGridCompositor
        let compositor = FeatureToggleGridCompositor::new(menu_store.clone());
        let widget = compositor.widget().clone();

        let patterns: Vec<String> = children
            .iter()
            .filter_map(|child| {
                if let LayoutNode::Widget { id } = child {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        debug!(
            "[renderer] FeatureToggleGrid (legacy) with {} patterns: {:?}",
            patterns.len(),
            patterns
        );

        bindings.push(WidgetBinding {
            patterns,
            compositor: Box::new(compositor),
        });

        return widget;
    }

    // Entity component mode: create toggle components and wire into a grid
    let grid = Rc::new(FeatureGridWidget::new(Vec::new(), menu_store.clone()));

    // Two-phase init for dynamic toggle rebuild cycle
    let rebuild_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

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
                        let t = Rc::new(DoNotDisturbToggle::new(
                            &ctx.store,
                            &ctx.action_callback,
                            dynamic_rebuild.clone(),
                        ));
                        dynamic_sources.push(t.clone());
                        keep.push(Box::new(t));
                    }
                    "CaffeineToggle" => {
                        let t = Rc::new(CaffeineToggle::new(
                            &ctx.store,
                            &ctx.action_callback,
                            dynamic_rebuild.clone(),
                        ));
                        dynamic_sources.push(t.clone());
                        keep.push(Box::new(t));
                    }
                    "DarkModeToggle" => {
                        let t = Rc::new(DarkModeToggle::new(
                            &ctx.store,
                            &ctx.action_callback,
                            dynamic_rebuild.clone(),
                        ));
                        dynamic_sources.push(t.clone());
                        keep.push(Box::new(t));
                    }
                    "NightLightToggle" => {
                        let t = Rc::new(NightLightToggle::new(
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
                    _ => warn!(
                        "[renderer] Unknown toggle component in FeatureToggleGrid: {name}"
                    ),
                },
                LayoutNode::Widget { id } => {
                    debug!(
                        "[renderer] FeatureToggleGrid legacy Widget child: {}",
                        id
                    );
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
        grid_ref.sync_toggles(&all);
    });

    *rebuild_slot.borrow_mut() = Some(rebuild.clone());

    // Initial sync
    rebuild();

    // If there are also Widget children, create a legacy compositor for them
    if has_widgets {
        let compositor = FeatureToggleGridCompositor::new(menu_store.clone());
        let patterns: Vec<String> = children
            .iter()
            .filter_map(|child| {
                if let LayoutNode::Widget { id } = child {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        bindings.push(WidgetBinding {
            patterns,
            compositor: Box::new(compositor),
        });
    }

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

impl DynamicToggleSource for DarkModeToggle {
    fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        DarkModeToggle::as_feature_toggles(self)
    }
}

impl DynamicToggleSource for NightLightToggle {
    fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        NightLightToggle::as_feature_toggles(self)
    }
}

impl DynamicToggleSource for CaffeineToggle {
    fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        CaffeineToggle::as_feature_toggles(self)
    }
}

impl DynamicToggleSource for DoNotDisturbToggle {
    fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        DoNotDisturbToggle::as_feature_toggles(self)
    }
}
