//! AI provider usage component.
//!
//! Subscribes to provider-usage entities and renders one card per reported
//! quota window. Providers and windows are discovered from the protocol data,
//! so the overview does not need provider-specific UI code.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::prelude::*;

use waft_client::EntityStore;
use waft_protocol::entity;
use waft_ui_gtk::widgets::info_card::InfoCardWidget;

const PROVIDER_ICON: &str = "applications-science-symbolic";

type CardMap = Rc<RefCell<HashMap<String, Rc<InfoCardWidget>>>>;

/// Renders quota cards for every provider/window currently available.
pub struct ProvidersComponent {
    container: gtk::FlowBox,
    _cards: CardMap,
}

impl ProvidersComponent {
    pub fn new(store: &Rc<EntityStore>) -> Self {
        let container = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .row_spacing(8)
            .column_spacing(16)
            .visible(false)
            .build();
        let cards: CardMap = Rc::new(RefCell::new(HashMap::new()));

        let store_ref = store.clone();
        let container_ref = container.clone();
        let cards_ref = cards.clone();
        let reconcile = move || reconcile(&store_ref, &container_ref, &cards_ref);

        let callback_store = store.clone();
        let callback_container = container.clone();
        let callback_cards = cards.clone();
        store.subscribe_type(entity::ai::ENTITY_TYPE, move || {
            reconcile(&callback_store, &callback_container, &callback_cards);
        });

        // Subscriptions are registered before the initial reconciliation so
        // entities received during startup cannot be missed.
        glib::idle_add_local_once(reconcile);

        Self {
            container,
            _cards: cards,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.container.clone().upcast()
    }
}

fn reconcile(store: &Rc<EntityStore>, container: &gtk::FlowBox, cards: &CardMap) {
    let mut entities =
        store.get_entities_typed::<entity::ai::ProviderUsage>(entity::ai::ENTITY_TYPE);
    entities.sort_by(|(_, left), (_, right)| left.provider.cmp(&right.provider));

    let mut visible_keys = HashSet::new();
    for (_urn, usage) in entities {
        for (index, window) in usage.windows.iter().enumerate() {
            let key = format!("{}:{}:{index}", usage.provider, window.window_type);
            let card = cards
                .borrow_mut()
                .entry(key.clone())
                .or_insert_with(|| {
                    let card = Rc::new(InfoCardWidget::new(PROVIDER_ICON, "", None));
                    container.insert(&card.widget(), -1);
                    card
                })
                .clone();

            let title = if window.limit > 0 {
                let utilization =
                    (window.used as f64 / window.limit as f64 * 100.0).clamp(0.0, 100.0);
                format!("{} {:.0}%", usage.display_name, utilization)
            } else {
                format!("{} {} left", usage.display_name, window.remaining)
            };
            let description = format!(
                "{} · {} · resets {}",
                usage.plan_name,
                window.window_type,
                format_remaining(window.reset_at),
            );

            card.set_title(&title);
            card.set_description(Some(&description));
            card.widget().set_visible(true);
            visible_keys.insert(key);
        }
    }

    for (key, card) in cards.borrow().iter() {
        if !visible_keys.contains(key) {
            card.widget().set_visible(false);
        }
    }
    container.set_visible(!visible_keys.is_empty());
}

/// Format time remaining until reset as a human-readable string.
fn format_remaining(reset_at_ms: Option<i64>) -> String {
    let Some(reset_at_ms) = reset_at_ms else {
        return "never".to_string();
    };

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let remaining_secs = ((reset_at_ms - now_ms) / 1000).max(0);
    let minutes = remaining_secs / 60;
    let hours = minutes / 60;
    let days = hours / 24;

    if days >= 1 {
        format!("{days}d {}h", hours - days * 24)
    } else if hours >= 1 {
        format!("{hours}h {}m", minutes - hours * 60)
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_remaining_without_reset() {
        assert_eq!(format_remaining(None), "never");
    }

    #[test]
    fn format_remaining_minutes() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("expected value")
            .as_millis() as i64;
        assert_eq!(format_remaining(Some(now_ms + 45 * 60 * 1000)), "45m");
    }

    #[test]
    fn format_remaining_hours() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("expected value")
            .as_millis() as i64;
        assert_eq!(
            format_remaining(Some(now_ms + (2 * 3600 + 15 * 60) * 1000)),
            "2h 15m"
        );
    }

    #[test]
    fn format_remaining_days() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("expected value")
            .as_millis() as i64;
        assert_eq!(
            format_remaining(Some(now_ms + (6 * 86400 + 4 * 3600) * 1000)),
            "6d 4h"
        );
    }
}
