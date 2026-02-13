//! Backup toggle component.
//!
//! Subscribes to the `backup-method` entity type and renders a
//! FeatureToggleWidget with an expandable menu listing each backup
//! method as a switchable row. Hidden until entity data arrives
//! from the daemon.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::backup::method_row::{BackupMethodRow, BackupMethodRowProps};
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::i18n;
use crate::plugin::WidgetFeatureToggle;

/// A backup method row with its associated URN for action routing.
struct MethodEntry {
    urn: Urn,
    row: BackupMethodRow,
}

/// Toggle for backup services (syncthing, etc.).
///
/// Reports zero toggles until at least one backup-method entity arrives.
/// Shows a single "Backup" feature toggle with an expandable menu of methods.
pub struct BackupToggle {
    toggle: Rc<FeatureToggleWidget>,
    menu_box: gtk::Box,
    available: Rc<Cell<bool>>,
    entries: Rc<RefCell<Vec<MethodEntry>>>,
}

impl BackupToggle {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let available = Rc::new(Cell::new(false));

        let menu_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        let toggle = Rc::new(FeatureToggleWidget::new(
            FeatureToggleProps {
                active: false,
                busy: false,
                details: None,
                expandable: true,
                icon: "drive-harddisk-symbolic".to_string(),
                title: i18n::t("backup-title"),
                menu_id: None,
            },
            None,
        ));

        let entries: Rc<RefCell<Vec<MethodEntry>>> = Rc::new(RefCell::new(Vec::new()));

        // Connect toggle output: toggle the first method when the main toggle is clicked
        let cb = action_callback.clone();
        let entries_ref = entries.clone();
        toggle.connect_output(move |_output| {
            let borrowed = entries_ref.borrow();
            if let Some(entry) = borrowed.first() {
                cb(
                    entry.urn.clone(),
                    "toggle".to_string(),
                    serde_json::Value::Null,
                );
            }
        });

        // Subscribe to backup-method entity changes
        let store_ref = store.clone();
        let toggle_ref = toggle.clone();
        let available_ref = available.clone();
        let entries_ref = entries.clone();
        let menu_box_ref = menu_box.clone();
        let cb = action_callback.clone();

        store.subscribe_type(entity::storage::BACKUP_METHOD_ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::storage::BackupMethod)> =
                store_ref.get_entities_typed(entity::storage::BACKUP_METHOD_ENTITY_TYPE);

            let was_available = available_ref.get();
            let now_available = !entities.is_empty();

            // Update toggle active state: active if any method is enabled
            let any_enabled = entities.iter().any(|(_, m)| m.enabled);
            toggle_ref.set_active(any_enabled);

            let count = entities.len();
            toggle_ref.set_details(if any_enabled {
                Some(format!(
                    "{count} {}",
                    if count == 1 { "service" } else { "services" }
                ))
            } else {
                None
            });

            // Rebuild method rows
            {
                let mut borrowed = entries_ref.borrow_mut();

                // Remove old rows
                for entry in borrowed.drain(..) {
                    menu_box_ref.remove(&entry.row.root);
                }

                // Add new rows
                for (urn, method) in &entities {
                    let row = BackupMethodRow::new(BackupMethodRowProps {
                        icon: method.icon.clone(),
                        name: method.name.clone(),
                        enabled: method.enabled,
                    });

                    let row_urn = urn.clone();
                    let row_cb = cb.clone();
                    row.connect_output(move |_output| {
                        row_cb(
                            row_urn.clone(),
                            "toggle".to_string(),
                            serde_json::Value::Null,
                        );
                    });

                    menu_box_ref.append(&row.root);
                    borrowed.push(MethodEntry {
                        urn: urn.clone(),
                        row,
                    });
                }
            }

            if was_available != now_available {
                available_ref.set(now_available);
                rebuild_callback();
            }
        });

        Self {
            toggle,
            menu_box,
            available,
            entries,
        }
    }

    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        if !self.available.get() {
            return Vec::new();
        }
        vec![Rc::new(WidgetFeatureToggle {
            id: "backup-toggle".to_string(),
            weight: 350,
            toggle: (*self.toggle).clone(),
            menu: Some(self.menu_box.clone().upcast()),
        })]
    }
}
