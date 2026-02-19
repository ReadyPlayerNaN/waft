//! Brightness settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `display` entity type.
//! Renders one preferences group per display with a brightness slider.

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::i18n::t;
use waft_protocol::Urn;
use waft_protocol::entity::display::{DISPLAY_ENTITY_TYPE, Display, DisplayKind};

/// Smart container for brightness settings.
pub struct BrightnessSection {
    pub root: gtk::Box,
}

struct DisplayWidgets {
    group: adw::PreferencesGroup,
    scale: gtk::Scale,
    updating: Rc<Cell<bool>>,
}

impl BrightnessSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .visible(false)
            .build();

        let displays: Rc<RefCell<HashMap<String, DisplayWidgets>>> =
            Rc::new(RefCell::new(HashMap::new()));

        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            let root_ref = root.clone();
            let displays_ref = displays;

            entity_store.subscribe_type(DISPLAY_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, Display)> = store.get_entities_typed(DISPLAY_ENTITY_TYPE);

                let mut map = displays_ref.borrow_mut();
                let mut seen = HashSet::new();

                for (urn, display) in &entities {
                    let urn_str = urn.as_str().to_string();
                    seen.insert(urn_str.clone());

                    if let Some(existing) = map.get(&urn_str) {
                        existing.updating.set(true);
                        existing.group.set_title(&display.name);
                        existing.scale.set_value(display.brightness);
                        let subtitle = match display.kind {
                            DisplayKind::Backlight => t("display-builtin"),
                            DisplayKind::External => t("display-external"),
                        };
                        existing.group.set_description(Some(&subtitle));
                        existing.updating.set(false);
                    } else {
                        let subtitle = match display.kind {
                            DisplayKind::Backlight => t("display-builtin"),
                            DisplayKind::External => t("display-external"),
                        };
                        let group = adw::PreferencesGroup::builder()
                            .title(&display.name)
                            .description(subtitle)
                            .build();

                        let scale = gtk::Scale::builder()
                            .orientation(gtk::Orientation::Horizontal)
                            .hexpand(true)
                            .draw_value(false)
                            .build();
                        scale.set_range(0.0, 1.0);
                        scale.set_increments(0.05, 0.1);
                        scale.set_value(display.brightness);

                        let row = adw::ActionRow::builder().title(t("display-brightness")).build();
                        row.add_suffix(&scale);
                        group.add(&row);

                        let updating = Rc::new(Cell::new(false));

                        let urn_clone = urn.clone();
                        let cb_clone = cb.clone();
                        let guard = updating.clone();
                        scale.connect_value_changed(move |s| {
                            if guard.get() {
                                return;
                            }
                            cb_clone(
                                urn_clone.clone(),
                                "set-brightness".to_string(),
                                serde_json::json!({ "value": s.value() }),
                            );
                        });

                        root_ref.append(&group);
                        map.insert(
                            urn_str,
                            DisplayWidgets {
                                group,
                                scale,
                                updating,
                            },
                        );
                    }
                }

                let to_remove: Vec<String> =
                    map.keys().filter(|k| !seen.contains(*k)).cloned().collect();

                for key in to_remove {
                    if let Some(widgets) = map.remove(&key) {
                        root_ref.remove(&widgets.group);
                    }
                }

                root_ref.set_visible(!map.is_empty());
            });
        }

        Self { root }
    }
}
