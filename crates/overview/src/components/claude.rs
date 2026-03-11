//! Claude Code usage header component.
//!
//! Subscribes to claude-usage entity type and renders two InfoCardWidgets:
//! - Left: 5-hour utilization percentage and time until reset
//! - Right: 7-day utilization percentage and time until reset
//!
//! Hides entirely when no claude-usage entity exists.

use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::prelude::*;

use waft_client::EntityStore;
use waft_protocol::entity;
use waft_ui_gtk::widgets::info_card::InfoCardWidget;

/// Claude icon SVG embedded at compile time, written to XDG_RUNTIME_DIR on first use.
const CLAUDE_ICON_SVG: &[u8] = include_bytes!("../../assets/claude-symbolic.svg");

/// Returns the path to the Claude icon SVG, writing it to a temp dir on first call.
fn claude_icon_path() -> String {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();

    PATH.get_or_init(|| {
        let dir = std::env::var("XDG_RUNTIME_DIR")
            .map(|d| format!("{d}/waft"))
            .unwrap_or_else(|_| "/tmp/waft".to_string());
        std::fs::create_dir_all(&dir).ok();
        let path = format!("{dir}/claude-symbolic.svg");
        std::fs::write(&path, CLAUDE_ICON_SVG).ok();
        path
    })
    .clone()
}

/// Renders two InfoCardWidgets for 5-hour and 7-day Claude Code usage.
pub struct ClaudeComponent {
    container: gtk::Box,
    _five_hour: Rc<InfoCardWidget>,
    _seven_day: Rc<InfoCardWidget>,
}

impl ClaudeComponent {
    pub fn new(store: &Rc<EntityStore>) -> Self {
        let icon_path = claude_icon_path();

        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(16)
            .visible(false)
            .build();

        let five_hour = Rc::new(InfoCardWidget::new(&icon_path, "", None));
        let seven_day = Rc::new(InfoCardWidget::new(&icon_path, "", None));

        container.append(&five_hour.widget());
        container.append(&seven_day.widget());

        let store_ref = store.clone();
        let five_hour_ref = five_hour.clone();
        let seven_day_ref = seven_day.clone();
        let container_ref = container.clone();

        store.subscribe_type(entity::ai::ENTITY_TYPE, move || {
            let entities = store_ref
                .get_entities_typed::<entity::ai::ClaudeUsage>(entity::ai::ENTITY_TYPE);

            match entities.first() {
                Some((_urn, usage)) => {
                    five_hour_ref.set_title(&format!("{:.0}%", usage.five_hour_utilization));
                    five_hour_ref
                        .set_description(Some(&format_remaining(usage.five_hour_reset_at)));

                    seven_day_ref.set_title(&format!("{:.0}%", usage.seven_day_utilization));
                    seven_day_ref
                        .set_description(Some(&format_remaining(usage.seven_day_reset_at)));

                    container_ref.set_visible(true);
                }
                None => {
                    container_ref.set_visible(false);
                }
            }
        });

        Self {
            container,
            _five_hour: five_hour,
            _seven_day: seven_day,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.container.clone().upcast()
    }
}

/// Format time remaining until reset as a human-readable string.
///
/// - `< 1 hour`: `"45m"`
/// - `1h–24h`: `"2h 15m"`
/// - `1d+`: `"6d 4h"`
fn format_remaining(reset_at_ms: i64) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let remaining_secs = ((reset_at_ms - now_ms) / 1000).max(0);
    let minutes = remaining_secs / 60;
    let hours = minutes / 60;
    let days = hours / 24;

    if days >= 1 {
        let remaining_hours = hours - days * 24;
        format!("{days}d {remaining_hours}h")
    } else if hours >= 1 {
        let remaining_mins = minutes - hours * 60;
        format!("{hours}h {remaining_mins}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_remaining_minutes() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let reset = now_ms + 45 * 60 * 1000;
        assert_eq!(format_remaining(reset), "45m");
    }

    #[test]
    fn format_remaining_hours() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let reset = now_ms + (2 * 3600 + 15 * 60) * 1000;
        assert_eq!(format_remaining(reset), "2h 15m");
    }

    #[test]
    fn format_remaining_days() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let reset = now_ms + (6 * 86400 + 4 * 3600) * 1000;
        assert_eq!(format_remaining(reset), "6d 4h");
    }

    #[test]
    fn format_remaining_zero() {
        assert_eq!(format_remaining(0), "0m");
    }
}
