/*!
Introduce declarative feature specs and build GNOME-like features section widget.

This module intentionally keeps the "Features" UI self-contained and mostly declarative.

Design goals:
- Keep `main.rs` small.
- Render GNOME Shell-like quick-settings tiles (content-less + split contentful) in a 2-column grid.
- Support "single-open" details panels (full-width) under tile rows.
- Provide a declarative `FeatureSpec` describing each feature.

Notes:
- This is GTK4 + libadwaita (Adw) UI code.
- Styling is done via CSS classes (e.g. `qs-tile`, `qs-on`, ...). You can keep CSS in `main.rs`
  or move it into its own CSS provider; this module only assigns classes.
- We avoid SCSS-only functions (like `shade(...)`) at runtime; rely on CSS classes and valid GTK CSS.
*/

use std::{cell::RefCell, collections::HashMap, future::Future, pin::Pin, rc::Rc};

use crate::ui::{UiEvent, UiEventSink};
use adw::prelude::*;
use gtk;

// Async, void toggle callback.
// - Runs on the GTK main loop via `glib::MainContext::spawn_local`.
// - Should emit `UiEvent`s to update UI state (active/status/menu) rather than returning values.
type OnToggleCallback =
    Rc<dyn Fn(&'static str, bool) -> Pin<Box<dyn Future<Output = ()> + 'static>> + Send + Sync>;

/// Declarative description of a feature tile.
///
/// The `menu` is the optional "details panel" that opens when you click the chevron half.
/// If `menu` is `None`, the tile is content-less (no chevron half).
#[derive(Clone)]
pub struct FeatureSpec {
    pub key: &'static str,
    pub title: String,
    pub icon: String,
    pub status_text: String,
    pub active: bool,

    /// Details panel content. If present, the tile renders as "split" (left toggles, right opens).
    pub menu: Option<MenuSpec>,

    /// Initial open state (if menu exists).
    pub open: bool,

    /// Async callback called when the feature is toggled. Returns the new active state.
    pub on_toggle: Option<OnToggleCallback>,
}

impl FeatureSpec {
    pub fn contentless(
        key: &'static str,
        title: impl Into<String>,
        icon: impl Into<String>,
        active: bool,
    ) -> Self {
        Self {
            key,
            title: title.into(),
            icon: icon.into(),
            status_text: String::new(),
            active,
            menu: None,
            open: false,
            on_toggle: None,
        }
    }

    pub fn contentless_with_toggle<F, Fut>(
        key: &'static str,
        title: impl Into<String>,
        icon: impl Into<String>,
        active: bool,
        on_toggle: F,
    ) -> Self
    where
        F: Fn(&'static str, bool) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        Self {
            key,
            title: title.into(),
            icon: icon.into(),
            status_text: String::new(),
            active,
            menu: None,
            open: false,
            on_toggle: Some(Rc::new(move |key, current_active| {
                Box::pin(on_toggle(key, current_active))
            })),
        }
    }

    pub fn contentful(
        key: &'static str,
        title: impl Into<String>,
        icon: impl Into<String>,
        status_text: impl Into<String>,
        active: bool,
        menu: MenuSpec,
        open: bool,
    ) -> Self {
        Self {
            key,
            title: title.into(),
            icon: icon.into(),
            status_text: status_text.into(),
            active,
            menu: Some(menu),
            open,
            on_toggle: None,
        }
    }
}

/// Declarative details panel.
#[derive(Clone)]
pub struct MenuSpec {
    pub widget: gtk::Widget,
}

impl MenuSpec {
    pub fn new<W: IsA<gtk::Widget>>(w: &W) -> Self {
        Self {
            widget: w.clone().upcast::<gtk::Widget>(),
        }
    }
}

/// A small model that allows the caller to programmatically update tile status/active state.
#[derive(Clone, Default)]
pub struct FeaturesModel {
    inner: Rc<RefCell<ModelInner>>,
}

