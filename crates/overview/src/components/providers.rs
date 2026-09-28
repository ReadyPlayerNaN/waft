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
use waft_ui_gtk::links::open_uri;
use waft_ui_gtk::widgets::info_card::InfoCardWidget;

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

        let callback_store = store.clone();
        let callback_container = container.clone();
        let callback_cards = cards.clone();
        let reconcile_callback = move || {
            reconcile(&callback_store, &callback_container, &callback_cards);
        };
        store.subscribe_type(entity::ai::ENTITY_TYPE, reconcile_callback.clone());
        store.subscribe_type(entity::ai::CONFIG_ENTITY_TYPE, reconcile_callback);

        // Subscriptions are registered before the initial reconciliation so
        // entities received during startup cannot be missed.
        let initial_store = store.clone();
        let initial_container = container.clone();
        let initial_cards = cards.clone();
        glib::idle_add_local_once(move || {
            reconcile(&initial_store, &initial_container, &initial_cards);
        });

        // Keep reset countdowns current even when the provider daemon is
        // retaining the last successful quota response.
        let timer_store = store.clone();
        let timer_container = container.clone();
        let timer_cards = cards.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(60), move || {
            reconcile(&timer_store, &timer_container, &timer_cards);
            glib::ControlFlow::Continue
        });

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
    let display_usage = store
        .get_entities_typed::<entity::ai::ProviderConfig>(entity::ai::CONFIG_ENTITY_TYPE)
        .into_iter()
        .next()
        .map(|(_, config)| config.display_usage)
        .unwrap_or(true);

    let mut visible_keys = HashSet::new();
    for (urn, usage) in entities {
        for (index, window) in usage.windows.iter().enumerate() {
            // A reset-less limit is not actionable and should never appear,
            // even if an older daemon published one before filtering it.
            if window.reset_at.is_none() {
                continue;
            }

            let Some(percent_value) = (if display_usage {
                percentage(window.used, window.limit)
            } else {
                percentage(window.remaining, window.limit)
            }) else {
                // Balance-only windows have no meaningful percentage card.
                continue;
            };
            let key = format!("{urn}:{}:{index}", window.window_type);
            let icon = provider_icon(&usage.provider);
            let usage_url = usage.usage_url.clone();
            let card = cards
                .borrow_mut()
                .entry(key.clone())
                .or_insert_with(|| {
                    let card = Rc::new(InfoCardWidget::new(&icon, "", None));
                    if let Some(url) = usage_url {
                        let gesture = gtk::GestureClick::new();
                        gesture.connect_released(move |_, _, _, _| open_uri(&url));
                        card.widget().add_controller(gesture);
                        card.widget().set_cursor_from_name(Some("pointer"));
                    }
                    container.insert(&card.widget(), -1);
                    card
                })
                .clone();

            let reset = format_remaining(window.reset_at);
            let details = format!(
                "{}\nPlan: {}\n{}\nUsed: {} / {}\nRemaining: {}\nResets: {}\nUpdated: {}",
                usage.display_name,
                usage.plan_name,
                window.window_type,
                window.used,
                window.limit,
                window.remaining,
                reset,
                format_freshness(usage.fetched_at)
            );
            card.set_icon(&icon);
            card.set_title(&format_percentage(percent_value));
            card.set_description(Some(&reset));
            card.widget().set_tooltip_text(Some(&details));
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

fn percentage(value: i64, limit: i64) -> Option<f64> {
    (limit > 0).then(|| (value as f64 / limit as f64 * 100.0).clamp(0.0, 100.0))
}

fn format_percentage(value: f64) -> String {
    format!("{value:.0}\u{00a0}%")
}

fn format_freshness(fetched_at_ms: i64) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let age_secs = ((now_ms - fetched_at_ms) / 1000).max(0);
    if age_secs < 60 {
        "just now".to_string()
    } else if age_secs < 3600 {
        format!("{}m ago", age_secs / 60)
    } else {
        format!("{}h ago", age_secs / 3600)
    }
}
fn provider_icon(provider: &str) -> String {
    let (slug, svg): (&str, &[u8]) = match provider {
        "claude" => (
            "claude",
            include_bytes!("../../assets/providers/claude.svg"),
        ),
        "codex" => ("codex", include_bytes!("../../assets/providers/codex.svg")),
        "cursor" => (
            "cursor",
            include_bytes!("../../assets/providers/cursor.svg"),
        ),
        "deepseek" => (
            "deepseek",
            include_bytes!("../../assets/providers/deepseek.svg"),
        ),
        "antigravity" => (
            "antigravity",
            include_bytes!("../../assets/providers/antigravity.svg"),
        ),
        "github-copilot" => (
            "github-copilot",
            include_bytes!("../../assets/providers/github-copilot.svg"),
        ),
        "grok" => ("grok", include_bytes!("../../assets/providers/grok.svg")),
        "kimi" => ("kimi", include_bytes!("../../assets/providers/kimi.svg")),
        "minimax" => (
            "minimax",
            include_bytes!("../../assets/providers/minimax.svg"),
        ),
        "openrouter" => (
            "openrouter",
            include_bytes!("../../assets/providers/openrouter.svg"),
        ),
        "siliconflow" => (
            "siliconflow",
            include_bytes!("../../assets/providers/siliconflow.svg"),
        ),
        "zai" => ("zai", include_bytes!("../../assets/providers/zai.svg")),
        "mimo" => ("mimo", include_bytes!("../../assets/providers/mimo.svg")),
        _ => return "applications-science-symbolic".to_string(),
    };
    bundled_provider_icon_path(slug, svg)
}

fn bundled_provider_icon_path(slug: &str, svg: &[u8]) -> String {
    use std::sync::OnceLock;

    let path = format!(
        "{}/provider-{slug}.svg",
        std::env::var("XDG_RUNTIME_DIR")
            .map(|path| format!("{path}/waft"))
            .unwrap_or_else(|_| "/tmp/waft".to_string())
    );
    static WRITTEN: OnceLock<std::sync::Mutex<HashSet<String>>> = OnceLock::new();
    let written = WRITTEN.get_or_init(|| std::sync::Mutex::new(HashSet::new()));
    if written
        .lock()
        .map(|set| set.contains(&path))
        .unwrap_or(false)
    {
        return path;
    }

    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(&path, svg).is_ok()
        && let Ok(mut set) = written.lock()
    {
        set.insert(path.clone());
    }
    path
}

/// Format time remaining until reset as a human-readable string.
fn format_remaining(reset_at_ms: Option<i64>) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    format_remaining_at(reset_at_ms, now_ms)
}

