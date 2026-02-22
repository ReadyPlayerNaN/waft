use gtk::prelude::*;

pub struct FeatureToggleMenuInfoRowProps {
    pub label: String,
    pub value: String,
}

pub struct FeatureToggleMenuInfoRow {
    root: gtk::Box,
}

fn create_label_box() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .css_classes(["menu-row"])
        .build()
}

impl FeatureToggleMenuInfoRow {
    pub fn new(props: FeatureToggleMenuInfoRowProps) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["menu-row"])
            .build();

        let label_box = create_label_box();
        let label_widget = gtk::Label::builder()
            .label(&props.label)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        label_box.append(&label_widget);

        let value_box = create_label_box();
        let value_widget = gtk::Label::builder()
            .label(&props.value)
            .hexpand(true)
            .xalign(1.0)
            .build();
        value_box.append(&value_widget);

        root.append(&label_box);
        root.append(&value_box);

        Self { root }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}
