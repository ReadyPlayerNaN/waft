//! Agenda event card widget — top row + expandable details.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use waft_plugin_api::common::Callback;
use crate::values::{AgendaEvent, extract_meeting_links};
use waft_core::menu_state::MenuStore;
// Removed: trigger_window_resize not available in plugin context
use waft_plugin_api::ui::menu_chevron::{MenuChevronProps, MenuChevronWidget};

use super::agenda_details::AgendaDetails;
use super::format::format_time_range;
use super::meeting_button::MeetingButton;

/// Output events from an agenda card.
#[derive(Debug, Clone)]
pub enum AgendaCardOutput {
    /// The user clicked the expand chevron.
    ToggleExpand(String),
}

/// A single agenda event card with optional expand/details.
pub struct AgendaCard {
    pub root: gtk::Box,
    menu_chevron: Option<MenuChevronWidget>,
    revealer: Option<gtk::Revealer>,
    is_past: bool,
    on_output: Callback<AgendaCardOutput>,
    menu_id: String,
}

impl AgendaCard {
    pub fn new(
        event: &AgendaEvent,
        is_past: bool,
        is_ongoing: bool,
        menu_store: &Rc<MenuStore>,
    ) -> Self {
        let mut css_classes: Vec<&str> = vec!["agenda-event-card"];
        if is_past {
            css_classes.push("agenda-event-past");
        }
        if is_ongoing {
            css_classes.push("agenda-event-ongoing");
        }

        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .css_classes(css_classes)
            .build();

        // Top row: time + summary + meeting btn + expand chevron
        let top_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        // Time label (fixed width for alignment)
        let time_text = if event.all_day {
            waft_plugin_api::i18n::t("agenda-all-day")
        } else {
            format_time_range(event.start_time, event.end_time)
        };

        let time_label = gtk::Label::builder()
            .label(&time_text)
            .xalign(0.0)
            .width_chars(13)
            .css_classes(["dim-label", "agenda-event-time", "caption"])
            .build();

        // Summary label (ellipsized, takes remaining space)
        let summary_text = if event.summary.trim().is_empty() {
            waft_plugin_api::i18n::t("agenda-no-title")
        } else {
            event.summary.clone()
        };

        let summary_label = gtk::Label::builder()
            .label(&summary_text)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["agenda-event-summary"])
            .build();

        top_row.append(&time_label);
        top_row.append(&summary_label);

        // Meeting link action widget
        let links = extract_meeting_links(event);
        if let Some(meeting_btn) = MeetingButton::new(&links, menu_store) {
            top_row.append(meeting_btn.widget());
        }

        let menu_id = format!("agenda-detail:{}", event.occurrence_key());
        let on_output: Callback<AgendaCardOutput> = Rc::new(RefCell::new(None));

        let mut menu_chevron_out = None;
        let mut revealer_out = None;

        // Expand chevron + revealer (only if event has details)
        if event.has_details() {
            let menu_chevron = MenuChevronWidget::new(MenuChevronProps { expanded: false });
            let expand_btn = gtk::Button::builder()
                .css_classes(["flat", "circular", "agenda-expand-btn"])
                .build();
            expand_btn.set_child(menu_chevron.widget());
            top_row.append(&expand_btn);

            card.append(&top_row);

            // Revealer for detail content
            let revealer = gtk::Revealer::builder()
                .transition_type(gtk::RevealerTransitionType::SlideDown)
                .transition_duration(200)
                .reveal_child(false)
                .build();

            let details = AgendaDetails::new(event);
            revealer.set_child(Some(&details.root));
            card.append(&revealer);

            // Click handler emits ToggleExpand via on_output
            let on_output_click = on_output.clone();
            let menu_id_click = menu_id.clone();
            expand_btn.connect_clicked(move |_| {
                if let Some(ref callback) = *on_output_click.borrow() {
                    callback(AgendaCardOutput::ToggleExpand(menu_id_click.clone()));
                }
            });

            // Trigger window resize when animation completes
            revealer.connect_child_revealed_notify(move |_| {
                // Removed: trigger_window_resize() not available in plugin context
            });

            // Sync initial state
            {
                let state = menu_store.get_state();
                let should_be_open = state.active_menu_id.as_deref() == Some(&menu_id);
                menu_chevron.set_expanded(should_be_open);
                revealer.set_reveal_child(should_be_open);
                if is_past && should_be_open {
                    card.remove_css_class("agenda-event-past");
                }
            }

            menu_chevron_out = Some(menu_chevron);
            revealer_out = Some(revealer);
        } else {
            card.append(&top_row);
        }

        Self {
            root: card,
            menu_chevron: menu_chevron_out,
            revealer: revealer_out,
            is_past,
            on_output,
            menu_id,
        }
    }

    /// Register a callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(AgendaCardOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Get the menu ID for this card.
    pub fn menu_id(&self) -> &str {
        &self.menu_id
    }

    /// Set expanded state — updates chevron, revealer, and past dimming.
    pub fn set_expanded(&self, expanded: bool) {
        if let Some(ref chevron) = self.menu_chevron {
            chevron.set_expanded(expanded);
        }
        if let Some(ref revealer) = self.revealer {
            revealer.set_reveal_child(expanded);
        }
        if self.is_past {
            if expanded {
                self.root.remove_css_class("agenda-event-past");
            } else {
                self.root.add_css_class("agenda-event-past");
            }
        }
    }

}
