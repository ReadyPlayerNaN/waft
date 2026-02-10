//! Layout renderer -- converts a LayoutNode tree into a GTK widget tree with bindings.

use std::collections::HashSet;
use std::rc::Rc;

use gtk::prelude::*;
use log::debug;

use crate::layout::compositor::{
    FeatureToggleGridCompositor, FragmentCompositor, WidgetCompositor,
};
use crate::layout::model::LayoutNode;
use crate::layout::parser::glob_match;
use crate::menu_state::MenuStore;
use crate::plugin_registry::{PluginRegistry, SlotItem};
use crate::ui::main_window::trigger_window_resize;

/// A binding between widget ID patterns and a compositor.
struct WidgetBinding {
    patterns: Vec<String>,
    compositor: Box<dyn WidgetCompositor>,
}

/// The result of rendering a layout tree into GTK widgets.
///
/// Holds the root GTK widget and the bindings that map widget IDs to compositors.
/// Call `sync()` to update the layout with current registry contents.
pub struct RenderedLayout {
    pub root: gtk::Widget,
    bindings: Vec<WidgetBinding>,
    unmatched: Option<Box<dyn WidgetCompositor>>,
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
pub fn render_layout(tree: &LayoutNode, menu_store: &Rc<MenuStore>) -> RenderedLayout {
    let mut bindings = Vec::new();
    let mut unmatched: Option<Box<dyn WidgetCompositor>> = None;

    let root = render_node(tree, menu_store, &mut bindings, &mut unmatched);

    RenderedLayout {
        root,
        bindings,
        unmatched,
    }
}

fn render_layout_box(
    orientation: gtk::Orientation,
    halign: &Option<String>,
    children: &[LayoutNode],
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
        let widget = render_node(child, menu_store, bindings, unmatched);
        container.append(&widget);
    }
    container.upcast()
}

fn render_node(
    node: &LayoutNode,
    menu_store: &Rc<MenuStore>,
    bindings: &mut Vec<WidgetBinding>,
    unmatched: &mut Option<Box<dyn WidgetCompositor>>,
) -> gtk::Widget {
    match node {
        LayoutNode::Overview { children } => {
            let vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
            for child in children {
                let widget = render_node(child, menu_store, bindings, unmatched);
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
                let widget = render_node(child, menu_store, bindings, unmatched);
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
                let widget = render_node(child, menu_store, bindings, unmatched);
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
                let widget = render_node(child, menu_store, bindings, unmatched);
                if let Some(w) = widget.downcast_ref::<gtk::Box>() {
                    w.set_hexpand(true);
                    w.set_width_request(480);
                }
                hbox.append(&widget);
            }

            hbox.upcast()
        }

        LayoutNode::Box { halign, children } => {
            render_layout_box(gtk::Orientation::Vertical, halign, children, menu_store, bindings, unmatched)
        }

        LayoutNode::Row { halign, children } => {
            render_layout_box(gtk::Orientation::Horizontal, halign, children, menu_store, bindings, unmatched)
        }

        LayoutNode::Col { halign, children } => {
            render_layout_box(gtk::Orientation::Vertical, halign, children, menu_store, bindings, unmatched)
        }

        LayoutNode::Divider => {
            let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
            sep.set_hexpand(true);
            sep.upcast()
        }

        LayoutNode::FeatureToggleGrid { children } => {
            let compositor = FeatureToggleGridCompositor::new(menu_store.clone());
            let widget = compositor.widget().clone();

            // Collect all Widget child patterns
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
                "[renderer] FeatureToggleGrid with {} patterns: {:?}",
                patterns.len(),
                patterns
            );

            bindings.push(WidgetBinding {
                patterns,
                compositor: Box::new(compositor),
            });

            widget
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
