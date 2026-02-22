use gtk::prelude::*;
use waft_ui_gtk::vdom::Component;

#[derive(Clone, PartialEq)]
pub struct FeatureToggleMenuInfoRowProps {
    pub label: String,
    pub value: String,
}

pub struct FeatureToggleMenuInfoRow {
    root: gtk::Box,
    label_widget: gtk::Label,
    value_widget: gtk::Label,
}

fn create_label_box() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .css_classes(["menu-row"])
        .build()
}

impl Component for FeatureToggleMenuInfoRow {
    type Props = FeatureToggleMenuInfoRowProps;
    type Output = ();

    fn build(props: &Self::Props) -> Self {
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

        Self {
            root,
            label_widget,
            value_widget,
        }
    }

    fn update(&self, props: &Self::Props) {
        self.label_widget.set_label(&props.label);
        self.value_widget.set_label(&props.value);
    }

    fn connect_output<F: Fn(()) + 'static>(&self, _callback: F) {}

    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}
