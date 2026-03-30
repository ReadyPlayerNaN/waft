//! Dialog for adding or editing a user timer.
//!
//! Two sections: Schedule (name, description, calendar/relative toggle, schedule fields)
//! and Service (command, working directory, cpu quota, memory limit, env, after, restart).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::entity::session::{RestartPolicy, ScheduleKind, UserTimer};

use crate::i18n::t;
use crate::scheduler::schedule_picker::SchedulePicker;

type ConfirmedCallback = Rc<RefCell<Option<Box<dyn Fn(UserTimer)>>>>;

/// Dialog for creating or editing a user timer.
pub struct TimerDialog {
    dialog: adw::Dialog,
    on_confirmed: ConfirmedCallback,
}

impl TimerDialog {
    /// Create a new timer dialog.
    ///
    /// If `initial` is provided, the fields are pre-populated for editing.
    pub fn new(initial: Option<&UserTimer>) -> Self {
        let heading = if initial.is_some() {
            t("scheduler-edit-timer")
        } else {
            t("scheduler-add-timer")
        };

        let dialog = adw::Dialog::builder()
            .title(&heading)
            .content_width(700)
            .content_height(600)
            .build();

        // Header bar with Cancel and Save buttons
        let cancel_btn = gtk::Button::builder()
            .label(t("notif-cancel"))
            .build();
        let save_btn = gtk::Button::builder()
            .label(t("startup-save"))
            .css_classes(["suggested-action"])
            .build();

        let header = adw::HeaderBar::builder().build();
        header.pack_start(&cancel_btn);
        header.pack_end(&save_btn);

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        // -- Schedule section --
        let schedule_group = adw::PreferencesGroup::builder()
            .title(t("scheduler-tab-schedule"))
            .build();

        let name_row = adw::EntryRow::builder()
            .title(t("scheduler-name"))
            .build();

        let description_row = adw::EntryRow::builder()
            .title(t("scheduler-description"))
            .build();

        // Schedule kind toggle (Calendar vs Relative)
        let calendar_btn = gtk::ToggleButton::builder()
            .label(t("scheduler-calendar"))
            .active(true)
            .build();
        let relative_btn = gtk::ToggleButton::builder()
            .label(t("scheduler-relative"))
            .group(&calendar_btn)
            .build();

        let toggle_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(0)
            .css_classes(["linked"])
            .halign(gtk::Align::Center)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        toggle_box.append(&calendar_btn);
        toggle_box.append(&relative_btn);

        // Calendar fields – structured schedule picker
        let (cal_spec_str, initial_persistent) = initial
            .map(|tmr| match &tmr.schedule {
                ScheduleKind::Calendar { spec, persistent } => {
                    (Some(spec.clone()), *persistent)
                }
                _ => (None, false),
            })
            .unwrap_or((None, false));
        let picker = Rc::new(SchedulePicker::new(cal_spec_str.as_deref(), initial_persistent));

        let calendar_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();
        calendar_box.append(&picker.group);

        // Relative fields
        let boot_sec_row = adw::EntryRow::builder()
            .title(t("scheduler-on-boot-sec"))
            .build();

        let repeat_sec_row = adw::EntryRow::builder()
            .title(t("scheduler-on-unit-active-sec"))
            .build();

        let relative_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .visible(false)
            .build();

        let relative_fields = adw::PreferencesGroup::new();
        relative_fields.add(&boot_sec_row);
        relative_fields.add(&repeat_sec_row);
        relative_box.append(&relative_fields);

        schedule_group.add(&name_row);
        schedule_group.add(&description_row);
        content.append(&schedule_group);
        content.append(&toggle_box);
        content.append(&calendar_box);
        content.append(&relative_box);

        // Toggle visibility
        {
            let calendar_ref = calendar_box.clone();
            let relative_ref = relative_box.clone();
            calendar_btn.connect_toggled(move |btn| {
                let is_calendar = btn.is_active();
                calendar_ref.set_visible(is_calendar);
                relative_ref.set_visible(!is_calendar);
            });
        }

        // -- Service section --
        let service_group = adw::PreferencesGroup::builder()
            .title(t("scheduler-tab-service"))
            .build();

        let command_row = adw::EntryRow::builder()
            .title(t("scheduler-command"))
            .build();

        let workdir_row = adw::EntryRow::builder()
            .title(t("scheduler-working-directory"))
            .build();

        let cpu_quota_row = adw::EntryRow::builder()
            .title(t("scheduler-cpu-quota"))
            .build();

        let memory_limit_row = adw::EntryRow::builder()
            .title(t("scheduler-memory-limit"))
            .build();

        let env_row = adw::EntryRow::builder()
            .title(t("scheduler-environment"))
            .build();

        let after_row = adw::EntryRow::builder()
            .title(t("scheduler-after"))
            .build();

        // Restart policy dropdown
        let restart_row = adw::ComboRow::builder()
            .title(t("scheduler-restart"))
            .build();
        let restart_model = gtk::StringList::new(&[
            &t("scheduler-restart-no"),
            &t("scheduler-restart-on-failure"),
            &t("scheduler-restart-always"),
        ]);
        restart_row.set_model(Some(&restart_model));

        service_group.add(&command_row);
        service_group.add(&workdir_row);
        service_group.add(&env_row);
        service_group.add(&after_row);
        service_group.add(&restart_row);
        service_group.add(&cpu_quota_row);
        service_group.add(&memory_limit_row);
        content.append(&service_group);

        // Pre-populate if editing
        if let Some(timer) = initial {
            name_row.set_text(&timer.name);
            description_row.set_text(&timer.description);
            command_row.set_text(&timer.command);
            if let Some(ref wd) = timer.working_directory {
                workdir_row.set_text(wd);
            }
            if let Some(ref cq) = timer.cpu_quota {
                cpu_quota_row.set_text(cq);
            }
            if let Some(ref ml) = timer.memory_limit {
                memory_limit_row.set_text(ml);
            }

            // Environment as KEY=VALUE lines
            if !timer.environment.is_empty() {
                let env_str: Vec<String> = timer
                    .environment
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect();
                env_row.set_text(&env_str.join(" "));
            }
            if !timer.after.is_empty() {
                after_row.set_text(&timer.after.join(" "));
            }

            match timer.restart {
                RestartPolicy::No => restart_row.set_selected(0),
                RestartPolicy::OnFailure => restart_row.set_selected(1),
                RestartPolicy::Always => restart_row.set_selected(2),
            }

            match &timer.schedule {
                ScheduleKind::Calendar { .. } => {
                    // picker was pre-populated in SchedulePicker::new
                    calendar_btn.set_active(true);
                }
                ScheduleKind::Relative {
                    on_boot_sec,
                    on_unit_active_sec,
                    ..
                } => {
                    relative_btn.set_active(true);
                    if let Some(sec) = on_boot_sec {
                        boot_sec_row.set_text(&sec.to_string());
                    }
                    if let Some(sec) = on_unit_active_sec {
                        repeat_sec_row.set_text(&sec.to_string());
                    }
                }
            }
        }

        // Wrap content in a scrolled window so the dialog doesn't grow unbounded
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&content)
            .build();

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&scrolled));
        dialog.set_child(Some(&toolbar_view));

        let on_confirmed: ConfirmedCallback = Rc::new(RefCell::new(None));

        // Disable save when required fields are empty or any field has invalid format.
        {
            let name_ref = name_row.clone();
            let cmd_ref = command_row.clone();
            let cal_btn_ref = calendar_btn.clone();
            let picker_for_validity = Rc::clone(&picker);
            let boot_ref = boot_sec_row.clone();
            let repeat_ref = repeat_sec_row.clone();
            let workdir_ref = workdir_row.clone();
            let cpu_ref = cpu_quota_row.clone();
            let mem_ref = memory_limit_row.clone();
            let env_ref = env_row.clone();
            let save_ref = save_btn.clone();
            let update_sensitivity: Rc<dyn Fn()> = Rc::new(move || {
                let name_ok = !name_ref.text().trim().is_empty();
                let cmd_ok = !cmd_ref.text().trim().is_empty();

                let is_calendar = cal_btn_ref.is_active();
                let cal_spec_ok = !is_calendar || picker_for_validity.is_valid();

                let boot_text = boot_ref.text();
                let boot_ok =
                    boot_text.trim().is_empty() || boot_text.trim().parse::<u64>().is_ok();
                set_row_error(&boot_ref, !boot_ok);

                let repeat_text = repeat_ref.text();
                let repeat_ok =
                    repeat_text.trim().is_empty() || repeat_text.trim().parse::<u64>().is_ok();
                set_row_error(&repeat_ref, !repeat_ok);

                let workdir_text = workdir_ref.text();
                let workdir_ok =
                    workdir_text.trim().is_empty() || workdir_text.trim().starts_with('/');
                set_row_error(&workdir_ref, !workdir_ok);

                let cpu_text = cpu_ref.text();
                let cpu_ok = cpu_text.trim().is_empty() || is_valid_cpu_quota(cpu_text.trim());
                set_row_error(&cpu_ref, !cpu_ok);

                let mem_text = mem_ref.text();
                let mem_ok =
                    mem_text.trim().is_empty() || is_valid_memory_size(mem_text.trim());
                set_row_error(&mem_ref, !mem_ok);

                let env_text = env_ref.text();
                let env_ok = env_text.trim().is_empty()
                    || env_text
                        .split_whitespace()
                        .all(|t| t.contains('=') && !t.starts_with('='));
                set_row_error(&env_ref, !env_ok);

                let valid = name_ok
                    && cmd_ok
                    && cal_spec_ok
                    && boot_ok
                    && repeat_ok
                    && workdir_ok
                    && cpu_ok
                    && mem_ok
                    && env_ok;
                save_ref.set_sensitive(valid);
            });

            let u = Rc::clone(&update_sensitivity);
            name_row.connect_changed(move |_| u());
            let u = Rc::clone(&update_sensitivity);
            command_row.connect_changed(move |_| u());
            let u = Rc::clone(&update_sensitivity);
            picker.connect_changed(move || u());
            let u = Rc::clone(&update_sensitivity);
            boot_sec_row.connect_changed(move |_| u());
            let u = Rc::clone(&update_sensitivity);
            repeat_sec_row.connect_changed(move |_| u());
            let u = Rc::clone(&update_sensitivity);
            workdir_row.connect_changed(move |_| u());
            let u = Rc::clone(&update_sensitivity);
            cpu_quota_row.connect_changed(move |_| u());
            let u = Rc::clone(&update_sensitivity);
            memory_limit_row.connect_changed(move |_| u());
            let u = Rc::clone(&update_sensitivity);
            env_row.connect_changed(move |_| u());
            let u = Rc::clone(&update_sensitivity);
            calendar_btn.connect_toggled(move |_| u());
            update_sensitivity();
        }

        // Cancel closes the dialog
        {
            let dialog_ref = dialog.clone();
            cancel_btn.connect_clicked(move |_| {
                dialog_ref.close();
            });
        }

        // Save collects fields, fires callback, closes dialog
        {
            let picker_for_save = Rc::clone(&picker);
            let cb = on_confirmed.clone();
            let dialog_ref = dialog.clone();
            save_btn.connect_clicked(move |_| {
                let name = name_row.text().trim().to_string();
                let description = description_row.text().trim().to_string();
                let command = command_row.text().trim().to_string();

                if name.is_empty() || command.is_empty() {
                    return;
                }

                let schedule = if calendar_btn.is_active() {
                    ScheduleKind::Calendar {
                        spec: picker_for_save.spec(),
                        persistent: picker_for_save.persistent_row.is_active(),
                    }
                } else {
                    ScheduleKind::Relative {
                        on_boot_sec: parse_optional_u64(&boot_sec_row.text()),
                        on_startup_sec: None,
                        on_unit_active_sec: parse_optional_u64(&repeat_sec_row.text()),
                    }
                };

                let working_directory = non_empty_string(&workdir_row.text());
                let cpu_quota = non_empty_string(&cpu_quota_row.text());
                let memory_limit = non_empty_string(&memory_limit_row.text());
                let environment = parse_env_vars(&env_row.text());
                let after = parse_space_list(&after_row.text());

                let restart = match restart_row.selected() {
                    1 => RestartPolicy::OnFailure,
                    2 => RestartPolicy::Always,
                    _ => RestartPolicy::No,
                };

                let timer = UserTimer {
                    name,
                    description,
                    enabled: true,
                    active: false,
                    schedule,
                    last_trigger: None,
                    next_elapse: None,
                    last_exit_code: None,
                    command,
                    working_directory,
                    environment,
                    after,
                    restart,
                    cpu_quota,
                    memory_limit,
                };

                if let Some(ref callback) = *cb.borrow() {
                    callback(timer);
                }
                dialog_ref.close();
            });
        }

        Self {
            dialog,
            on_confirmed,
        }
    }

    /// Register a callback for when the user confirms the dialog.
    pub fn connect_confirmed(&self, cb: impl Fn(UserTimer) + 'static) {
        *self.on_confirmed.borrow_mut() = Some(Box::new(cb));
    }

    /// Present the dialog on the given widget.
    pub fn present(&self, parent: &impl IsA<gtk::Widget>) {
        self.dialog.present(Some(parent.upcast_ref()));
    }
}