#[derive(Default)]
struct ModelInner {
    // key -> surface widget to toggle "qs-on"
    surfaces: Vec<(&'static str, gtk::Widget)>,
    // key -> status label
    status_labels: Vec<(&'static str, gtk::Label)>,
    // key -> chevron image (optional)
    chevrons: Vec<(&'static str, gtk::Image)>,
}

impl FeaturesModel {
    pub fn set_active(&self, key: &'static str, active: bool) {
        let inner = self.inner.borrow();
        if let Some((_, surface)) = inner.surfaces.iter().find(|(k, _)| *k == key) {
            if active {
                surface.add_css_class("qs-on");
            } else {
                surface.remove_css_class("qs-on");
            }
        }
    }

    pub fn set_status_text(&self, key: &'static str, text: &str) {
        let inner = self.inner.borrow();
        if let Some((_, lbl)) = inner.status_labels.iter().find(|(k, _)| *k == key) {
            lbl.set_label(text);
        }
    }

    pub fn set_chevron_open(&self, key: &'static str, open: bool) {
        let inner = self.inner.borrow();
        if let Some((_, img)) = inner.chevrons.iter().find(|(k, _)| *k == key) {
            img.set_icon_name(Some(if open {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            }));
        }
    }
}

impl UiEventSink for FeaturesModel {
    fn send(&self, event: UiEvent) {
        match event {
            UiEvent::FeatureActiveChanged { key, active } => {
                self.set_active(Box::leak(key.into_boxed_str()), active);
            }
            UiEvent::FeatureStatusTextChanged { key, text } => {
                self.set_status_text(Box::leak(key.into_boxed_str()), &text);
            }
            UiEvent::FeatureMenuOpenChanged { key, open } => {
                self.set_chevron_open(Box::leak(key.into_boxed_str()), open);
            }
        }
    }
}

/// Build a generic Features section:
/// - Renders tiles in a 2-column grid, consuming `specs` in order.
/// - After each tile row, inserts 0 or 2 full-width "details rows" (Revealers) if tiles have menus.
/// - Supports 0–2 tiles per row (last row may have 1 tile).
/// - At most ONE details panel is open at a time (single-open), regardless of which row it belongs to.
///
/// This new implementation creates separate expanders for each tile position, eliminating
/// the need for content swapping and delays.
pub fn build_features_section(specs: Vec<FeatureSpec>) -> (gtk::Box, FeaturesModel) {
    let model = FeaturesModel::default();

    let section = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .build();

    let title = gtk::Label::builder()
        .label("Features")
        .xalign(0.0)
        .css_classes(["qs-section-title"])
        .build();
    section.append(&title);

    let grid = gtk::Grid::builder()
        .column_spacing(12)
        .row_spacing(0)
        .build();

    // Single-open across all detail panels.
    let open_key: Rc<RefCell<Option<&'static str>>> = Rc::new(RefCell::new(None));

    // Track all detail panels (one per tile position that has a menu).
    let all_detail_panels: Rc<RefCell<HashMap<&'static str, (gtk::Revealer, gtk::Box)>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // Close all panels + reset chevrons (animated collapse).
    let close_all = {
        let open_key = open_key.clone();
        let all_detail_panels = all_detail_panels.clone();
        let model = model.clone();

        move || {
            // Start collapse animations for all panels.
            for (_key, (revealer, _holder)) in all_detail_panels.borrow().iter() {
                revealer.set_reveal_child(false);
            }

            // Reset chevron for whichever tile is open.
            if let Some(k) = *open_key.borrow() {
                model.set_chevron_open(k, false);
            }

            *open_key.borrow_mut() = None;
        }
    };

    // Open a menu in its dedicated detail panel.
    //
    // Behavior:
    // - If the same panel is open -> collapse it (animated)
    // - If another panel is open -> collapse all (animated) then expand new one
    let open_menu = {
        let open_key = open_key.clone();
        let all_detail_panels = all_detail_panels.clone();
        let close_all = close_all.clone();
        let model = model.clone();

        move |key: &'static str, menu: &gtk::Widget| {
            let already_open = open_key.borrow().as_deref() == Some(key)
                && all_detail_panels
                    .borrow()
                    .get(key)
                    .map(|(r, _)| r.reveals_child())
                    .unwrap_or(false);

            if already_open {
                // Collapse just this panel.
                if let Some((revealer, _holder)) = all_detail_panels.borrow().get(key) {
                    revealer.set_reveal_child(false);
                }

                model.set_chevron_open(key, false);
                *open_key.borrow_mut() = None;
                return;
            }

            // Switching: collapse everything first (animated).
            close_all();

            // Set new open state and expand immediately.
            *open_key.borrow_mut() = Some(key);
            model.set_chevron_open(key, true);

            if let Some((revealer, holder)) = all_detail_panels.borrow().get(key) {
                // Clear any existing content and set new content
                while let Some(child) = holder.first_child() {
                    holder.remove(&child);
                }
                holder.append(menu);
                revealer.set_reveal_child(true);
            }
        }
    };

    // ---- Tile builders ----

    // Shared left content: icon + title + optional status.
    let build_left_content =
        |icon_name: &str, title: &str, status: &str| -> (gtk::Box, gtk::Label) {
            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .valign(gtk::Align::Center)
                .build();

            let icon = gtk::Image::from_icon_name(icon_name);
            icon.set_pixel_size(20);

            let text_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(if status.is_empty() { 0 } else { 1 })
                .hexpand(true)
                .valign(gtk::Align::Center)
                .build();

            let title_label = gtk::Label::builder()
                .label(title)
                .xalign(0.0)
                .css_classes(["heading"])
                .build();
            text_box.append(&title_label);

            let status_label = gtk::Label::builder()
                .label(status)
                .xalign(0.0)
                .css_classes(["caption", "dim-label"])
                .build();

            if !status.is_empty() {
                text_box.append(&status_label);
            }

            content.append(&icon);
            content.append(&text_box);

            (content, status_label)
        };

    // Shared tile container creation and registration
    let create_tile_container = |spec: &FeatureSpec| -> gtk::Box {
        let tile = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["qs-tile"])
            .build();

        if spec.active {
            tile.add_css_class("qs-on");
        }

        // Register surface for model
        model
            .inner
            .borrow_mut()
            .surfaces
            .push((spec.key, tile.clone().upcast()));

        tile
    };

    // Shared toggle logic for active state using async callback (void result).
    //
    // The callback is responsible for emitting `UiEvent`s to update UI state.
    // The UI does not optimistically toggle; it waits for model updates.
    let create_toggle_handler = |tile: gtk::Box,
                                 key: &'static str,
                                 model: FeaturesModel,
                                 on_toggle: Option<OnToggleCallback>|
     -> Box<dyn Fn()> {
        let tile = tile.clone();
        let model = model.clone();
        Box::new(move || {
            let current_active = tile.has_css_class("qs-on");

            if let Some(ref callback) = on_toggle {
                let key = key;
                let _tile = tile.clone();
                let _model = model.clone();
                let callback = callback.clone();

                // Spawn the async callback on the GTK main loop.
                gtk::glib::MainContext::default().spawn_local(async move {
                    callback(key, current_active).await;
                });
            } else {
                // Fallback to simple toggle if no callback provided
                let new_active = !current_active;
                if new_active {
                    tile.add_css_class("qs-on");
                    model.set_active(key, true);
                } else {
                    tile.remove_css_class("qs-on");
                    model.set_active(key, false);
                }
            }
        })
    };

    // Content-less: tile + single button.
    let build_contentless_tile =
        |spec: &FeatureSpec, model: &FeaturesModel| -> (gtk::Widget, gtk::Widget) {
            let tile = create_tile_container(spec);

            let btn = gtk::Button::builder()
                .css_classes(["flat", "qs-btn-single"])
                .hexpand(true)
                .build();

            let (content, _status_label) = build_left_content(&spec.icon, &spec.title, "");
            btn.set_child(Some(&content));

            // Toggle ON/OFF on click using async callback.
            let toggle_handler = create_toggle_handler(
                tile.clone(),
                spec.key,
                model.clone(),
                spec.on_toggle.clone(),
            );
            btn.connect_clicked(move |_| toggle_handler());

            tile.append(&btn);

            (tile.upcast::<gtk::Widget>(), btn.upcast::<gtk::Widget>())
        };

    // Split: tile + left + right + divider, with menu open.
    let build_split_tile = |spec: &FeatureSpec, model: &FeaturesModel| -> gtk::Widget {
        let tile = create_tile_container(spec);

        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(0)
            .css_classes(["qs-split-row"])
            .build();

        let left_btn = gtk::Button::builder()
            .css_classes(["flat", "qs-btn-left"])
            .hexpand(true)
            .build();

        let (left_content, status_label) =
            build_left_content(&spec.icon, &spec.title, &spec.status_text);
        left_btn.set_child(Some(&left_content));

        let divider = gtk::Separator::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["qs-divider"])
            .build();

        let right_btn = gtk::Button::builder()
            .css_classes(["flat", "qs-btn-right"])
            .build();

        // Chevron icon
        let chevron = gtk::Image::from_icon_name(if spec.open {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        });
        chevron.set_pixel_size(18);
        right_btn.set_child(Some(&chevron));

        // Left toggles active (close-on-off) using async callback.
        left_btn.connect_clicked({
            let tile = tile.clone();
            let key = spec.key;
            let status_label = status_label.clone();
            let status_on = spec.status_text.clone();
            let model = model.clone();
            let close_all = close_all.clone();
            let open_key = open_key.clone();
            let on_toggle = spec.on_toggle.clone();
            move |_| {
                let current_active = tile.has_css_class("qs-on");

                if let Some(ref callback) = on_toggle {
                    let key = key;
                    let _tile = tile.clone();
                    let _model = model.clone();
                    let _close_all = close_all.clone();
                    let _open_key = open_key.clone();
                    let _status_label = status_label.clone();
                    let _status_on = status_on.clone();
                    let callback = callback.clone();

                    // Spawn the async callback on the GTK main loop.
                    // The callback is responsible for emitting `UiEvent`s that update model/UI.
                    gtk::glib::MainContext::default().spawn_local(async move {
                        callback(key, current_active).await;
                    });
                } else {
                    // Fallback to simple toggle if no callback provided
                    let new_active = !current_active;
                    if new_active {
                        tile.add_css_class("qs-on");
                        model.set_active(key, true);
                        if !status_on.is_empty() {
                            status_label.set_label(&status_on);
                        }
                    } else {
                        tile.remove_css_class("qs-on");
                        // if this panel is open, close it (close-on-off)
                        if open_key.borrow().as_deref() == Some(key) {
                            close_all();
                        }
                        model.set_active(key, false);
                    }
                }
            }
        });

        // Right opens the menu (allowed even while off).
        if let Some(menu) = spec.menu.as_ref() {
            right_btn.connect_clicked({
                let key = spec.key;
                let open_menu = open_menu.clone();
                let menu_widget = menu.widget.clone();
                move |_| {
                    open_menu(key, &menu_widget);
                }
            });
        }

        row.append(&left_btn);
        row.append(&divider);
        row.append(&right_btn);
        tile.append(&row);

        // Register remaining model components
        model
            .inner
            .borrow_mut()
            .status_labels
            .push((spec.key, status_label));
        model.inner.borrow_mut().chevrons.push((spec.key, chevron));

        tile.upcast::<gtk::Widget>()
    };

