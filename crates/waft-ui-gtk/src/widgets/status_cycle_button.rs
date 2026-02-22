//! StatusCycleButton widget — displays current option and cycles to next on click.

use std::cell::RefCell;
use std::rc::Rc;

use crate::icons::Icon;
use crate::vdom::{Component, RenderCallback, RenderComponent, RenderFn, VBox, VIcon, VNode, VCustomButton};

/// An option for StatusCycleButton.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusOption {
    pub id: String,
    pub label: String,
}

/// Properties for the status cycle button.
#[derive(Clone, PartialEq, Debug)]
pub struct StatusCycleButtonProps {
    pub value: String,
    pub icon: String,
    pub options: Vec<StatusOption>,
}

pub enum StatusCycleButtonOutput {
    Cycle(String), // Emits next option ID
}

pub struct StatusCycleButtonRender;

impl RenderFn for StatusCycleButtonRender {
    type Props = StatusCycleButtonProps;
    type Output = StatusCycleButtonOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<StatusCycleButtonOutput>) -> VNode {
        let label = Self::find_label(&props.value, &props.options);

        // Create icon and label within a horizontal box
        let icon = VIcon::new(vec![Icon::parse(&props.icon)], 16);
        let mut box_content = VBox::horizontal(8);
        box_content = box_content.child(VNode::icon(icon));

        box_content = box_content.child(
            VNode::label(
                crate::vdom::VLabel::new(label)
            )
        );

        // Create the button with cycling logic
        let options = props.options.clone();
        let value = props.value.clone();
        let emit_clone = emit.clone();

        let button = VCustomButton::new(VNode::vbox(box_content))
            .css_classes(["flat", "status-cycle-button"])
            .sensitive(props.options.len() >= 2)
            .on_click(move || {
                let next_id = Self::next_option_id(&value, &options);
                if let Some(callback) = &*emit_clone.borrow() {
                    callback(StatusCycleButtonOutput::Cycle(next_id));
                }
            });

        VNode::custom_button(button)
    }
}

impl StatusCycleButtonRender {
    /// Find the label for the given value, or "---" if not found.
    fn find_label(value: &str, options: &[StatusOption]) -> String {
        options
            .iter()
            .find(|o| o.id == value)
            .map(|o| o.label.clone())
            .unwrap_or_else(|| "---".to_string())
    }

    /// Get the next option ID after the current value.
    /// If value not found in options, returns the first option's ID.
    fn next_option_id(value: &str, options: &[StatusOption]) -> String {
        if options.is_empty() {
            return String::new();
        }
        let current_idx = options.iter().position(|o| o.id == value);
        match current_idx {
            Some(idx) => {
                let next_idx = (idx + 1) % options.len();
                options[next_idx].id.clone()
            }
            None => options[0].id.clone(),
        }
    }
}

/// Wrapper around RenderComponent<StatusCycleButtonRender> with state tracking.
#[derive(Clone)]
pub struct StatusCycleButtonWidget {
    inner: Rc<RenderComponent<StatusCycleButtonRender>>,
    props: Rc<RefCell<StatusCycleButtonProps>>,
}

impl StatusCycleButtonWidget {
    /// Create a new status cycle button widget.
    pub fn new(value: &str, icon: &str, options: &[StatusOption]) -> Self {
        let props = StatusCycleButtonProps {
            value: value.to_string(),
            icon: icon.to_string(),
            options: options.to_vec(),
        };
        let inner = Rc::new(RenderComponent::<StatusCycleButtonRender>::build(&props));
        Self {
            inner,
            props: Rc::new(RefCell::new(props)),
        }
    }

    /// Connect an output callback to the widget.
    pub fn connect_output<F: Fn(StatusCycleButtonOutput) + 'static>(&self, callback: F) {
        self.inner.connect_output(callback);
    }

    /// Update value.
    pub fn set_value(&self, value: &str) {
        let mut props = self.props.borrow_mut();
        props.value = value.to_string();
        self.inner.update(&*props);
    }

    /// Update the icon.
    pub fn set_icon(&self, icon: &str) {
        let mut props = self.props.borrow_mut();
        props.icon = icon.to_string();
        self.inner.update(&*props);
    }

    /// Update options.
    pub fn set_options(&self, options: &[StatusOption]) {
        let mut props = self.props.borrow_mut();
        props.options = options.to_vec();
        self.inner.update(&*props);
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.inner.widget()
    }
}

impl crate::widget_base::WidgetBase for StatusCycleButtonWidget {
    fn widget(&self) -> gtk::Widget {
        self.widget()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_label_found() {
        let options = vec![
            StatusOption { id: "a".to_string(), label: "Label A".to_string() },
            StatusOption { id: "b".to_string(), label: "Label B".to_string() },
        ];
        assert_eq!(StatusCycleButtonRender::find_label("a", &options), "Label A");
        assert_eq!(StatusCycleButtonRender::find_label("b", &options), "Label B");
    }

    #[test]
    fn test_find_label_not_found() {
        let options = vec![
            StatusOption { id: "a".to_string(), label: "Label A".to_string() },
        ];
        assert_eq!(StatusCycleButtonRender::find_label("x", &options), "---");
    }

    #[test]
    fn test_find_label_empty() {
        let options: Vec<StatusOption> = vec![];
        assert_eq!(StatusCycleButtonRender::find_label("a", &options), "---");
    }

    #[test]
    fn test_next_option_id_cycle() {
        let options = vec![
            StatusOption { id: "a".to_string(), label: "A".to_string() },
            StatusOption { id: "b".to_string(), label: "B".to_string() },
            StatusOption { id: "c".to_string(), label: "C".to_string() },
        ];

        assert_eq!(StatusCycleButtonRender::next_option_id("a", &options), "b");
        assert_eq!(StatusCycleButtonRender::next_option_id("b", &options), "c");
        assert_eq!(StatusCycleButtonRender::next_option_id("c", &options), "a");
    }

    #[test]
    fn test_next_option_id_not_found() {
        let options = vec![
            StatusOption { id: "a".to_string(), label: "A".to_string() },
            StatusOption { id: "b".to_string(), label: "B".to_string() },
        ];

        // If not found, returns the first option
        assert_eq!(StatusCycleButtonRender::next_option_id("x", &options), "a");
    }

    #[test]
    fn test_next_option_id_empty() {
        let options: Vec<StatusOption> = vec![];
        assert_eq!(StatusCycleButtonRender::next_option_id("a", &options), "");
    }

    #[test]
    fn test_next_option_id_single() {
        let options = vec![
            StatusOption { id: "a".to_string(), label: "A".to_string() },
        ];

        // Cycling with single option returns itself
        assert_eq!(StatusCycleButtonRender::next_option_id("a", &options), "a");
    }
}
