use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, Default)]
pub struct ConnectionDetails {
    pub link_speed: Option<String>,
    pub ipv4_address: Option<String>,
    pub ipv6_address: Option<String>,
    pub subnet_mask: Option<String>,
    pub gateway: Option<String>,
}

struct DetailRow {
    root: gtk::Box,
    value_label: gtk::Label,
}

impl DetailRow {
    fn new(label_text: &str) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["detail-row"])
            .build();

        let label = gtk::Label::builder()
            .label(label_text)
            .css_classes(["dim-label", "caption"])
            .xalign(0.0)
            .width_chars(15)
            .build();

        let value_label = gtk::Label::builder()
            .label("")
            .css_classes(["caption"])
            .xalign(0.0)
            .hexpand(true)
            .build();

        root.append(&label);
        root.append(&value_label);

        Self { root, value_label }
    }

    fn set_value(&self, value: &str) {
        self.value_label.set_label(value);
    }

    fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

#[derive(Clone)]
pub struct EthernetMenuWidget {
    inner: Rc<EthernetMenuWidgetInner>,
}

struct EthernetMenuWidgetInner {
    root: gtk::Box,
    details_box: gtk::Box,
    link_speed_row: DetailRow,
    ipv4_row: DetailRow,
    ipv6_row: DetailRow,
    subnet_row: DetailRow,
    gateway_row: DetailRow,
    empty_label: gtk::Label,
    details: RefCell<Option<ConnectionDetails>>,
}

impl EthernetMenuWidget {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        let details_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        use waft_plugin_api::i18n::t;
        let empty_label = gtk::Label::builder()
            .label(t("network-disconnected"))
            .css_classes(["dim-label", "caption"])
            .halign(gtk::Align::Start)
            .build();

        let link_speed_row = DetailRow::new(&t("network-link-speed"));
        let ipv4_row = DetailRow::new(&t("network-ipv4-address"));
        let ipv6_row = DetailRow::new(&t("network-ipv6-address"));
        let subnet_row = DetailRow::new(&t("network-subnet-mask"));
        let gateway_row = DetailRow::new(&t("network-gateway"));

        root.append(&details_box);
        root.append(&empty_label);

        Self {
            inner: Rc::new(EthernetMenuWidgetInner {
                root,
                details_box,
                link_speed_row,
                ipv4_row,
                ipv6_row,
                subnet_row,
                gateway_row,
                empty_label,
                details: RefCell::new(None),
            }),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.inner.root.clone().upcast()
    }

    pub fn set_connection_details(&self, details: Option<ConnectionDetails>) {
        *self.inner.details.borrow_mut() = details.clone();

        // Clear existing detail rows
        while let Some(child) = self.inner.details_box.first_child() {
            self.inner.details_box.remove(&child);
        }

        if let Some(details) = details {
            self.inner.empty_label.set_visible(false);

            // Only show fields that have values
            if let Some(ref speed) = details.link_speed {
                self.inner.link_speed_row.set_value(speed);
                self.inner
                    .details_box
                    .append(self.inner.link_speed_row.widget());
            }

            if let Some(ref ipv4) = details.ipv4_address {
                self.inner.ipv4_row.set_value(ipv4);
                self.inner.details_box.append(self.inner.ipv4_row.widget());
            }

            if let Some(ref ipv6) = details.ipv6_address {
                self.inner.ipv6_row.set_value(ipv6);
                self.inner.details_box.append(self.inner.ipv6_row.widget());
            }

            if let Some(ref mask) = details.subnet_mask {
                self.inner.subnet_row.set_value(mask);
                self.inner
                    .details_box
                    .append(self.inner.subnet_row.widget());
            }

            if let Some(ref gw) = details.gateway {
                self.inner.gateway_row.set_value(gw);
                self.inner
                    .details_box
                    .append(self.inner.gateway_row.widget());
            }

            // If no details are available, show empty message
            if details.link_speed.is_none()
                && details.ipv4_address.is_none()
                && details.ipv6_address.is_none()
                && details.subnet_mask.is_none()
                && details.gateway.is_none()
            {
                self.inner.empty_label.set_visible(true);
            }
        } else {
            // No connection details - show empty state
            self.inner.empty_label.set_visible(true);
        }
    }

    pub fn clear(&self) {
        self.set_connection_details(None);
    }
}
