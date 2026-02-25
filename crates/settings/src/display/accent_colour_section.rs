//! Accent colour settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `gtk-appearance` entity type.
//! Displays a colour swatch grid for selecting the system accent colour.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::appearance::{GTK_APPEARANCE_ENTITY_TYPE, GtkAppearance};
use waft_ui_gtk::icons::icon::IconWidget;

use crate::i18n::t;
use crate::search_index::SearchIndex;

/// The 9 predefined accent colours: (gsettings_value, i18n_key, hex_color).
const ACCENT_COLOURS: &[(&str, &str, &str)] = &[
    ("blue", "display-accent-blue", "#3584e4"),
    ("teal", "display-accent-teal", "#2190a4"),
    ("green", "display-accent-green", "#3a944a"),
    ("yellow", "display-accent-yellow", "#c88800"),
    ("orange", "display-accent-orange", "#ed5b00"),
    ("red", "display-accent-red", "#e62d42"),
    ("pink", "display-accent-pink", "#d56199"),
    ("purple", "display-accent-purple", "#9141ac"),
    ("slate", "display-accent-slate", "#6f8396"),
];

/// Smart container for accent colour selection.
pub struct AccentColourSection {
    pub root: adw::PreferencesGroup,
}

impl AccentColourSection {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        // Load colour swatch CSS display-wide
        load_swatch_css();

        let group = adw::PreferencesGroup::builder()
            .title(t("display-accent-colour"))
            .visible(false)
            .build();

        let flow_box = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(9)
            .min_children_per_line(3)
            .homogeneous(true)
            .row_spacing(8)
            .column_spacing(8)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        group.add(&flow_box);

        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // Build swatch buttons with checkmark overlays
        let swatches: Rc<Vec<SwatchEntry>> = Rc::new(
            ACCENT_COLOURS
                .iter()
                .map(|(value, i18n_key, _hex)| {
                    let overlay = gtk::Overlay::new();

                    let button = gtk::Button::builder()
                        .width_request(36)
                        .height_request(36)
                        .halign(gtk::Align::Center)
                        .valign(gtk::Align::Center)
                        .tooltip_text(t(i18n_key))
                        .build();
                    button.add_css_class("circular");
                    button.add_css_class("flat");
                    button.add_css_class(&format!("accent-swatch-{}", value));

                    overlay.set_child(Some(&button));

                    // Checkmark overlay (hidden by default)
                    let check = IconWidget::from_name("object-select-symbolic", 16);
                    let check_widget = check.widget().clone();
                    check_widget.set_halign(gtk::Align::Center);
                    check_widget.set_valign(gtk::Align::Center);
                    check_widget.set_visible(false);
                    overlay.add_overlay(&check_widget);

                    // Wire click to dispatch action
                    let cb = action_callback.clone();
                    let urn_ref = current_urn.clone();
                    let color_value = value.to_string();
                    button.connect_clicked(move |_| {
                        if let Some(ref urn) = *urn_ref.borrow() {
                            cb(
                                urn.clone(),
                                "set-accent-color".to_string(),
                                serde_json::json!({ "color": color_value }),
                            );
                        }
                    });

                    flow_box.append(&overlay);

                    SwatchEntry {
                        color: value.to_string(),
                        check: check_widget,
                    }
                })
                .collect(),
        );

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-appearance");
            let section_title = t("display-accent-colour");
            idx.add_section(
                "appearance",
                &page_title,
                &section_title,
                "display-accent-colour",
                &group,
            );
            for (_value, i18n_key, _hex) in ACCENT_COLOURS {
                idx.add_input(
                    "appearance",
                    &page_title,
                    &section_title,
                    &t(i18n_key),
                    i18n_key,
                    &flow_box,
                );
            }
        }

        // Subscribe to gtk-appearance entity updates
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let urn_ref = current_urn.clone();
            let sw = swatches.clone();

            entity_store.subscribe_type(GTK_APPEARANCE_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, GtkAppearance)> =
                    store.get_entities_typed(GTK_APPEARANCE_ENTITY_TYPE);

                if let Some((urn, appearance)) = entities.first() {
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);
                    update_checkmarks(&sw, &appearance.accent_color);
                } else {
                    group_ref.set_visible(false);
                }
            });
        }

        // Initial reconciliation with cached data
        {
            let store_clone = entity_store.clone();
            let group_ref = group.clone();
            let urn_ref = current_urn;
            let sw = swatches;

            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, GtkAppearance)> =
                    store_clone.get_entities_typed(GTK_APPEARANCE_ENTITY_TYPE);

                if let Some((urn, appearance)) = entities.first() {
                    log::debug!("[accent-colour] Initial reconciliation with cached data");
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);
                    update_checkmarks(&sw, &appearance.accent_color);
                }
            });
        }

        Self { root: group }
    }
}

struct SwatchEntry {
    color: String,
    check: gtk::Image,
}

/// Show checkmark on the active colour swatch, hide on all others.
fn update_checkmarks(swatches: &[SwatchEntry], active_color: &str) {
    for entry in swatches {
        entry.check.set_visible(entry.color == active_color);
    }
}

/// Load CSS for accent colour swatches. Safe to call multiple times.
fn load_swatch_css() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut css = String::new();
        for (value, _i18n_key, hex) in ACCENT_COLOURS {
            css.push_str(&format!(
                ".accent-swatch-{} {{ background-color: {}; min-width: 36px; min-height: 36px; }}\n",
                value, hex
            ));
        }

        let provider = gtk::CssProvider::new();
        provider.load_from_data(&css);

        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        } else {
            log::warn!("[accent-colour] Failed to load swatch CSS: no display found");
        }
    });
}