    // ---- Layout (generic, order-based) ----
    let mut y: i32 = 0;
    let mut i: usize = 0;
    let specs = specs;

    while i < specs.len() {
        // Collect up to two specs for this tile row.
        let left = &specs[i];
        let right = if i + 1 < specs.len() {
            Some(&specs[i + 1])
        } else {
            None
        };
        i += 2;

        // Helper to build and place tiles
        let build_and_place_tile = |spec: &FeatureSpec, col: i32| {
            let widget = if spec.menu.is_some() {
                build_split_tile(spec, &model)
            } else {
                build_contentless_tile(spec, &model).0
            };
            grid.attach(&widget, col, y, 1, 1);
        };

        // Place tiles
        build_and_place_tile(left, 0);
        if let Some(r) = right {
            build_and_place_tile(r, 1);
        } else {
            grid.attach(&gtk::Box::builder().build(), 1, y, 1, 1);
        }

        // Create detail panels for tiles that have menus.
        // Each panel spans full width (2 columns) and pushes content below down.
        let mut detail_rows_created = 0;

        // Left tile detail panel
        if let Some(menu) = left.menu.as_ref() {
            let details_y = y + 1 + detail_rows_created;

            let revealer = gtk::Revealer::builder()
                .reveal_child(false)
                .transition_type(gtk::RevealerTransitionType::SlideDown)
                .hexpand(true)
                .build();
            let holder = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .css_classes(["qs-details"])
                .build();
            revealer.set_child(Some(&holder));

            // Span both columns (0, width=2) for full row width
            grid.attach(&revealer, 0, details_y, 2, 1);
            all_detail_panels
                .borrow_mut()
                .insert(left.key, (revealer.clone(), holder.clone()));

            // Open initially if requested
            if left.open {
                open_menu(left.key, &menu.widget);
            }

            detail_rows_created += 1;
        }

        // Right tile detail panel
        if let Some(r) = right {
            if let Some(menu) = r.menu.as_ref() {
                let details_y = y + 1 + detail_rows_created;

                let revealer = gtk::Revealer::builder()
                    .reveal_child(false)
                    .transition_type(gtk::RevealerTransitionType::SlideDown)
                    .hexpand(true)
                    .build();
                let holder = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .css_classes(["qs-details"])
                    .build();
                revealer.set_child(Some(&holder));

                // Span both columns (0, width=2) for full row width
                grid.attach(&revealer, 0, details_y, 2, 1);
                all_detail_panels
                    .borrow_mut()
                    .insert(r.key, (revealer.clone(), holder.clone()));

                // Open initially if requested
                if r.open {
                    open_menu(r.key, &menu.widget);
                }

                detail_rows_created += 1;
            }
        }

        // Update y position based on how many detail rows we created
        if detail_rows_created > 0 {
            y += 1 + detail_rows_created;
        } else {
            y += 1;
        }

        // Right tile detail panel
        if let Some(r) = right {
            if let Some(menu) = r.menu.as_ref() {
                let details_y = y + 1 + detail_rows_created;

                let revealer = gtk::Revealer::builder()
                    .reveal_child(false)
                    .transition_type(gtk::RevealerTransitionType::SlideDown)
                    .hexpand(true)
                    .build();
                let holder = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .css_classes(["qs-details"])
                    .build();
                revealer.set_child(Some(&holder));

                // Span both columns (0, width=2) for full row width
                grid.attach(&revealer, 0, details_y, 2, 1);
                all_detail_panels
                    .borrow_mut()
                    .insert(r.key, (revealer.clone(), holder.clone()));

                // Open initially if requested
                if r.open {
                    open_menu(r.key, &menu.widget);
                }

                detail_rows_created += 1;
            }
        }

        // Update y position based on how many detail rows we created
        if detail_rows_created > 0 {
            y += 1 + detail_rows_created;
        } else {
            y += 1;
        }
    }

    section.append(&grid);
    (section, model)
}
