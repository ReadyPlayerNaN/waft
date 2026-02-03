#![allow(dead_code)] // NetworkManager plugin is under development

use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::wifi_icon::get_wifi_icon;
use crate::ui::menu_item::MenuItemWidget;

#[derive(Debug, Clone)]
pub enum WiFiMenuOutput {
    Connect(String), // SSID
    Disconnect,
    Scan,
}

struct NetworkRow {
    widget: gtk::Widget,
    spinner: gtk::Spinner,
}

impl NetworkRow {
    fn new(
        ssid: &str,
        strength: u8,
        secure: bool,
        is_active: bool,
        is_connecting: bool,
        on_output: Rc<RefCell<Option<Box<dyn Fn(WiFiMenuOutput)>>>>,
    ) -> Self {
        // Build content structure
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["network-row"])
            .build();

        // Signal strength icon
        let signal_icon = get_wifi_icon(Some(strength), true, true);

        let icon_image = gtk::Image::builder()
            .icon_name(signal_icon)
            .pixel_size(20)
            .build();

        // SSID label
        let ssid_label = gtk::Label::builder()
            .label(ssid)
            .hexpand(true)
            .xalign(0.0)
            .build();

        // Security icon
        let security_icon = if secure {
            let lock_icon = gtk::Image::builder()
                .icon_name("channel-secure-symbolic")
                .pixel_size(16)
                .build();
            Some(lock_icon)
        } else {
            None
        };

        // Spinner for connecting state
        let spinner = gtk::Spinner::builder()
            .spinning(is_connecting)
            .visible(is_connecting)
            .build();

        content.append(&icon_image);
        content.append(&ssid_label);
        if let Some(ref lock) = security_icon {
            content.append(lock);
        }
        content.append(&spinner);

        // Wrap with MenuItemWidget if not already active
        let widget = if !is_active {
            let ssid_clone = ssid.to_string();
            let menu_item = MenuItemWidget::new(&content, move || {
                if let Some(ref callback) = *on_output.borrow() {
                    callback(WiFiMenuOutput::Connect(ssid_clone.clone()));
                }
            });
            content.add_css_class("clickable");
            menu_item.widget().clone().upcast()
        } else {
            content.add_css_class("active");
            content.upcast()
        };

        Self { widget, spinner }
    }

    fn set_connecting(&self, connecting: bool) {
        self.spinner.set_spinning(connecting);
        self.spinner.set_visible(connecting);
    }
}

#[derive(Clone)]
pub struct WiFiMenuWidget {
    inner: Rc<WiFiMenuWidgetInner>,
}

struct WiFiMenuWidgetInner {
    root: gtk::Box,
    networks_box: gtk::Box,
    network_rows: RefCell<HashMap<String, NetworkRow>>,
    active_ssid: RefCell<Option<String>>,
    on_output: Rc<RefCell<Option<Box<dyn Fn(WiFiMenuOutput)>>>>,
}

impl WiFiMenuWidget {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        let networks_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        root.append(&networks_box);

        let on_output: Rc<RefCell<Option<Box<dyn Fn(WiFiMenuOutput)>>>> =
            Rc::new(RefCell::new(None));

        Self {
            inner: Rc::new(WiFiMenuWidgetInner {
                root,
                networks_box,
                network_rows: RefCell::new(HashMap::new()),
                active_ssid: RefCell::new(None),
                on_output,
            }),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.inner.root.clone().upcast()
    }

    pub fn set_networks(
        &self,
        networks: Vec<(String, u8, bool)>, // (ssid, strength, secure)
    ) {
        let active_ssid = self.inner.active_ssid.borrow().clone();

        // Clear existing networks
        while let Some(child) = self.inner.networks_box.first_child() {
            self.inner.networks_box.remove(&child);
        }
        self.inner.network_rows.borrow_mut().clear();

        // Sort by signal strength
        let mut networks = networks;
        networks.sort_by(|a, b| b.1.cmp(&a.1));

        // Add network rows
        for (ssid, strength, secure) in networks {
            let is_active = active_ssid.as_ref() == Some(&ssid);
            let row = NetworkRow::new(
                &ssid,
                strength,
                secure,
                is_active,
                false,
                self.inner.on_output.clone(),
            );
            self.inner.networks_box.append(&row.widget);
            self.inner.network_rows.borrow_mut().insert(ssid, row);
        }
    }

    pub fn set_active_ssid(&self, ssid: Option<String>) {
        *self.inner.active_ssid.borrow_mut() = ssid;
    }

    pub fn set_connecting(&self, ssid: &str, connecting: bool) {
        if let Some(row) = self.inner.network_rows.borrow().get(ssid) {
            row.set_connecting(connecting);
        }
    }

    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(WiFiMenuOutput) + 'static,
    {
        *self.inner.on_output.borrow_mut() = Some(Box::new(callback));
    }
}
