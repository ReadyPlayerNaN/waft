use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_core::Callback;
use waft_ui_gtk::icons::{Icon, IconWidget};

pub struct FeatureToggleMenuButtonProps {
    pub disabled: bool,
    pub name: String,
    pub working: bool,
}

pub enum FeatureToggleMenuButtonOutput {
    Click,
}

pub struct FeatureToggleMenuButton {
    name_label: gtk::Label,
    on_output: Callback<FeatureToggleMenuButtonOutput>,
    primary_icon: IconWidget,
    right_box: gtk::Box,
    root: gtk::Button,
    secondary_icon: IconWidget,
    spinner: gtk::Spinner,
}

fn create_item_box() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .valign(gtk::Align::Center)
        .build()
}

fn update_icon(target: &IconWidget, icon_hints: Vec<Icon>) {
    if icon_hints.is_empty() {
        target.widget().set_visible(false);
    } else {
        target.update_icon(icon_hints);
        target.widget().set_visible(true);
    }
}

impl FeatureToggleMenuButton {
    pub fn new(props: FeatureToggleMenuButtonProps) -> Self {
        let inner = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let icon_box = create_item_box();
        let right_box = create_item_box();
        let name_label = gtk::Label::builder()
            .label(&props.name)
            .hexpand(true)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();

        let spinner = gtk::Spinner::builder()
            .visible(props.working)
            .spinning(props.working)
            .build();

        let primary_icon = IconWidget::new(vec![], 16);
        let secondary_icon = IconWidget::new(vec![], 16);

        primary_icon.widget().set_visible(false);
        secondary_icon.widget().set_visible(false);
        icon_box.append(primary_icon.widget());
        icon_box.append(secondary_icon.widget());

        inner.append(&icon_box);
        inner.append(&name_label);
        inner.append(&right_box);

        let root = gtk::Button::builder()
            .child(&inner)
            .css_classes(["flat", "device-row"])
            .sensitive(!props.disabled)
            .build();

        let on_output: Callback<FeatureToggleMenuButtonOutput> = Rc::new(RefCell::new(None));
        let on_output_ref = on_output.clone();
        root.connect_clicked(move |_| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                callback(FeatureToggleMenuButtonOutput::Click);
            }
        });

        Self {
            name_label,
            on_output,
            primary_icon,
            root,
            secondary_icon,
            spinner,
            right_box,
        }
    }

    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(FeatureToggleMenuButtonOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    pub fn get_right_box(&self) -> &gtk::Box {
        &self.right_box
    }

    pub fn set_primary_icon(&self, icon_hints: Vec<Icon>) {
        update_icon(&self.primary_icon, icon_hints);
    }

    pub fn set_name(&self, name: &str) {
        self.name_label.set_label(name);
    }

    pub fn set_secondary_icon(&self, icon_hints: Vec<Icon>) {
        update_icon(&self.secondary_icon, icon_hints);
    }

    pub fn set_visible(&self, visible: bool) {
        self.root.set_visible(visible);
    }

    pub fn set_working(&self, working: bool) {
        self.spinner.set_visible(working);
        self.spinner.set_spinning(working);
        self.root.set_sensitive(!working);
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
