//! Structured schedule picker for OnCalendar expressions.
//!
//! Presents a "Frequency" combo row with contextual fields (interval spinner,
//! time-of-day spinbuttons, day-of-week toggles, day-of-month spinner) plus
//! a "Custom" fallback for raw expression entry.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

const FREQ_MINUTELY: u32 = 0;
const FREQ_EVERY_N_MINUTES: u32 = 1;
const FREQ_HOURLY: u32 = 2;
const FREQ_DAILY: u32 = 3;
const FREQ_WEEKLY: u32 = 4;
const FREQ_MONTHLY: u32 = 5;
const FREQ_CUSTOM: u32 = 6;

const DOW_NAMES: &[&str] = &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

type ChangeCallback = Rc<RefCell<Option<Box<dyn Fn()>>>>;

/// Structured schedule picker for systemd OnCalendar expressions.
pub struct SchedulePicker {
    /// The preferences group containing all picker rows — add this to your layout.
    pub group: adw::PreferencesGroup,
    freq_row: adw::ComboRow,
    interval_row: adw::SpinRow,
    time_row: adw::ActionRow,
    hour_spin: gtk::SpinButton,
    min_spin: gtk::SpinButton,
    dow_row: adw::ActionRow,
    dow_buttons: Vec<gtk::ToggleButton>,
    dom_row: adw::SpinRow,
    pub persistent_row: adw::SwitchRow,
    custom_row: adw::EntryRow,
    on_change: ChangeCallback,
}

impl SchedulePicker {
    /// Create a new schedule picker.
    ///
    /// `initial_spec` pre-populates the fields in edit mode; `None` defaults
    /// to "Daily at 09:00". `initial_persistent` sets the persistent switch.
    pub fn new(initial_spec: Option<&str>, initial_persistent: bool) -> Self {
        let group = adw::PreferencesGroup::new();

        // Frequency selector
        let freq_row = adw::ComboRow::builder()
            .title(t("scheduler-frequency"))
            .build();
        let freq_model = gtk::StringList::new(&[
            &t("scheduler-freq-minutely"),
            &t("scheduler-freq-every-n-minutes"),
            &t("scheduler-freq-hourly"),
            &t("scheduler-freq-daily"),
            &t("scheduler-freq-weekly"),
            &t("scheduler-freq-monthly"),
            &t("scheduler-freq-custom"),
        ]);
        freq_row.set_model(Some(&freq_model));

        // "Every N minutes" interval
        let interval_row = adw::SpinRow::with_range(1.0, 59.0, 1.0);
        interval_row.set_title(&t("scheduler-interval-minutes"));
        interval_row.set_value(5.0);
        interval_row.set_visible(false);

        // Time of day (daily / weekly / monthly)
        let time_row = adw::ActionRow::builder()
            .title(t("scheduler-time-of-day"))
            .build();
        let hour_adj = gtk::Adjustment::new(9.0, 0.0, 23.0, 1.0, 1.0, 0.0);
        let hour_spin = gtk::SpinButton::new(Some(&hour_adj), 1.0, 0);
        let colon = gtk::Label::new(Some(":"));
        let min_adj = gtk::Adjustment::new(0.0, 0.0, 59.0, 1.0, 5.0, 0.0);
        let min_spin = gtk::SpinButton::new(Some(&min_adj), 1.0, 0);
        let time_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk::Align::Center)
            .build();
        time_box.append(&hour_spin);
        time_box.append(&colon);
        time_box.append(&min_spin);
        time_row.add_suffix(&time_box);
        time_row.set_visible(false);

