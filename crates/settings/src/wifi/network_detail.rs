//! Detail sub-page for a single known WiFi network.
//!
//! Stateful GTK4 widget showing connection settings (autoconnect, metered,
//! DNS, IP method) and a destructive Forget button with confirmation dialog.
//! Created once per known network and pushed onto the NavigationView on demand.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use waft_protocol::entity::network::{IpMethod, MeteredState, WiFiNetwork};

use crate::i18n::t;

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(NetworkDetailOutput)>>>>;

/// Props for the network detail page.
#[derive(Clone, PartialEq)]
pub struct NetworkDetailProps {
    pub ssid: String,
    pub connected: bool,
    pub autoconnect: Option<bool>,
    pub metered: Option<MeteredState>,
    pub dns_servers: Option<Vec<String>>,
    pub ip_method: Option<IpMethod>,
}

impl From<&WiFiNetwork> for NetworkDetailProps {
    fn from(n: &WiFiNetwork) -> Self {
        Self {
            ssid: n.ssid.clone(),
            connected: n.connected,
            autoconnect: n.autoconnect,
            metered: n.metered,
            dns_servers: n.dns_servers.clone(),
            ip_method: n.ip_method,
        }
    }
}

/// Output events from the network detail page.
#[derive(Debug, Clone)]
pub enum NetworkDetailOutput {
    Forget,
    Share,
    UpdateSettings { settings: serde_json::Value },
}

/// Stateful detail page for a single known network.
pub struct NetworkDetailPage {
    pub root: gtk::Box,
    autoconnect_row: adw::SwitchRow,
    autoconnect_handler: glib::SignalHandlerId,
    metered_row: adw::ComboRow,
    metered_handler: glib::SignalHandlerId,
    dns_entry: gtk::Entry,
    ip_method_row: adw::ComboRow,
    ip_method_handler: glib::SignalHandlerId,
    output_cb: OutputCallback,
}

const METERED_OPTIONS: &[MeteredState] = &[
    MeteredState::Unknown,
    MeteredState::No,
    MeteredState::Yes,
    MeteredState::GuessNo,
    MeteredState::GuessYes,
];

fn metered_labels() -> Vec<String> {
    vec![
        t("wifi-metered-unknown"),
        t("wifi-metered-no"),
        t("wifi-metered-yes"),
        t("wifi-metered-guess-no"),
        t("wifi-metered-guess-yes"),
    ]
}

fn metered_to_index(state: MeteredState) -> u32 {
    METERED_OPTIONS
        .iter()
        .position(|&s| s == state)
        .unwrap_or(0) as u32
}

const IP_METHOD_OPTIONS: &[IpMethod] = &[
    IpMethod::Auto,
    IpMethod::Manual,
    IpMethod::LinkLocal,
    IpMethod::Disabled,
];

fn ip_method_labels() -> Vec<String> {
    vec![
        t("wifi-ip-auto"),
        t("wifi-ip-manual"),
        t("wifi-ip-link-local"),
        t("wifi-ip-disabled"),
    ]
}

fn ip_method_to_index(method: IpMethod) -> u32 {
    IP_METHOD_OPTIONS
        .iter()
        .position(|&m| m == method)
        .unwrap_or(0) as u32
}

impl NetworkDetailPage {
    pub fn new(props: &NetworkDetailProps) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        // --- Connection Settings group ---
        let settings_group = adw::PreferencesGroup::builder()
            .title(t("wifi-detail-settings"))
            .build();
        root.append(&settings_group);

        // Autoconnect toggle
        let autoconnect_row = adw::SwitchRow::builder()
            .title(t("wifi-detail-autoconnect"))
            .active(props.autoconnect.unwrap_or(true))
            .build();
        settings_group.add(&autoconnect_row);

        let cb_ref = output_cb.clone();
        let autoconnect_handler = autoconnect_row.connect_active_notify(move |row| {
            if let Some(ref cb) = *cb_ref.borrow() {
                cb(NetworkDetailOutput::UpdateSettings {
                    settings: serde_json::json!({ "autoconnect": row.is_active() }),
                });
            }
        });

        // Metered dropdown
        let metered_labels = metered_labels();
        let metered_str_refs: Vec<&str> = metered_labels.iter().map(std::string::String::as_str).collect();
        let metered_row = adw::ComboRow::builder()
            .title(t("wifi-detail-metered"))
            .model(&gtk::StringList::new(&metered_str_refs))
            .selected(metered_to_index(
                props.metered.unwrap_or(MeteredState::Unknown),
            ))
            .build();
        settings_group.add(&metered_row);

        let cb_ref = output_cb.clone();
        let metered_handler = metered_row.connect_selected_notify(move |row| {
            if let Some(&state) = METERED_OPTIONS.get(row.selected() as usize)
                && let Some(ref cb) = *cb_ref.borrow()
            {
                cb(NetworkDetailOutput::UpdateSettings {
                    settings: serde_json::json!({ "metered": state }),
                });
            }
        });

