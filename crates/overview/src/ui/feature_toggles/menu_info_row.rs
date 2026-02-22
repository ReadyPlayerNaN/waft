use gtk::prelude::*;

pub struct FeatureToggleMenuInfoRowProps {
    pub label: String,
    pub value: String,
}

pub struct FeatureToggleMenuInfoRow {
    root: gtk::Box,
    value_widget: gtk::Label,
}

impl FeatureToggleMenuInfoRow {
    pub fn new(props: FeatureToggleMenuInfoRowProps) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["menu-row"])
            .build();

        let label_widget = gtk::Label::builder()
            .label(&props.label)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        root.append(&label_widget);

        let value_widget = gtk::Label::builder()
            .label(&props.value)
            .hexpand(true)
            .xalign(1.0)
            .build();
        root.append(&value_widget);

        Self { root, value_widget }
    }

    pub fn set_value(&self, value: &str) {
        self.value_widget.set_label(value);
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}
