//! Day cell widget for the calendar month grid (RenderFn).
//!
//! A presentational widget that renders a single day in the calendar.
//! Shows the day number, up to 3 event dots, and visual states for
//! today, selected, and other-month days.

use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VBox, VCustomButton, VLabel, VNode};

/// Input properties for a day cell.
#[derive(Clone, PartialEq)]
pub struct DayCellProps {
    /// Day number (1-31).
    pub day: u32,
    /// Whether this day belongs to the currently viewed month.
    pub current_month: bool,
    /// Whether this day is today.
    pub today: bool,
    /// Whether this day is currently selected.
    pub selected: bool,
    /// Number of events on this day (dots shown for up to 3).
    pub event_count: usize,
}

/// Output events emitted by the day cell.
pub enum DayCellOutput {
    /// The day was clicked. Contains the day number.
    #[allow(dead_code)]
    Clicked(u32),
}

pub(crate) struct DayCellRender;

impl RenderFn for DayCellRender {
    type Props = DayCellProps;
    type Output = DayCellOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        // Build dots row (up to 3 small boxes with calendar-event-dot class)
        let dot_count = props.event_count.min(3);
        let mut dots = VBox::horizontal(2).halign(gtk::Align::Center);
        for _ in 0..dot_count {
            dots = dots.child(VNode::vbox(
                VBox::horizontal(0).css_class("calendar-event-dot"),
            ));
        }

        // Content: day label + dots
        let content = VBox::vertical(2)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .child(VNode::label(
                VLabel::new(props.day.to_string()).xalign(0.5),
            ))
            .child(VNode::vbox(dots));

        // CSS classes
        let mut button = VCustomButton::new(VNode::vbox(content))
            .css_class("calendar-day-cell");

        if props.today {
            button = button.css_class("today");
        }
        if props.selected {
            button = button.css_class("selected");
        }
        if !props.current_month {
            button = button.css_class("other-month");
        }
        if props.event_count > 0 {
            button = button.css_class("has-events");
        }

        let emit_clone = emit.clone();
        let day = props.day;
        button = button.on_click(move || {
            if let Some(ref cb) = *emit_clone.borrow() {
                cb(DayCellOutput::Clicked(day));
            }
        });

        VNode::custom_button(button)
    }
}

pub type DayCellComponent = RenderComponent<DayCellRender>;
