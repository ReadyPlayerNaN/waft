//! AI provider usage component.
//!
//! Subscribes to provider-usage entities and renders one card per reported
//! quota window. Providers and windows are discovered from the protocol data,
//! so the overview does not need provider-specific UI code.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::prelude::*;

use crate::i18n::{t, t_args};
use waft_client::EntityStore;
use waft_protocol::entity;
use waft_ui_gtk::icons::{Icon, IconWidget};
use waft_ui_gtk::links::open_uri;

type CardMap = Rc<RefCell<HashMap<String, Rc<ProviderCard>>>>;

const CAPACITY_WIDTH: i32 = 16;
const CAPACITY_HEIGHT: i32 = 32;
const RESET_HEIGHT: i32 = 4;

struct QuotaWindowChart {
    root: gtk::Box,
    capacity: Rc<Cell<Option<f64>>>,
    reset_remaining: Rc<Cell<Option<f64>>>,
    label: gtk::Label,
}

impl QuotaWindowChart {
    fn new(label: &str) -> Rc<Self> {
        let capacity = Rc::new(Cell::new(None));
        let capacity_state = capacity.clone();
        let capacity_area = gtk::DrawingArea::builder()
            .content_width(CAPACITY_WIDTH)
            .content_height(CAPACITY_HEIGHT)
            .build();
        capacity_area.set_draw_func(move |_area, cr, width, height| {
            draw_capacity_bar(cr, width as f64, height as f64, capacity_state.get());
        });

        let reset_remaining = Rc::new(Cell::new(None));
        let reset_state = reset_remaining.clone();
        let reset_area = gtk::DrawingArea::builder()
            .content_width(CAPACITY_WIDTH)
            .content_height(RESET_HEIGHT)
            .build();
        reset_area.set_draw_func(move |_area, cr, width, height| {
            draw_reset_bar(cr, width as f64, height as f64, reset_state.get());
        });

        let label_widget = gtk::Label::new(Some(label));
        label_widget.set_css_classes(&["caption", "provider-quota-label"]);
        label_widget.set_halign(gtk::Align::Center);
        label_widget.set_valign(gtk::Align::End);
        label_widget.set_margin_bottom(1);
        label_widget.set_width_request(CAPACITY_WIDTH);

        let capacity_overlay = gtk::Overlay::new();
        capacity_overlay.set_size_request(CAPACITY_WIDTH, CAPACITY_HEIGHT);
        capacity_overlay.set_child(Some(&capacity_area));
        capacity_overlay.add_overlay(&label_widget);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 1);
        root.set_css_classes(&["provider-quota-chart"]);
        root.set_size_request(CAPACITY_WIDTH, CAPACITY_HEIGHT + RESET_HEIGHT + 1);
        root.append(&capacity_overlay);
        root.append(&reset_area);

        Rc::new(Self {
            root,
            capacity,
            reset_remaining,
            label: label_widget,
        })
    }

    fn update(&self, window: &entity::ai::ProviderUsageWindow, now_ms: i64, details: &str) {
        self.label.set_label(&short_window_label(window));
        self.capacity.set(capacity_fraction(window));
        self.reset_remaining
            .set(reset_remaining_fraction(window, now_ms));
        self.root.set_tooltip_text(Some(details));
        self.root.queue_draw();
        if let Some(parent) = self.root.parent() {
            parent.queue_draw();
        }
    }
}

struct ProviderCard {
    root: gtk::Box,
    windows_box: gtk::Box,
    windows: RefCell<HashMap<String, Rc<QuotaWindowChart>>>,
}

impl ProviderCard {
    fn new(icon: &str, usage_url: Option<String>) -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        root.set_valign(gtk::Align::Center);

        let provider_icon = IconWidget::new(&[Icon::parse(icon)], 32);
        root.append(provider_icon.widget());

        let windows_box = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        windows_box.set_valign(gtk::Align::Center);
        root.append(&windows_box);

        if let Some(url) = usage_url {
            let gesture = gtk::GestureClick::new();
            gesture.connect_released(move |_, _, _, _| open_uri(&url));
            root.add_controller(gesture);
            root.set_cursor_from_name(Some("pointer"));
        }

