use std::{cell::RefCell, rc::Rc};

use super::{model::NotificationsModel, types::Notification, view::NotificationsView};

/// A small controller that safely wires a `NotificationsModel` to a `NotificationsView`.
///
/// This is designed to be owned by the plugin so the notifications state persists even if the
/// UI is rebuilt/recreated. The controller keeps:
/// - the model (`Rc<RefCell<_>>`) so callbacks can mutate it,
/// - the view (`Rc<_>`) which owns GTK widgets,
/// - a `render_now` function that callbacks can call to request a refresh.
///
/// This avoids unsafe/self-referential closure patterns by using an `Rc<RefCell<Option<...>>>`
/// indirection slot.
pub struct NotificationsController {
    model: Rc<RefCell<NotificationsModel>>,
    view: Rc<NotificationsView>,
    render_now: Rc<dyn Fn()>,

    /// Optional hook invoked when a notification is closed/dismissed from the UI.
    ///
    /// This exists to allow higher layers (e.g. a DBus notifications server integration) to
    /// emit external side-effects (like `NotificationClosed`) without putting DBus concepts
    /// into the view layer.
    on_notification_closed: Rc<RefCell<Option<Rc<dyn Fn(u64)>>>>,
}

impl NotificationsController {
    /// Create a controller with an initial set of notifications.
    pub fn new(initial: Vec<Notification>) -> Self {
        let model = Rc::new(RefCell::new(NotificationsModel::new()));
        for n in initial {
            model.borrow_mut().add(n);
        }

        let view = Rc::new(NotificationsView::new());

        let on_notification_closed: Rc<RefCell<Option<Rc<dyn Fn(u64)>>>> =
            Rc::new(RefCell::new(None));

        // Two-step wiring for render function:
        // - create a slot that can later store the final Rc<dyn Fn()>
        // - create render_impl which captures the slot and looks up render_now for callbacks
        let render_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

        let model_for_render = model.clone();
        let view_for_render = view.clone();
        let render_slot_for_render = render_slot.clone();
        let on_notification_closed_for_render = on_notification_closed.clone();

        let render_impl: Rc<dyn Fn()> = Rc::new(move || {
            let snapshot = model_for_render.borrow().snapshot();

            let render_now = render_slot_for_render
                .borrow()
                .as_ref()
                .expect("render_now must be initialized")
                .clone();

            let on_close_notification = {
                let model = model_for_render.clone();
                let render_now = render_now.clone();
                let on_notification_closed = on_notification_closed_for_render.clone();
                move |id: u64| {
                    // UI-driven close (dismiss by user).
                    let removed = model.borrow_mut().remove(id);
                    if removed {
                        if let Some(cb) = on_notification_closed.borrow().as_ref() {
                            (cb)(id);
                        }
                    }
                    (render_now)();
                }
            };

            let on_toggle_group = {
                let model = model_for_render.clone();
                let render_now = render_now.clone();
                move |app_key: String| {
                    model.borrow_mut().toggle_open_group(&app_key);
                    (render_now)();
                }
            };

            let on_close_all_groups = {
                let model = model_for_render.clone();
                let render_now = render_now.clone();
                move || {
                    model.borrow_mut().set_open_group(None);
                    (render_now)();
                }
            };

            view_for_render.render_from_snapshot(
                snapshot,
                on_close_notification,
                on_toggle_group,
                on_close_all_groups,
            );
        });

        // Publish render_impl into the slot so render_impl can fetch it for callbacks.
        *render_slot.borrow_mut() = Some(render_impl.clone());

        let controller = Self {
            model: model.clone(),
            view: view.clone(),
            render_now: render_impl.clone(),
            on_notification_closed,
        };

        // Wire "Clear" button => model.clear() + rerender.
        {
            let model = controller.model.clone();
            let render_now = controller.render_now.clone();
            controller.view.connect_clear(move || {
                model.borrow_mut().clear();
                (render_now)();
            });
        }

        controller
    }

    /// Install/replace a hook that will be called when the user dismisses a notification in the UI.
    ///
    /// This hook is invoked *after* the notification is removed from the model.
    pub fn set_on_notification_closed<F: Fn(u64) + 'static>(&self, f: F) {
        *self.on_notification_closed.borrow_mut() = Some(Rc::new(f));
    }

    /// Get the root widget to insert into the UI.
    pub fn widget(&self) -> gtk::Widget {
        self.view.widget()
    }

    /// Force a render using the current model snapshot.
    pub fn render_now(&self) {
        (self.render_now)();
    }

    /// Add a new notification (imperative API).
    pub fn add(&self, n: Notification) {
        self.model.borrow_mut().add(n);
        (self.render_now)();
    }

    /// Remove a notification by id (imperative API).
    pub fn remove(&self, id: u64) -> bool {
        let removed = self.model.borrow_mut().remove(id);
        if removed {
            (self.render_now)();
        }
        removed
    }

    /// Fetch a notification by id from the underlying model (GTK-free).
    ///
    /// This is intended for toast rendering, where we keep a toast-id stack and need to resolve
    /// ids into full `Notification` payloads without rebuilding/group-expanding the overlay UI.
    ///
    /// Returns a cloned `Notification` if found.
    pub fn get_by_id(&self, id: u64) -> Option<Notification> {
        self.model.borrow().get_by_id(id)
    }
}