        // Day-of-week toggles (weekly)
        let dow_row = adw::ActionRow::builder()
            .title(t("scheduler-days-of-week"))
            .build();
        let dow_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk::Align::Center)
            .build();
        let dow_keys = [
            "scheduler-day-mon",
            "scheduler-day-tue",
            "scheduler-day-wed",
            "scheduler-day-thu",
            "scheduler-day-fri",
            "scheduler-day-sat",
            "scheduler-day-sun",
        ];
        let mut dow_buttons: Vec<gtk::ToggleButton> = Vec::new();
        for key in &dow_keys {
            let btn = gtk::ToggleButton::builder()
                .label(t(key))
                .css_classes(["flat"])
                .build();
            dow_box.append(&btn);
            dow_buttons.push(btn);
        }
        // Default: Mon–Fri active
        for btn in &dow_buttons[..5] {
            btn.set_active(true);
        }
        dow_row.add_suffix(&dow_box);
        dow_row.set_visible(false);

        // Day of month (monthly)
        let dom_row = adw::SpinRow::with_range(1.0, 31.0, 1.0);
        dom_row.set_title(&t("scheduler-day-of-month"));
        dom_row.set_value(1.0);
        dom_row.set_visible(false);

        // Persistent switch
        let persistent_row = adw::SwitchRow::builder()
            .title(t("scheduler-persistent"))
            .active(initial_persistent)
            .build();

        // Custom expression fallback
        let custom_row = adw::EntryRow::builder()
            .title(t("scheduler-custom-expression"))
            .visible(false)
            .build();

        group.add(&freq_row);
        group.add(&interval_row);
        group.add(&time_row);
        group.add(&dow_row);
        group.add(&dom_row);
        group.add(&persistent_row);
        group.add(&custom_row);

        let on_change: ChangeCallback = Rc::new(RefCell::new(None));

        let picker = Self {
            group,
            freq_row,
            interval_row,
            time_row,
            hour_spin,
            min_spin,
            dow_row,
            dow_buttons,
            dom_row,
            persistent_row,
            custom_row,
            on_change,
        };

        if let Some(spec) = initial_spec {
            picker.set_from_spec(spec);
        } else {
            picker.freq_row.set_selected(FREQ_DAILY);
            picker.sync_visibility();
        }

        picker.wire_signals();
        picker
    }

    fn sync_visibility(&self) {
        let freq = self.freq_row.selected();
        self.interval_row.set_visible(freq == FREQ_EVERY_N_MINUTES);
        let show_time = matches!(freq, FREQ_DAILY | FREQ_WEEKLY | FREQ_MONTHLY);
        self.time_row.set_visible(show_time);
        self.dow_row.set_visible(freq == FREQ_WEEKLY);
        self.dom_row.set_visible(freq == FREQ_MONTHLY);
        self.persistent_row.set_visible(freq != FREQ_CUSTOM);
        self.custom_row.set_visible(freq == FREQ_CUSTOM);
    }

    fn wire_signals(&self) {
        // Frequency change → sync visibility + fire on_change
        {
            let interval = self.interval_row.clone();
            let time = self.time_row.clone();
            let dow = self.dow_row.clone();
            let dom = self.dom_row.clone();
            let persistent = self.persistent_row.clone();
            let custom = self.custom_row.clone();
            let cb = Rc::clone(&self.on_change);
            self.freq_row.connect_selected_notify(move |row| {
                let freq = row.selected();
                interval.set_visible(freq == FREQ_EVERY_N_MINUTES);
                let show_time = matches!(freq, FREQ_DAILY | FREQ_WEEKLY | FREQ_MONTHLY);
                time.set_visible(show_time);
                dow.set_visible(freq == FREQ_WEEKLY);
                dom.set_visible(freq == FREQ_MONTHLY);
                persistent.set_visible(freq != FREQ_CUSTOM);
                custom.set_visible(freq == FREQ_CUSTOM);
                if let Some(ref f) = *cb.borrow() {
                    f();
                }
            });
        }

        // Shared notify helper
        let notify: Rc<dyn Fn()> = {
            let cb = Rc::clone(&self.on_change);
            Rc::new(move || {
                if let Some(ref f) = *cb.borrow() {
                    f();
                }
            })
        };

        let n = Rc::clone(&notify);
        self.interval_row.connect_value_notify(move |_| n());
        let n = Rc::clone(&notify);
        self.hour_spin.connect_value_changed(move |_| n());
        let n = Rc::clone(&notify);
        self.min_spin.connect_value_changed(move |_| n());
        let n = Rc::clone(&notify);
        self.dom_row.connect_value_notify(move |_| n());
        let n = Rc::clone(&notify);
        self.custom_row.connect_changed(move |_| n());

        for btn in &self.dow_buttons {
            let n = Rc::clone(&notify);
            btn.connect_toggled(move |_| n());
        }
    }

    /// Register a callback invoked whenever any picker value changes.
    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        *self.on_change.borrow_mut() = Some(Box::new(f));
    }

    /// Generate the OnCalendar expression for the current picker state.
    pub fn spec(&self) -> String {
        let h = self.hour_spin.value() as u32;
        let m = self.min_spin.value() as u32;
        match self.freq_row.selected() {
            FREQ_MINUTELY => "*:*:00".to_string(),
            FREQ_EVERY_N_MINUTES => {
                let n = self.interval_row.value() as u32;
                format!("*:0/{}:00", n)
            }
            FREQ_HOURLY => "*:00:00".to_string(),
            FREQ_DAILY => format!("*-*-* {:02}:{:02}:00", h, m),
            FREQ_WEEKLY => {
                let days: Vec<&str> = DOW_NAMES
                    .iter()
                    .zip(&self.dow_buttons)
                    .filter(|(_, btn)| btn.is_active())
                    .map(|(name, _)| *name)
                    .collect();
                let day_str = if days.is_empty() {
                    "Mon".to_string()
                } else {
                    days.join(",")
                };
                format!("{} *-*-* {:02}:{:02}:00", day_str, h, m)
            }
            FREQ_MONTHLY => {
                let d = self.dom_row.value() as u32;
                format!("*-*-{:02} {:02}:{:02}:00", d, h, m)
            }
            _ => self.custom_row.text().trim().to_string(),
        }
    }

    /// Returns true if the current state produces a non-empty valid expression.
    pub fn is_valid(&self) -> bool {
        match self.freq_row.selected() {
            FREQ_CUSTOM => !self.custom_row.text().trim().is_empty(),
            FREQ_WEEKLY => self.dow_buttons.iter().any(|b| b.is_active()),
            _ => true,
        }
    }

    /// Parse a spec string and update the picker UI to match.
    pub fn set_from_spec(&self, spec: &str) {
        match parse_spec(spec.trim()) {
            ParsedSpec::Minutely => {
                self.freq_row.set_selected(FREQ_MINUTELY);
            }
            ParsedSpec::EveryNMinutes(n) => {
                self.freq_row.set_selected(FREQ_EVERY_N_MINUTES);
                self.interval_row.set_value(n as f64);
            }
            ParsedSpec::Hourly => {
                self.freq_row.set_selected(FREQ_HOURLY);
            }
            ParsedSpec::Daily(h, m) => {
                self.freq_row.set_selected(FREQ_DAILY);
                self.hour_spin.set_value(h as f64);
                self.min_spin.set_value(m as f64);
            }
            ParsedSpec::Weekly(days, h, m) => {
                self.freq_row.set_selected(FREQ_WEEKLY);
                for (btn, active) in self.dow_buttons.iter().zip(days.iter()) {
                    btn.set_active(*active);
                }
                self.hour_spin.set_value(h as f64);
                self.min_spin.set_value(m as f64);
            }
            ParsedSpec::Monthly(d, h, m) => {
                self.freq_row.set_selected(FREQ_MONTHLY);
                self.dom_row.set_value(d as f64);
                self.hour_spin.set_value(h as f64);
                self.min_spin.set_value(m as f64);
            }
            ParsedSpec::Custom(s) => {
                self.freq_row.set_selected(FREQ_CUSTOM);
                self.custom_row.set_text(&s);
            }
        }
        self.sync_visibility();
    }
}

