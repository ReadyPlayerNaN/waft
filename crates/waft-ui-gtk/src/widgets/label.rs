//! Label widget

use crate::css::apply_css_classes;
use gtk::prelude::*;

/// GTK4 label widget with text and CSS classes.
pub struct LabelWidget {
    label: gtk::Label,
}

impl LabelWidget {
    pub fn new(text: &str, css_classes: &[String]) -> Self {
        let label = gtk::Label::new(Some(text));
        apply_css_classes(&label, css_classes);
        Self { label }
    }

    pub fn set_text(&self, text: &str) {
        self.label.set_text(text);
    }
}

impl crate::widget_base::WidgetBase for LabelWidget {
    fn widget(&self) -> gtk::Widget {
        self.label.clone().upcast()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::init_gtk_for_tests;

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_label_widget_set_text() {
        init_gtk_for_tests();
        let label_widget = LabelWidget::new("Initial", &[]);
        label_widget.set_text("Updated");

        let label: gtk::Label = label_widget.label;
        assert_eq!(label.text(), "Updated");
    }
}