fn format_remaining_at(reset_at_ms: Option<i64>, now_ms: i64) -> String {
    let Some(reset_at_ms) = reset_at_ms else {
        return "never".to_string();
    };

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
    fn percentage_format_uses_a_non_breaking_space() {
        assert_eq!(format_percentage(0.0), "0\u{00a0}%");
        assert_eq!(format_percentage(42.0), "42\u{00a0}%");
    }

    #[test]
    fn balance_only_windows_do_not_become_zero_percent_cards() {
        assert_eq!(percentage(25, 0), None);
        assert_eq!(percentage(25, -1), None);
        assert_eq!(percentage(25, 100), Some(25.0));
    }

    #[test]
    fn provider_icons_are_bundled_per_service() {
        for provider in [
            "claude",
            "codex",
            "cursor",
            "deepseek",
            "antigravity",
            "github-copilot",
            "grok",
            "kimi",
            "minimax",
            "openrouter",
            "siliconflow",
            "zai",
            "mimo",
        ] {
            let path = provider_icon(provider);
            assert!(path.ends_with(&format!("provider-{provider}.svg")));
            assert!(std::path::Path::new(&path).is_file());
        }
    }

    #[test]
    fn format_remaining_minutes() {
        let now_ms = 1_000_000_000_000;
        assert_eq!(
            format_remaining_at(Some(now_ms + 45 * 60 * 1000), now_ms),
            "45m"
        );
    }

    #[test]
    fn format_remaining_hours() {
        let now_ms = 1_000_000_000_000;
        assert_eq!(
            format_remaining_at(Some(now_ms + (2 * 3600 + 15 * 60) * 1000), now_ms),
            "2h 15m"
        );
    }

    #[test]
    fn format_remaining_days() {
        let now_ms = 1_000_000_000_000;
        assert_eq!(
            format_remaining_at(Some(now_ms + (6 * 86400 + 4 * 3600) * 1000), now_ms),
            "6d 4h"
        );
    }
}
