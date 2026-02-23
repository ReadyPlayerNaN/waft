//! Backup toggle component.
//!
//! Subscribes to the `backup-method` entity type and renders a
//! FeatureToggleWidget with an expandable menu listing each backup
//! method as a switchable row. Hidden until entity data arrives
//! from the daemon.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::backup::method_row::{BackupMethodRow, BackupMethodRowProps};
use waft_ui_gtk::menu_state::{menu_id_for_widget, toggle_menu};
use waft_ui_gtk::vdom::Component;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget};

use crate::i18n;
use crate::layout::types::WidgetFeatureToggle;
use crate::ui::feature_toggles::menu::FeatureToggleMenuWidget;
use waft_client::{EntityActionCallback, EntityStore};

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
    menu: FeatureToggleMenuWidget,
    available: Rc<Cell<bool>>,
    #[allow(dead_code)]
    entries: Rc<RefCell<Vec<MethodEntry>>>,
}

impl BackupToggle {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let menu_id = menu_id_for_widget("backup-toggle");
        let available = Rc::new(Cell::new(false));

        let menu_box = FeatureToggleMenuWidget::new();
        let toggle = Rc::new(FeatureToggleWidget::new(
            FeatureToggleProps {
                active: false,
                busy: false,
                details: None,
                expandable: true,
                icon: "drive-harddisk-symbolic".to_string(),
                title: i18n::t("backup-title"),
                menu_id: Some(menu_id.clone()),
                expanded: false,
            },
            Some(menu_store.clone()),
        ));

        let entries: Rc<RefCell<Vec<MethodEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let any_enabled = Rc::new(Cell::new(false));

        // Connect toggle output: enable all or disable all methods
        let cb = action_callback.clone();
        let entries_ref = entries.clone();
        let any_enabled_ref = any_enabled.clone();
        let menu_id_for_expand = menu_id.clone();
        let menu_store_for_expand = menu_store.clone();
        toggle.connect_output(move |output| {
            match output {
                FeatureToggleOutput::Activate | FeatureToggleOutput::Deactivate => {
                    let action = if any_enabled_ref.get() {
                        "disable"
                    } else {
                        "enable"
                    };
                    let borrowed = entries_ref.borrow();
                    for entry in borrowed.iter() {
                        cb(
                            entry.urn.clone(),
                            action.to_string(),
                            serde_json::Value::Null,
                        );
                    }
                }
                FeatureToggleOutput::ExpandToggle(_) => {
                    toggle_menu(&menu_store_for_expand, &menu_id_for_expand);
                }
            }
        });

        // Subscribe to backup-method entity changes
        let store_ref = store.clone();
        let toggle_ref = toggle.clone();
        let available_ref = available.clone();
        let any_enabled_ref = any_enabled.clone();
        let entries_ref = entries.clone();
        let menu_box_ref = menu_box.clone();
        let cb = action_callback.clone();

        store.subscribe_type(entity::storage::BACKUP_METHOD_ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::storage::BackupMethod)> =
                store_ref.get_entities_typed(entity::storage::BACKUP_METHOD_ENTITY_TYPE);

            let was_available = available_ref.get();
            let now_available = !entities.is_empty();

            // Update toggle active state: active if any method is enabled
            let currently_enabled = entities.iter().any(|(_, m)| m.enabled);
            any_enabled_ref.set(currently_enabled);
            toggle_ref.set_active(currently_enabled);

            let count = entities.len();
            toggle_ref.set_details(if currently_enabled {
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
                    menu_box_ref.remove(&entry.row.widget());
                }

                // Add new rows
                for (urn, method) in &entities {
                    let row = BackupMethodRow::build(&BackupMethodRowProps {
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

                    menu_box_ref.append(&row.widget());
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
            menu: menu_box,
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
            menu: Some(self.menu.widget().clone()),
        })]
    }
}
