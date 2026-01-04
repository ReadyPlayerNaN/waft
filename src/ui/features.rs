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

Animation note:
- The details panel "slide up" collapse animation requires the content to remain present while the
  revealer collapses.
- Canceling GLib timeouts via `SourceId::remove()` can panic if the source is already gone.
- Instead of canceling timers, this module uses generation tokens per details row: a scheduled clear
  will only run if its generation is still current when the timeout fires.
*/

use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use adw::prelude::*;
use gtk::{self, glib};

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

/// Build a generic Features section:
/// - Renders tiles in a 2-column grid, consuming `specs` in order.
/// - After each tile row, inserts 0 or 1 full-width "details row" (Revealer) if any tile in that row
///   has a menu. That details row can host menus for either tile in that row.
/// - Supports 0–2 tiles per row (last row may have 1 tile).
/// - At most ONE details row is open at a time (single-open), regardless of which row it belongs to.
///
/// Important:
/// - We map each `FeatureSpec.key` to its computed `details_y` when building the grid, and the
///   chevron click handler uses that mapping.
/// - For smooth collapse animation, we never clear the revealer's child while collapsing. We
///   postpone clearing/remounting by a tick.
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

    // Single-open across an arbitrary number of per-row detail revealers.
    let open_key: Rc<RefCell<Option<&'static str>>> = Rc::new(RefCell::new(None));
    let open_row: Rc<RefCell<Option<i32>>> = Rc::new(RefCell::new(None));

    // Track all revealers/holders so we can close them all.
    let all_detail_rows: Rc<RefCell<Vec<(i32, gtk::Revealer, gtk::Box)>>> =
        Rc::new(RefCell::new(Vec::new()));

    // Map each feature key -> computed details_y (grid row of the revealer).
    let key_to_details_y: Rc<RefCell<HashMap<&'static str, i32>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // Per-details-row "generation" counters for scheduled clears.
    //
    // Instead of canceling a pending timeout (which can panic depending on GLib state),
    // we bump a generation counter. A scheduled clear will only execute if its generation
    // still matches the latest generation for that row.
    let clear_generations: Rc<RefCell<HashMap<i32, u64>>> = Rc::new(RefCell::new(HashMap::new()));

    // Bump generation for a row (invalidates any pending clear for that row).
    let bump_generation = {
        let clear_generations = clear_generations.clone();
        move |details_y: i32| -> u64 {
            let mut gens = clear_generations.borrow_mut();
            let next = gens.get(&details_y).copied().unwrap_or(0).saturating_add(1);
            gens.insert(details_y, next);
            next
        }
    };

    // Schedule clearing for a given details row after the collapse animation.
    // The clear runs only if the generation hasn't changed since scheduling.
    let schedule_clear_for_row = {
        let clear_generations = clear_generations.clone();
        let all_detail_rows = all_detail_rows.clone();
        let bump_generation = bump_generation.clone();
        move |details_y: i32| {
            let generation = bump_generation(details_y);

            let all_detail_rows = all_detail_rows.clone();
            let clear_generations = clear_generations.clone();

            glib::timeout_add_local_once(Duration::from_millis(220), move || {
                // If generation changed, this clear is stale.
                let current = clear_generations
                    .borrow()
                    .get(&details_y)
                    .copied()
                    .unwrap_or(0);
                if current != generation {
                    return;
                }

                if let Some((_y, _revealer, holder)) = all_detail_rows
                    .borrow()
                    .iter()
                    .find(|(y, _, _)| *y == details_y)
                {
                    while let Some(child) = holder.first_child() {
                        holder.remove(&child);
                    }
                }
            });
        }
    };

    // Close all panels + reset chevrons (animated collapse).
    let close_all = {
        let open_key = open_key.clone();
        let open_row = open_row.clone();
        let all_detail_rows = all_detail_rows.clone();
        let model = model.clone();
        let schedule_clear_for_row = schedule_clear_for_row.clone();

        move || {
            // Start collapse animations for all rows (do not clear immediately).
            for (details_y, revealer, _holder) in all_detail_rows.borrow().iter() {
                revealer.set_reveal_child(false);
                schedule_clear_for_row(*details_y);
            }

            // Reset chevron for whichever tile is open.
            if let Some(k) = *open_key.borrow() {
                model.set_chevron_open(k, false);
            }

            *open_key.borrow_mut() = None;
            *open_row.borrow_mut() = None;
        }
    };

    // Open a menu in the detail revealer belonging to a specific grid row `details_y`.
    //
    // Behavior:
    // - If the same panel is open -> collapse it (animated)
    // - If another panel is open -> collapse all (animated) then mount+expand the new one
    let open_menu = {
        let open_key = open_key.clone();
        let open_row = open_row.clone();
        let all_detail_rows = all_detail_rows.clone();
        let close_all = close_all.clone();
        let model = model.clone();
        let schedule_clear_for_row = schedule_clear_for_row.clone();
        let bump_generation = bump_generation.clone();

        move |key: &'static str, menu: &gtk::Widget, details_y: i32| {
            let already_open = open_key.borrow().as_deref() == Some(key)
                && *open_row.borrow() == Some(details_y)
                && all_detail_rows
                    .borrow()
                    .iter()
                    .find(|(y, _, _)| *y == details_y)
                    .map(|(_, r, _)| r.reveals_child())
                    .unwrap_or(false);

            if already_open {
                // Collapse just this row.
                if let Some((_y, revealer, _holder)) = all_detail_rows
                    .borrow()
                    .iter()
                    .find(|(y, _, _)| *y == details_y)
                {
                    revealer.set_reveal_child(false);
                }

                model.set_chevron_open(key, false);
                *open_key.borrow_mut() = None;
                *open_row.borrow_mut() = None;

                schedule_clear_for_row(details_y);
                return;
            }

            // Switching: collapse everything first (animated + per-row clears).
            close_all();

            // Invalidate any pending clear for the row we're about to open.
            let generation = bump_generation(details_y);

            *open_key.borrow_mut() = Some(key);
            *open_row.borrow_mut() = Some(details_y);

            // For smooth animation when switching menus, delay content mounting
            // until after the collapse animation completes.
            let all_detail_rows_clone = all_detail_rows.clone();
            let menu_clone = menu.clone();
            let model_clone = model.clone();
            let key_clone = key;
            let clear_generations_clone = clear_generations.clone();

            glib::timeout_add_local_once(Duration::from_millis(220), move || {
                // Check if generation is still current (no other operations occurred)
                let current = clear_generations_clone
                    .borrow()
                    .get(&details_y)
                    .copied()
                    .unwrap_or(0);
                if current != generation {
                    return;
                }

                let mut rows = all_detail_rows_clone.borrow_mut();
                if let Some((_y, revealer, holder)) =
                    rows.iter_mut().find(|(y, _, _)| *y == details_y)
                {
                    while let Some(child) = holder.first_child() {
                        holder.remove(&child);
                    }
                    holder.append(&menu_clone);
                    revealer.set_reveal_child(true);
                    model_clone.set_chevron_open(key_clone, true);
                }
            });
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

    // Content-less: tile + single button.
    let build_contentless_tile =
        |spec: &FeatureSpec, model: &FeaturesModel| -> (gtk::Widget, gtk::Widget) {
            let tile = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .css_classes(["qs-tile"])
                .build();

            if spec.active {
                tile.add_css_class("qs-on");
            }

            let btn = gtk::Button::builder()
                .css_classes(["flat", "qs-btn-single"])
                .hexpand(true)
                .build();

            let (content, _status_label) = build_left_content(&spec.icon, &spec.title, "");
            btn.set_child(Some(&content));

            // Toggle ON/OFF on click.
            btn.connect_clicked({
                let tile = tile.clone();
                let key = spec.key;
                let model = model.clone();
                move |_| {
                    let on = tile.has_css_class("qs-on");
                    if on {
                        tile.remove_css_class("qs-on");
                        model.set_active(key, false);
                    } else {
                        tile.add_css_class("qs-on");
                        model.set_active(key, true);
                    }
                }
            });

            tile.append(&btn);

            // Register surface for model
            model
                .inner
                .borrow_mut()
                .surfaces
                .push((spec.key, tile.clone().upcast()));

            (tile.upcast::<gtk::Widget>(), btn.upcast::<gtk::Widget>())
        };

    // Split: tile + left + right + divider, with menu open.
    let build_split_tile = |spec: &FeatureSpec, model: &FeaturesModel| -> gtk::Widget {
        let tile = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["qs-tile"])
            .build();

        if spec.active {
            tile.add_css_class("qs-on");
        }

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

        // Left toggles active (close-on-off).
        left_btn.connect_clicked({
            let tile = tile.clone();
            let key = spec.key;
            let status_label = status_label.clone();
            let status_on = spec.status_text.clone();
            let model = model.clone();
            let close_all = close_all.clone();
            let open_key = open_key.clone();
            move |_| {
                let on = tile.has_css_class("qs-on");
                if on {
                    tile.remove_css_class("qs-on");
                    // if this panel is open, close it (close-on-off)
                    if open_key.borrow().as_deref() == Some(key) {
                        close_all();
                    }
                    // best-effort status update if you want "Off"
                    if !status_on.is_empty() {
                        status_label.set_label("Off");
                    }
                    model.set_active(key, false);
                } else {
                    tile.add_css_class("qs-on");
                    if !status_on.is_empty() {
                        status_label.set_label(&status_on);
                    }
                    model.set_active(key, true);
                }
            }
        });

        // Right opens the menu (allowed even while off).
        if let Some(menu) = spec.menu.as_ref() {
            right_btn.connect_clicked({
                let key = spec.key;
                let open_menu = open_menu.clone();
                let menu_widget = menu.widget.clone();
                let key_to_details_y = key_to_details_y.clone();
                move |_| {
                    let details_y = key_to_details_y.borrow().get(&key).copied();
                    if let Some(details_y) = details_y {
                        open_menu(key, &menu_widget, details_y);
                    }
                }
            });
        }

        row.append(&left_btn);
        row.append(&divider);
        row.append(&right_btn);
        tile.append(&row);

        // Register for model updates
        model
            .inner
            .borrow_mut()
            .surfaces
            .push((spec.key, tile.clone().upcast()));
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

        // Place left tile
        let left_widget = if left.menu.is_some() {
            build_split_tile(left, &model)
        } else {
            build_contentless_tile(left, &model).0
        };
        grid.attach(&left_widget, 0, y, 1, 1);

        // Place right tile (or spacer)
        if let Some(r) = right {
            let right_widget = if r.menu.is_some() {
                build_split_tile(r, &model)
            } else {
                build_contentless_tile(r, &model).0
            };
            grid.attach(&right_widget, 1, y, 1, 1);
        } else {
            grid.attach(&gtk::Box::builder().build(), 1, y, 1, 1);
        }

        // Determine whether we need a details row under this tile row.
        let has_menu = left.menu.is_some() || right.map(|r| r.menu.is_some()).unwrap_or(false);
        if has_menu {
            let details_y = y + 1;

            // Create and attach a details row for this tile row.
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

            grid.attach(&revealer, 0, details_y, 2, 1);
            all_detail_rows
                .borrow_mut()
                .push((details_y, revealer.clone(), holder.clone()));

            // Register key -> details_y for any feature in this tile row that has a menu.
            for spec in [Some(left), right].into_iter().flatten() {
                if spec.menu.is_some() {
                    key_to_details_y.borrow_mut().insert(spec.key, details_y);
                }
            }

            // If a feature in this row wants to start open, honor it by opening its menu.
            for spec in [Some(left), right].into_iter().flatten() {
                if spec.open {
                    if let Some(menu) = spec.menu.as_ref() {
                        open_menu(spec.key, &menu.widget, details_y);
                    }
                }
            }

            y = details_y + 1;
        } else {
            y += 1;
        }
    }

    section.append(&grid);
    (section, model)
}
