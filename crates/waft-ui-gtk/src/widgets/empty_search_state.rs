//! EmptySearchStateWidget -- placeholder for zero search results.

use gtk::prelude::*;

use crate::widget_base::WidgetBase;
use crate::icons::IconWidget;

/// Properties for the empty search state.
pub struct EmptySearchStateProps {
    /// The current search query (shown in the message).
    pub query: String,
}

/// Centered icon + message shown when a search returns no results.
///
/// Hidden automatically when `query` is empty.
#[derive(Clone)]
pub struct EmptySearchStateWidget {
    root: gtk::Box,
    message_label: gtk::Label,
}

impl EmptySearchStateWidget {
    pub fn new(props: &EmptySearchStateProps) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .halign(gtk::Align::Center)
            .spacing(12)
            .margin_top(24)
            .margin_bottom(24)
            .css_classes(["empty-search-state"])
            .visible(!props.query.is_empty())
            .build();

        let icon = IconWidget::from_name("edit-find-symbolic", 48);
        let icon_widget = WidgetBase::widget(&icon);
        icon_widget.add_css_class("dim-label");
        root.append(&icon_widget);

        let message = if props.query.is_empty() {
            String::new()
        } else {
            format!("No apps matching \u{2018}{}\u{2019}", props.query)
        };

        let message_label = gtk::Label::builder()
            .label(&message)
            .css_classes(["dim-label"])
            .build();
        root.append(&message_label);

        Self { root, message_label }
    }

    /// Update the displayed query. Pass empty string to hide the widget.
    pub fn set_query(&self, query: &str) {
        if query.is_empty() {
            self.root.set_visible(false);
        } else {
            self.message_label
                .set_label(&format!("No apps matching \u{2018}{query}\u{2019}"));
            self.root.set_visible(true);
        }
    }
}

impl WidgetBase for EmptySearchStateWidget {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
