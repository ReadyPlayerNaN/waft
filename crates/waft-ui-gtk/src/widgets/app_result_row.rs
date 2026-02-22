//! AppResultRowWidget -- dumb row for a single app search result.

use gtk::prelude::*;

use crate::widget_base::WidgetBase;
use crate::icons::IconWidget;

/// Properties for an app result row.
pub struct AppResultRowProps {
    pub name: String,
    pub icon: String,
    pub description: Option<String>,
}

/// Horizontal row: 48px icon + vertical label stack (name + optional description).
///
/// No Output enum -- selection and activation are handled at the list level.
pub struct AppResultRowWidget {
    root: gtk::Box,
}

impl AppResultRowWidget {
    pub fn new(props: AppResultRowProps) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .css_classes(["app-result-row"])
            .build();

        let icon = IconWidget::from_name(&props.icon, 48);
        root.append(&WidgetBase::widget(&icon));

        let label_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .spacing(2)
            .build();

        let name_label = gtk::Label::builder()
            .label(&props.name)
            .halign(gtk::Align::Start)
            .css_classes(["app-result-name"])
            .build();
        label_box.append(&name_label);

        if let Some(desc) = &props.description {
            let desc_label = gtk::Label::builder()
                .label(desc.as_str())
                .halign(gtk::Align::Start)
                .css_classes(["app-result-description", "dim-label"])
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            label_box.append(&desc_label);
        }

        root.append(&label_box);
        Self { root }
    }
}

impl WidgetBase for AppResultRowWidget {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
