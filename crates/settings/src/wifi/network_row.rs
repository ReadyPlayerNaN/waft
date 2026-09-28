//! XML-backed WiFi network row.
//!
//! The row's stable widget hierarchy lives in `ui/wifi-network-row.ui`.
//! Runtime values, icons, navigation, and action callbacks remain in Rust.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::subclass::prelude::*;
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::t;

/// Props for creating or updating a network row.
#[derive(Clone)]
pub struct NetworkRowProps {
    pub ssid: String,
    pub strength: u8,
    pub secure: bool,
    pub connected: bool,
    pub connecting: bool,
    pub on_navigate: Option<Rc<dyn Fn()>>,
}

/// Output events from a network row.
pub enum NetworkRowOutput {
    Connect,
    Disconnect,
}

fn signal_icon_name(strength: u8) -> &'static str {
    if strength > 75 {
        "network-wireless-signal-excellent-symbolic"
    } else if strength > 50 {
        "network-wireless-signal-good-symbolic"
    } else if strength > 25 {
        "network-wireless-signal-ok-symbolic"
    } else {
        "network-wireless-signal-weak-symbolic"
    }
}

type OutputCallback = Box<dyn Fn(NetworkRowOutput)>;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/waft/settings/wifi-network-row.ui")]
    pub struct NetworkRow {
        #[template_child]
        pub icon_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub action_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub action_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub navigate_box: TemplateChild<gtk::Box>,
        pub(crate) output_cb: RefCell<Option<OutputCallback>>,
        pub(crate) navigate_cb: RefCell<Option<Rc<dyn Fn()>>>,
        pub(crate) connected: Cell<bool>,
        pub(crate) signal_icon: RefCell<Option<IconWidget>>,
        pub(crate) security_icon: RefCell<Option<IconWidget>>,
        pub(crate) navigate_icon: RefCell<Option<IconWidget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NetworkRow {
        const NAME: &'static str = "WaftNetworkRow";
        type Type = super::NetworkRow;
        type ParentType = adw::ActionRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for NetworkRow {}
    impl WidgetImpl for NetworkRow {}
    impl gtk::subclass::prelude::ListBoxRowImpl for NetworkRow {}
    impl adw::subclass::prelude::PreferencesRowImpl for NetworkRow {}
    impl adw::subclass::prelude::ActionRowImpl for NetworkRow {}
}

glib::wrapper! {
    /// A single WiFi network rendered from a GTK XML template.
    pub struct NetworkRow(ObjectSubclass<imp::NetworkRow>)
        @extends adw::ActionRow, adw::PreferencesRow, gtk::ListBoxRow, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Actionable;
}

impl NetworkRow {
    /// Construct a row and initialize its stable icon children and callbacks.
    pub fn build(props: &NetworkRowProps) -> Self {
        let row: Self = glib::Object::new();
        let imp = row.imp();

        let signal_icon = IconWidget::from_name(signal_icon_name(props.strength), 16);
        let security_icon = IconWidget::from_name("network-wireless-encrypted-symbolic", 16);
        let navigate_icon = IconWidget::from_name("go-next-symbolic", 16);
        imp.icon_box.append(signal_icon.widget());
        imp.icon_box.append(security_icon.widget());
        imp.navigate_box.append(navigate_icon.widget());
        *imp.signal_icon.borrow_mut() = Some(signal_icon);
        *imp.security_icon.borrow_mut() = Some(security_icon);
        *imp.navigate_icon.borrow_mut() = Some(navigate_icon);

        let weak_row = row.downgrade();
        imp.action_button.connect_clicked(move |_| {
            if let Some(row) = weak_row.upgrade() {
                row.emit_action();
            }
        });

        let weak_row = row.downgrade();
        row.connect_activated(move |_| {
            if let Some(row) = weak_row.upgrade()
                && let Some(callback) = row.imp().navigate_cb.borrow().as_ref()
            {
                callback();
            }
        });

        row.update(props);
        row
    }

    /// Update runtime values without replacing the GTK widget.
    pub fn update(&self, props: &NetworkRowProps) {
        let imp = self.imp();
        self.set_title(&props.ssid);
        let subtitle = if props.connecting {
            Some(t("wifi-connecting"))
        } else if props.connected {
            Some(t("wifi-connected"))
        } else {
            None
        };
        self.set_subtitle(subtitle.as_deref().unwrap_or_default());

        let action_label = if props.connected {
            t("wifi-disconnect")
        } else {
            t("wifi-connect")
        };
        imp.action_label.set_label(&action_label);
        imp.action_button.set_sensitive(!props.connecting);
        imp.connected.set(props.connected);
        imp.security_icon
            .borrow()
            .as_ref()
            .expect("security icon initialized before update")
            .widget()
            .set_visible(props.secure);
        imp.navigate_box.set_visible(props.on_navigate.is_some());
        self.set_activatable(props.on_navigate.is_some());
        *imp.navigate_cb.borrow_mut() = props.on_navigate.clone();
        imp.signal_icon
            .borrow()
            .as_ref()
            .expect("signal icon initialized before update")
            .set_icon(signal_icon_name(props.strength));
    }

    /// Return the row as a regular GTK widget for container APIs.
    pub fn widget(&self) -> gtk::Widget {
        self.clone().upcast()
    }

    /// Register the callback emitted by the connect/disconnect button.
    pub fn connect_output<F: Fn(NetworkRowOutput) + 'static>(&self, callback: F) {
        *self.imp().output_cb.borrow_mut() = Some(Box::new(callback));
    }

    fn emit_action(&self) {
        let output = if self.imp().connected.get() {
            NetworkRowOutput::Disconnect
        } else {
            NetworkRowOutput::Connect
        };
        if let Some(callback) = self.imp().output_cb.borrow().as_ref() {
            callback(output);
        }
    }
}
