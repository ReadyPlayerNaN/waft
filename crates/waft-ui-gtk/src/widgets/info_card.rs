//! Pure GTK4 InfoCard widget.
//!
//! A display-only card with icon, title, and optional description.
//! Layout: `[Icon 32x32] [Title (bold) / Description (dim)]`

use gtk::prelude::*;

use crate::reconcile::{ReconcileOutcome, Reconcilable};
use crate::utils::icon::IconWidget;
use waft_ipc::Widget as IpcWidget;

/// Pure GTK4 info card widget — display-only, no actions.
#[derive(Clone)]
pub struct InfoCardWidget {
    root: gtk::Box,
    icon_widget: IconWidget,
    title_label: gtk::Label,
    description_label: gtk::Label,
}

impl InfoCardWidget {
    /// Create a new info card widget.
    pub fn new(icon: &str, title: &str, description: Option<&str>) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let icon_widget = IconWidget::from_name(icon, 32);
        icon_widget.widget().set_height_request(32);

        let labels_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .valign(gtk::Align::Center)
            .build();

        let title_label = gtk::Label::builder()
            .label(title)
            .css_classes(["title-3"])
            .xalign(0.0)
            .build();

        let description_label = gtk::Label::builder()
            .label(description.unwrap_or(""))
            .css_classes(["dim-label"])
            .xalign(0.0)
            .visible(description.is_some())
            .build();

        labels_box.append(&title_label);
        labels_box.append(&description_label);

        root.append(icon_widget.widget());
        root.append(&labels_box);

        Self {
            root,
            icon_widget,
            title_label,
            description_label,
        }
    }

    /// Update the icon.
    pub fn set_icon(&self, icon: &str) {
        self.icon_widget.set_icon(icon);
    }

    /// Update the title text.
    pub fn set_title(&self, title: &str) {
        self.title_label.set_label(title);
    }

    /// Update the description text and visibility.
    pub fn set_description(&self, description: Option<&str>) {
        match description {
            Some(text) => {
                self.description_label.set_label(text);
                self.description_label.set_visible(true);
            }
            None => {
                self.description_label.set_label("");
                self.description_label.set_visible(false);
            }
        }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}

impl Reconcilable for InfoCardWidget {
    fn try_reconcile(&self, old_desc: &IpcWidget, new_desc: &IpcWidget) -> ReconcileOutcome {
        match (old_desc, new_desc) {
            (
                IpcWidget::InfoCard { .. },
                IpcWidget::InfoCard {
                    icon,
                    title,
                    description,
                },
            ) => {
                self.set_icon(icon);
                self.set_title(title);
                self.set_description(description.as_deref());
                ReconcileOutcome::Updated
            }
            _ => ReconcileOutcome::Recreate,
        }
    }
}

/// Render an InfoCard widget from the IPC protocol.
pub(crate) fn render_info_card(
    icon: &str,
    title: &str,
    description: &Option<String>,
) -> gtk::Widget {
    let card = InfoCardWidget::new(icon, title, description.as_deref());
    card.widget()
}