        // --- IP Configuration group ---
        let ip_group = adw::PreferencesGroup::builder()
            .title(t("wifi-detail-ip-config"))
            .build();
        root.append(&ip_group);

        // IP Method dropdown
        let ip_labels = ip_method_labels();
        let ip_str_refs: Vec<&str> = ip_labels.iter().map(std::string::String::as_str).collect();
        let ip_method_row = adw::ComboRow::builder()
            .title(t("wifi-detail-ip-method"))
            .model(&gtk::StringList::new(&ip_str_refs))
            .selected(ip_method_to_index(
                props.ip_method.unwrap_or(IpMethod::Auto),
            ))
            .build();
        ip_group.add(&ip_method_row);

        let cb_ref = output_cb.clone();
        let ip_method_handler = ip_method_row.connect_selected_notify(move |row| {
            if let Some(&method) = IP_METHOD_OPTIONS.get(row.selected() as usize)
                && let Some(ref cb) = *cb_ref.borrow()
            {
                cb(NetworkDetailOutput::UpdateSettings {
                    settings: serde_json::json!({ "ip_method": method }),
                });
            }
        });

        // DNS servers entry
        let dns_text = props
            .dns_servers
            .as_ref()
            .map(|servers| servers.join(", "))
            .unwrap_or_default();
        let dns_entry = gtk::Entry::builder()
            .text(&dns_text)
            .placeholder_text(t("wifi-detail-dns-placeholder"))
            .build();

        let dns_row = adw::ActionRow::builder()
            .title(t("wifi-detail-dns"))
            .build();
        dns_row.add_suffix(&dns_entry);
        ip_group.add(&dns_row);

        // Commit DNS on focus-out or activate
        {
            let cb_ref = output_cb.clone();
            let entry = dns_entry.clone();
            dns_entry.connect_activate(move |_| {
                emit_dns_update(&cb_ref, &entry);
            });
        }
        {
            let cb_ref = output_cb.clone();
            let entry = dns_entry.clone();
            let focus_controller = gtk::EventControllerFocus::new();
            focus_controller.connect_leave(move |_| {
                emit_dns_update(&cb_ref, &entry);
            });
            dns_entry.add_controller(focus_controller);
        }

        // --- Actions group ---
        let actions_group = adw::PreferencesGroup::builder().margin_top(24).build();

        let share_button = gtk::Button::builder()
            .label(t("wifi-share"))
            .css_classes(["pill"])
            .halign(gtk::Align::Start)
            .build();
        {
            let cb_ref = output_cb.clone();
            share_button.connect_clicked(move |_| {
                if let Some(ref cb) = *cb_ref.borrow() {
                    cb(NetworkDetailOutput::Share);
                }
            });
        }
        actions_group.add(&share_button);

        let forget_button = gtk::Button::builder()
            .label(t("wifi-detail-forget"))
            .css_classes(["destructive-action", "pill"])
            .halign(gtk::Align::Start)
            .build();
        {
            let cb_ref = output_cb.clone();
            forget_button.connect_clicked(move |_| {
                if let Some(ref cb) = *cb_ref.borrow() {
                    cb(NetworkDetailOutput::Forget);
                }
            });
        }
        actions_group.add(&forget_button);
        root.append(&actions_group);

        Self {
            root,
            autoconnect_row,
            autoconnect_handler,
            metered_row,
            metered_handler,
            dns_entry,
            ip_method_row,
            ip_method_handler,
            output_cb,
        }
    }

    pub fn connect_output(&self, cb: impl Fn(NetworkDetailOutput) + 'static) {
        *self.output_cb.borrow_mut() = Some(Box::new(cb));
    }

    pub fn update(&self, props: &NetworkDetailProps) {
        // Block signals during update to prevent spurious actions
        self.autoconnect_row.block_signal(&self.autoconnect_handler);
        self.autoconnect_row
            .set_active(props.autoconnect.unwrap_or(true));
        self.autoconnect_row
            .unblock_signal(&self.autoconnect_handler);

        self.metered_row.block_signal(&self.metered_handler);
        self.metered_row.set_selected(metered_to_index(
            props.metered.unwrap_or(MeteredState::Unknown),
        ));
        self.metered_row.unblock_signal(&self.metered_handler);

        self.ip_method_row.block_signal(&self.ip_method_handler);
        self.ip_method_row.set_selected(ip_method_to_index(
            props.ip_method.unwrap_or(IpMethod::Auto),
        ));
        self.ip_method_row.unblock_signal(&self.ip_method_handler);

        let dns_text = props
            .dns_servers
            .as_ref()
            .map(|servers| servers.join(", "))
            .unwrap_or_default();
        self.dns_entry.set_text(&dns_text);
    }
}

fn emit_dns_update(cb_ref: &OutputCallback, entry: &gtk::Entry) {
    let text = entry.text().to_string();
    let servers: Vec<String> = text
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if let Some(ref cb) = *cb_ref.borrow() {
        cb(NetworkDetailOutput::UpdateSettings {
            settings: serde_json::json!({ "dns_servers": servers }),
        });
    }
}