fn parse_optional_u64(text: &str) -> Option<u64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse().ok()
    }
}

fn non_empty_string(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_env_vars(text: &str) -> Vec<(String, String)> {
    text.split_whitespace()
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.trim().to_string();
            let value = parts.next().unwrap_or("").trim().to_string();
            if key.is_empty() {
                None
            } else {
                Some((key, value))
            }
        })
        .collect()
}

fn parse_space_list(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|s| !s.is_empty())
        .map(std::string::ToString::to_string)
        .collect()
}

fn set_row_error(row: &adw::EntryRow, has_error: bool) {
    if has_error {
        row.add_css_class("error");
    } else {
        row.remove_css_class("error");
    }
}

/// Accept "50%" or an integer number of CPU milliseconds per second (0–1000000).
fn is_valid_cpu_quota(s: &str) -> bool {
    if let Some(pct) = s.strip_suffix('%') {
        pct.parse::<f64>().map(|v| v > 0.0).unwrap_or(false)
    } else {
        s.parse::<u64>().is_ok()
    }
}

/// Accept systemd memory size strings: plain integer, or number followed by
/// K/M/G/T/P (with optional B suffix), or "infinity".
fn is_valid_memory_size(s: &str) -> bool {
    if s.eq_ignore_ascii_case("infinity") {
        return true;
    }
    let suffix_len = s
        .chars()
        .rev()
        .take_while(char::is_ascii_alphabetic)
        .count();
    let (num_part, suffix) = s.split_at(s.len() - suffix_len);
    matches!(
        suffix.to_uppercase().as_str(),
        "" | "K" | "M" | "G" | "T" | "P" | "KB" | "MB" | "GB" | "TB" | "PB"
    ) && num_part.parse::<f64>().map(|v| v >= 0.0).unwrap_or(false)
}