// ── Parser ─────────────────────────────────────────────────────────────────

enum ParsedSpec {
    Minutely,
    EveryNMinutes(u32),
    Hourly,
    Daily(u32, u32),
    Weekly([bool; 7], u32, u32),
    Monthly(u32, u32, u32),
    Custom(String),
}

fn parse_spec(spec: &str) -> ParsedSpec {
    // Every minute
    if spec == "*:*:00" || spec == "*:*" {
        return ParsedSpec::Minutely;
    }
    // Hourly
    if spec == "*:00:00" || spec == "*:00" || spec.eq_ignore_ascii_case("hourly") {
        return ParsedSpec::Hourly;
    }
    // Every N minutes: *:0/N:00
    if let Some(rest) = spec.strip_prefix("*:0/") {
        let n_str = rest.strip_suffix(":00").unwrap_or(rest);
        if let Ok(n) = n_str.parse::<u32>() && (1..=59).contains(&n) {
            return ParsedSpec::EveryNMinutes(n);
        }
    }
    // Weekly: leading DOW token(s) before "*-*-* HH:MM:SS"
    let dow_abbrevs = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    if let Some(first) = spec.split_whitespace().next()
        && !first.is_empty()
        && first
            .split(',')
            .all(|d| dow_abbrevs.iter().any(|&p| d.eq_ignore_ascii_case(p)))
    {
        let rest = spec[first.len()..].trim();
        if let Some((h, m)) = parse_daily_time(rest) {
            let mut days = [false; 7];
            for token in first.split(',') {
                for (i, &name) in dow_abbrevs.iter().enumerate() {
                    if token.eq_ignore_ascii_case(name) {
                        days[i] = true;
                    }
                }
            }
            return ParsedSpec::Weekly(days, h, m);
        }
    }
    // Daily or Monthly: *-*-* HH:MM or *-*-DD HH:MM
    if let Some((day_field, time_str)) = split_date_time(spec)
        && let Some((h, m)) = parse_hhmm(time_str)
    {
        if day_field == "*" {
            return ParsedSpec::Daily(h, m);
        }
        if let Ok(d) = day_field.parse::<u32>() && (1..=31).contains(&d) {
            return ParsedSpec::Monthly(d, h, m);
        }
    }
    ParsedSpec::Custom(spec.to_string())
}

/// Split "*-*-DAY TIME" → (day_field, time_str).
fn split_date_time(spec: &str) -> Option<(&str, &str)> {
    let (date, time) = spec.split_once(' ')?;
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() == 3 && parts[0] == "*" && parts[1] == "*" {
        Some((parts[2], time.trim()))
    } else {
        None
    }
}

/// Parse "*-*-* HH:MM[:SS]" (daily pattern) → (h, m).
fn parse_daily_time(spec: &str) -> Option<(u32, u32)> {
    let (day, time) = split_date_time(spec)?;
    if day != "*" {
        return None;
    }
    parse_hhmm(time)
}

/// Parse "HH:MM[:SS]" → (h, m).
fn parse_hhmm(time: &str) -> Option<(u32, u32)> {
    let mut parts = time.split(':');
    let h: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    if h <= 23 && m <= 59 { Some((h, m)) } else { None }
}