        Rc::new(Self {
            root,
            windows_box,
            windows: RefCell::new(HashMap::new()),
        })
    }

    fn update_window(
        &self,
        key: &str,
        window: &entity::ai::ProviderUsageWindow,
        now_ms: i64,
        details: &str,
    ) {
        let chart = self
            .windows
            .borrow_mut()
            .entry(key.to_string())
            .or_insert_with(|| {
                let chart = QuotaWindowChart::new(&short_window_label(window));
                self.windows_box.append(&chart.root);
                chart
            })
            .clone();
        chart.update(window, now_ms, details);
        chart.root.set_visible(true);
    }

    fn hide_missing_windows(&self, visible_keys: &HashSet<String>) {
        for (key, chart) in self.windows.borrow().iter() {
            chart.root.set_visible(visible_keys.contains(key));
        }
    }
}

fn draw_capacity_bar(cr: &gtk::cairo::Context, width: f64, height: f64, remaining: Option<f64>) {
    let Some(remaining) = remaining else {
        draw_rect(cr, 0.0, 0.0, width, height, (0.30, 0.30, 0.33, 1.0));
        return;
    };

    draw_rect(cr, 0.0, 0.0, width, height, (0.82, 0.16, 0.22, 1.0));
    let green_height = height * remaining;
    draw_rect(
        cr,
        0.0,
        height - green_height,
        width,
        green_height,
        (0.20, 0.68, 0.32, 1.0),
    );
}

fn draw_reset_bar(cr: &gtk::cairo::Context, width: f64, height: f64, remaining: Option<f64>) {
    let Some(remaining) = remaining else {
        draw_rect(cr, 0.0, 0.0, width, height, (0.30, 0.30, 0.33, 1.0));
        return;
    };

    draw_rect(cr, 0.0, 0.0, width, height, (0.92, 0.47, 0.10, 1.0));
    let blue_width = width * remaining;
    draw_rect(
        cr,
        width - blue_width,
        0.0,
        blue_width,
        height,
        (0.20, 0.48, 0.90, 1.0),
    );
}

fn draw_rect(
    cr: &gtk::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: (f64, f64, f64, f64),
) {
    cr.set_source_rgba(color.0, color.1, color.2, color.3);
    cr.rectangle(x, y, width.max(0.0), height.max(0.0));
    if let Err(error) = cr.fill() {
        log::warn!("provider quota chart draw failed: {error}");
    }
}

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
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut visible_cards = HashSet::new();
    for (urn, usage) in entities {
        let card_key = urn.to_string();
        let card = if let Some(card) = cards.borrow().get(&card_key).cloned() {
            card
        } else {
            let card = ProviderCard::new(&provider_icon(&usage.provider), usage.usage_url.clone());
            container.insert(&card.root, -1);
            cards.borrow_mut().insert(card_key.clone(), card.clone());
            card
        };

        let mut visible_windows = HashSet::new();
        let mut tooltip_sections = Vec::new();
        for (index, window) in usage.windows.iter().enumerate() {
            // A positive limit is enough to display a capacity bar. Reset-less
            // windows remain visible; their reset bar shows the unknown state.
            if capacity_fraction(window).is_none() {
                continue;
            }

            let key = format!("{}:{index}", window.window_type);
            let reset = format_remaining(window.reset_at);
            let details = format!(
                "{}: {}\n{}: {}\n{}: {}\n{}: {} / {}\n{}: {}\n{}: {}\n{}: {}",
                t("providers-tooltip-provider"),
                usage.display_name,
                t("providers-tooltip-plan"),
                usage.plan_name,
                t("providers-tooltip-window"),
                short_window_label(window),
                t("providers-tooltip-used"),
                format_quota_value(window.used, window.percentage),
                format_quota_value(window.limit, window.percentage),
                t("providers-tooltip-remaining"),
                format_quota_value(window.remaining, window.percentage),
                t("providers-tooltip-resets"),
                reset,
                t("providers-tooltip-updated"),
                format_freshness(usage.fetched_at)
            );
            card.update_window(&key, window, now_ms, &details);
            visible_windows.insert(key);
            tooltip_sections.push(details);
        }

        card.hide_missing_windows(&visible_windows);
        let tooltip = tooltip_sections.join("\n\n");
        card.root
            .set_tooltip_text((!tooltip.is_empty()).then_some(tooltip.as_str()));
        card.root.set_visible(!visible_windows.is_empty());
        if !visible_windows.is_empty() {
            visible_cards.insert(card_key);
        }
    }

    for (key, card) in cards.borrow().iter() {
        if !visible_cards.contains(key) {
            card.root.set_visible(false);
        }
    }
    container.set_visible(!visible_cards.is_empty());
}

