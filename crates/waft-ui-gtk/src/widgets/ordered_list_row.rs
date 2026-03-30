//! OrderedListRow widget - draggable row with drag handle.
//!
//! A reusable component for rows in drag-and-drop ordered lists.
//! Uses `adw::ActionRow` with native title/subtitle/suffix layout.
//! Drag is initiated ONLY via the drag handle icon, not the entire row.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;

use crate::widget_base::WidgetBase;

/// Properties for initializing an ordered list row.
pub struct OrderedListRowProps {
    /// Stable identifier for this row (transferred during drag-drop).
    pub id: String,
    /// Whether drag functionality is enabled.
    pub draggable: bool,
    /// Row title text.
    pub title: String,
    /// Optional subtitle text.
    pub subtitle: Option<String>,
}

/// Output events from the ordered list row.
#[derive(Debug, Clone)]
pub enum OrderedListRowOutput {
    /// Drag operation started on this row.
    DragBegin(String),
    /// Drag operation ended on this row.
    DragEnd(String),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(OrderedListRowOutput)>>>>;

/// OrderedListRow widget - draggable row with drag handle.
#[derive(Clone)]
pub struct OrderedListRow {
    pub root: adw::ActionRow,
    id: String,
    draggable: Rc<RefCell<bool>>,
    drag_handle_box: gtk::Box,
    output_cb: OutputCallback,
}

impl OrderedListRow {
    /// Create a new ordered list row.
    pub fn new(props: &OrderedListRowProps) -> Self {
        let mut builder = adw::ActionRow::builder()
            .title(&props.title)
            .activatable(false)
            .css_classes(["ordered-list-row"]);

        if let Some(ref subtitle) = props.subtitle {
            builder = builder.subtitle(subtitle);
        }

        let root = builder.build();

        // Drag handle (prefix)
        let drag_handle_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .valign(gtk::Align::Center)
            .css_classes(["drag-handle"])
            .build();

        let drag_icon = gtk::Image::builder()
            .icon_name("list-drag-handle-symbolic")
            .pixel_size(16)
            .build();

        drag_handle_box.append(&drag_icon);
        drag_handle_box.set_cursor_from_name(Some("grab"));
        root.add_prefix(&drag_handle_box);

        let draggable = Rc::new(RefCell::new(props.draggable));
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        let instance = Self {
            root: root.clone(),
            id: props.id.clone(),
            draggable,
            drag_handle_box: drag_handle_box.clone(),
            output_cb,
        };

        // Setup drag source if draggable
        if props.draggable {
            instance.setup_drag_source(&props.id, &root);
        }

        // Update CSS classes
        instance.update_css_classes();

        instance
    }

    /// Setup drag source for drag-and-drop.
    fn setup_drag_source(&self, id: &str, row: &adw::ActionRow) {
        let drag_source = gtk::DragSource::new();
        drag_source.set_actions(gdk::DragAction::MOVE);

        // Prepare drag data (item ID as string)
        let id_clone = id.to_string();
        drag_source.connect_prepare(move |_source, _x, _y| {
            let value = gtk::glib::Value::from(&id_clone);
            Some(gdk::ContentProvider::for_value(&value))
        });

        // Drag begin: add "dragging" CSS class and emit event
        let row_clone = row.clone();
        let id_clone = id.to_string();
        let output_clone = self.output_cb.clone();
        drag_source.connect_drag_begin(move |_source, _drag| {
            crate::css::add_class(&row_clone, "dragging");
            if let Some(ref callback) = *output_clone.borrow() {
                callback(OrderedListRowOutput::DragBegin(id_clone.clone()));
            }
        });

        // Drag end: remove "dragging" CSS class and emit event
        let row_clone = row.clone();
        let id_clone = id.to_string();
        let output_clone = self.output_cb.clone();
        drag_source.connect_drag_end(move |_source, _drag, _delete_data| {
            crate::css::remove_class(&row_clone, "dragging");
            if let Some(ref callback) = *output_clone.borrow() {
                callback(OrderedListRowOutput::DragEnd(id_clone.clone()));
            }
        });

        // Attach drag source to the drag handle box (not the entire row)
        self.drag_handle_box.add_controller(drag_source);
    }

    /// Add a suffix widget to the row.
    pub fn add_suffix(&self, widget: &impl IsA<gtk::Widget>) {
        self.root.add_suffix(widget);
    }

    /// Enable or disable drag functionality.
    pub fn set_draggable(&self, draggable: bool) {
        *self.draggable.borrow_mut() = draggable;
        self.update_css_classes();
    }

    /// Get the item ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(OrderedListRowOutput) + 'static,
    {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    fn update_css_classes(&self) {
        crate::css::apply_state_classes(
            &self.root,
            Some("ordered-list-row"),
            &[("draggable", *self.draggable.borrow())],
        );
    }
}

impl WidgetBase for OrderedListRow {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}
