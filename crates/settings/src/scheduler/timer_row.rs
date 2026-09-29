//! XML-backed widget for a scheduled user timer.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::entity::session::ScheduleKind;

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

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(TimerRowOutput)>>>>;

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
                format!("Every {repeat}s")
            } else if let Some(boot) = on_boot_sec {
                format!("{boot}s after boot")
            } else {
                t("scheduler-relative")
            }
        }
    }
}

/// A timer row whose layout is defined in GTK XML.
pub struct TimerRow {
    pub root: adw::ActionRow,
    enable_switch: gtk::Switch,
    running: Rc<Cell<bool>>,
    updating: Rc<Cell<bool>>,
    output_cb: OutputCallback,
}

impl TimerRow {
    pub fn build(props: &TimerRowProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/timer-row.ui");
        let root: adw::ActionRow = builder
            .object("root")
            .expect("timer-row.ui must contain root");
        let enable_switch: gtk::Switch = builder
            .object("enable_switch")
            .expect("timer-row.ui must contain enable_switch");
        let run_button: gtk::Button = builder
            .object("run_button")
            .expect("timer-row.ui must contain run_button");
        let edit_button: gtk::Button = builder
            .object("edit_button")
            .expect("timer-row.ui must contain edit_button");
        let delete_button: gtk::Button = builder
            .object("delete_button")
            .expect("timer-row.ui must contain delete_button");

        run_button.set_label(&t("scheduler-run-now"));
        edit_button.set_label(&t("scheduler-edit-timer"));
        delete_button.set_label(&t("scheduler-delete-timer"));

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let running = Rc::new(Cell::new(false));
        let updating = Rc::new(Cell::new(false));
        {
            let output_cb = output_cb.clone();
            let updating = updating.clone();
            enable_switch.connect_active_notify(move |switch| {
                if updating.get() {
                    return;
                }
                let output = if switch.is_active() {
                    TimerRowOutput::Enable
                } else {
                    TimerRowOutput::Disable
                };
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(output);
                }
            });
        }
        {
            let output_cb = output_cb.clone();
            run_button.connect_clicked(move |_| {
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(TimerRowOutput::RunNow);
                }
            });
        }
        {
            let output_cb = output_cb.clone();
            edit_button.connect_clicked(move |_| {
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(TimerRowOutput::Edit);
                }
            });
        }
        {
            let output_cb = output_cb.clone();
            delete_button.connect_clicked(move |_| {
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(TimerRowOutput::Delete);
                }
            });
        }

        let row = Self {
            root,
            enable_switch,
            running,
            updating,
            output_cb,
        };
        row.update(props);
        row
    }

    pub fn update(&self, props: &TimerRowProps) {
        let summary = schedule_summary(&props.schedule);
        let subtitle = if props.description.is_empty() {
            summary
        } else {
            format!("{} — {}", props.description, summary)
        };
        self.root.set_title(&props.name);
        self.root.set_subtitle(&subtitle);
        self.running.set(props.active);
        self.updating.set(true);
        self.enable_switch.set_active(props.enabled);
        self.updating.set(false);
    }

    pub fn connect_output<F: Fn(TimerRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
