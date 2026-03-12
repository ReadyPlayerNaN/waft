//! Dumb widget for a single user timer row.
//!
//! Renders timer name, schedule summary, enable/disable switch, run-now button,
//! edit button, and delete button as an `adw::ActionRow` with suffix widgets.

use waft_protocol::entity::session::ScheduleKind;
use waft_ui_gtk::vdom::primitives::{VActionRow, VBox, VButton, VSwitch};
use waft_ui_gtk::vdom::{RenderCallback, RenderFn, VNode};

use crate::i18n::t;

/// Input data for constructing or updating a timer row.
#[derive(Clone, PartialEq)]
pub struct TimerRowProps {
    pub name: String,
    pub description: String,
    pub schedule: ScheduleKind,
    pub enabled: bool,
    pub active: bool,
}

/// Output events from a timer row.
#[derive(Debug, Clone)]
pub enum TimerRowOutput {
    Enable,
    Disable,
    RunNow,
    Edit,
    Delete,
}

/// Produce a short schedule summary string.
fn schedule_summary(schedule: &ScheduleKind) -> String {
    match schedule {
        ScheduleKind::Calendar { spec, .. } => {
            let lower = spec.to_lowercase();
            if lower == "daily" || lower == "*-*-* 00:00:00" {
                "Daily".to_string()
            } else if lower == "hourly" {
                "Hourly".to_string()
            } else if lower == "weekly" {
                "Weekly".to_string()
            } else {
                spec.clone()
            }
        }
        ScheduleKind::Relative {
            on_boot_sec,
            on_unit_active_sec,
            ..
        } => {
            if let Some(repeat) = on_unit_active_sec {
                format!("Every {}s", repeat)
            } else if let Some(boot) = on_boot_sec {
                format!("{}s after boot", boot)
            } else {
                t("scheduler-relative")
            }
        }
    }
}

pub(crate) struct TimerRowRender;

impl RenderFn for TimerRowRender {
    type Props = TimerRowProps;
    type Output = TimerRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<TimerRowOutput>) -> VNode {
        let summary = schedule_summary(&props.schedule);

        let subtitle = if props.description.is_empty() {
            summary
        } else {
            format!("{} — {}", props.description, summary)
        };

        // Enable/disable switch
        let enable_emit = emit.clone();
        let enabled = props.enabled;
        let enable_switch = VSwitch::new(enabled).on_toggle(move |new_state| {
            if let Some(ref cb) = *enable_emit.borrow() {
                if new_state {
                    cb(TimerRowOutput::Enable);
                } else {
                    cb(TimerRowOutput::Disable);
                }
            }
        });

        // Run now button
        let run_emit = emit.clone();
        let run_btn = VButton::new(&t("scheduler-run-now")).on_click(move || {
            if let Some(ref cb) = *run_emit.borrow() {
                cb(TimerRowOutput::RunNow);
            }
        });

        // Edit button
        let edit_emit = emit.clone();
        let edit_btn = VButton::new(&t("scheduler-edit-timer")).on_click(move || {
            if let Some(ref cb) = *edit_emit.borrow() {
                cb(TimerRowOutput::Edit);
            }
        });

        // Delete button
        let delete_emit = emit.clone();
        let delete_btn = VButton::new(&t("scheduler-delete-timer")).on_click(move || {
            if let Some(ref cb) = *delete_emit.borrow() {
                cb(TimerRowOutput::Delete);
            }
        });

        VNode::action_row(
            VActionRow::new(&props.name)
                .subtitle(&subtitle)
                .suffix(VNode::vbox(
                    VBox::horizontal(4)
                        .valign(gtk::Align::Center)
                        .child(VNode::switch(enable_switch))
                        .child(VNode::button(run_btn))
                        .child(VNode::button(edit_btn))
                        .child(VNode::button(delete_btn)),
                )),
        )
    }
}

pub type TimerRow = waft_ui_gtk::vdom::RenderComponent<TimerRowRender>;