fn capacity_fraction(window: &entity::ai::ProviderUsageWindow) -> Option<f64> {
    percentage(window.remaining, window.limit)
}

fn reset_remaining_fraction(window: &entity::ai::ProviderUsageWindow, now_ms: i64) -> Option<f64> {
    let period_seconds = window.period_seconds.filter(|seconds| *seconds > 0)?;
    let Some(reset_at) = window.reset_at else {
        // A missing reset means the provider has not started this interval yet;
        // show the complete interval as remaining.
        return Some(1.0);
    };
    let period_ms = period_seconds.saturating_mul(1_000);
    Some(((reset_at - now_ms) as f64 / period_ms as f64).clamp(0.0, 1.0))
}

fn percentage(value: i64, limit: i64) -> Option<f64> {
    (limit > 0).then(|| (value as f64 / limit as f64).clamp(0.0, 1.0))
}

fn short_window_label(window: &entity::ai::ProviderUsageWindow) -> String {
    if let Some(seconds) = window.period_seconds.filter(|seconds| *seconds > 0) {
        if seconds % 86_400 == 0 {
            return format!("{}{}", seconds / 86_400, t("providers-unit-day"));
        }
        if seconds % 3_600 == 0 {
            return format!("{}{}", seconds / 3_600, t("providers-unit-hour"));
        }
        if seconds % 60 == 0 {
            return format!("{}{}", seconds / 60, t("providers-unit-minute"));
        }
        return format!("{}{}", seconds, t("providers-unit-second"));
    }

    let raw = window.window_type.to_ascii_lowercase();
    for part in raw.split('/') {
        let digit_end = part
            .char_indices()
            .take_while(|(_, character)| character.is_ascii_digit())
            .map(|(index, character)| index + character.len_utf8())
            .last()
            .unwrap_or(0);
        let suffix = &part[digit_end..];
        if digit_end > 0 && ["s", "m", "h", "d"].contains(&suffix) {
            return format!(
                "{}{}",
                &part[..digit_end],
                t(&format!("providers-unit-{suffix}"))
            );
        }
    }

    if raw.contains("week") || raw == "wk" {
        format!("7{}", t("providers-unit-day"))
    } else if raw.contains("month") {
        format!("30{}", t("providers-unit-day"))
    } else {
        t("providers-unit-unknown")
    }
}

fn format_quota_value(value: i64, percentage: bool) -> String {
    if percentage {
        format!("{value}\u{00a0}%")
    } else {
        value.to_string()
    }
}

fn format_freshness(fetched_at_ms: i64) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let age_secs = ((now_ms - fetched_at_ms) / 1000).max(0);
    if age_secs < 60 {
        t("providers-tooltip-just-now")
    } else if age_secs < 3600 {
        let minutes = (age_secs / 60).to_string();
        t_args("providers-tooltip-minutes-ago", &[("minutes", &minutes)])
    } else {
        let hours = (age_secs / 3600).to_string();
        t_args("providers-tooltip-hours-ago", &[("hours", &hours)])
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
        return t("providers-tooltip-reset-not-started");
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
        assert_eq!(
            format_remaining(None),
            t("providers-tooltip-reset-not-started")
        );
    }

    #[test]
    fn missing_reset_starts_with_a_full_time_bar() {
        let window = entity::ai::ProviderUsageWindow {
            window_type: "5h".to_string(),
            used: 0,
            limit: 100,
            remaining: 100,
            reset_at: None,
            percentage: true,
            period_seconds: Some(5 * 3_600),
        };
        assert_eq!(reset_remaining_fraction(&window, 1_000), Some(1.0));
    }

    #[test]
    fn tooltip_values_distinguish_percentages_from_credits() {
        assert_eq!(format_quota_value(42, true), "42\u{00a0}%");
        assert_eq!(format_quota_value(42, false), "42");
    }

    #[test]
    fn balance_only_windows_do_not_become_zero_percent_cards() {
        assert_eq!(percentage(25, 0), None);
        assert_eq!(percentage(25, -1), None);
        assert_eq!(percentage(25, 100), Some(0.25));
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
    fn period_labels_use_a_number_and_single_unit_letter() {
        let window = entity::ai::ProviderUsageWindow {
            window_type: "weekly".to_string(),
            used: 1,
            limit: 100,
            remaining: 99,
            reset_at: None,
            percentage: true,
            period_seconds: Some(7 * 86_400),
        };
        assert_eq!(short_window_label(&window), "7d");
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
