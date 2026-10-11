//! Run from the overview's single GTK test entry point, under Xvfb in CI.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use waft_client::EntityStore;
use waft_config::ToastPosition;
use waft_protocol::entity::notification::{Notification, NotificationUrgency};
use waft_protocol::{AppNotification, Urn};
use waft_ui_gtk::widgets::countdown_bar::CountdownBarWidget;
use waft_ui_gtk::widgets::notification_card::NotificationCard;

use super::{ToastManager, reset_notification_session};

type Actions = Rc<RefCell<Vec<(Urn, String)>>>;

fn manager() -> (Rc<ToastManager>, gtk::Box, Actions) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let output = actions.clone();
    let manager = Rc::new(ToastManager::new(
        container.clone(),
        Rc::new(move |urn, action, _| {
            output.borrow_mut().push((urn, action));
            None
        }),
        Rc::new(|| {}),
        Rc::new(|_| {}),
        ToastPosition::TopRight,
        true,
    ));
    (manager, container, actions)
}

fn notification(ttl: Option<u64>) -> Notification {
    Notification {
        title: "Test".into(),
        description: "Body".into(),
        app_name: Some("Test".into()),
        app_id: Some("test".into()),
        urgency: NotificationUrgency::Normal,
        actions: vec![],
        icon_hints: vec![],
        created_at_ms: 1,
        resident: false,
        workspace: None,
        suppress_toast: false,
        ttl,
    }
}

fn urn(id: &str) -> Urn {
    Urn::new("notifications", "notification", id)
}

fn motion(widget: &gtk::Widget) -> gtk::EventControllerMotion {
    let controllers = widget.observe_controllers();
    (0..controllers.n_items())
        .find_map(|i| {
            controllers
                .item(i)?
                .downcast::<gtk::EventControllerMotion>()
                .ok()
        })
        .expect("countdown hover controller")
}

fn enter(controller: &gtk::EventControllerMotion) {
    controller.emit_by_name::<()>("enter", &[&0.0_f64, &0.0_f64]);
}

fn leave(controller: &gtk::EventControllerMotion) {
    controller.emit_by_name::<()>("leave", &[]);
}

fn pump_for(duration: Duration) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + duration;
    loop {
        while context.pending() {
            context.iteration(false);
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(crate) fn run_all() {
    duplicate_update_preserves_hovered_card_and_timer();
    queued_updates_are_upserts();
    retired_card_ignores_late_hover_and_click();
    countdown_clone_and_stop_lifetimes();
    reconnect_discards_old_ids_and_callbacks();
    default_toast_timeout_preserves_notification();
}

fn duplicate_update_preserves_hovered_card_and_timer() {
    let (manager, container, actions) = manager();
    let id = urn("duplicate");
    let mut data = notification(Some(80));
    manager.handle_notification(id.clone(), data.clone());
    let root = container.first_child().expect("toast");
    let hover = motion(&root);
    enter(&hover);

    manager.handle_notification(id.clone(), data.clone());
    data.title = "Updated".into();
    manager.handle_notification(id.clone(), data.clone());
    assert_eq!(container.first_child().as_ref(), Some(&root));
    assert_eq!(container.observe_children().n_items(), 1);
    assert_eq!(manager.test_state(), (1, 0, 1, false, true));

    // Both the original and updated notification deadlines pass while hovered.
    pump_for(Duration::from_millis(200));
    assert_eq!(manager.test_state().0, 1);
    assert!(actions.borrow().is_empty());
    leave(&hover);
    pump_for(Duration::from_millis(200));
    assert_eq!(manager.test_state().0, 0);
    assert_eq!(container.observe_children().n_items(), 0);
    assert!(actions.borrow().is_empty());

    // A status replay must not re-toast an entity whose popup already timed out.
    manager.handle_notification(id.clone(), data.clone());
    assert_eq!(manager.test_state().0, 0);
    manager.handle_entity_removed(&id);
    manager.handle_notification(id, data);
    assert_eq!(manager.test_state().0, 1);
}

fn queued_updates_are_upserts() {
    let (manager, container, _) = manager();
    let mut critical = notification(None);
    critical.urgency = NotificationUrgency::Critical;
    for id in ["1", "2", "3"] {
        manager.handle_notification(urn(id), critical.clone());
    }
    let mut queued = notification(Some(10_000));
    manager.handle_notification(urn("queued"), queued.clone());
    queued.title = "Updated queued notification".into();
    manager.handle_notification(urn("queued"), queued.clone());
    manager.handle_notification(urn("queued"), queued);
    assert_eq!(manager.test_state(), (3, 1, 3, false, true));
    manager.handle_entity_removed(&urn("1"));
    pump_for(Duration::ZERO);
    assert_eq!(manager.test_state(), (3, 0, 3, false, true));
    assert_eq!(container.observe_children().n_items(), 3);
}

fn retired_card_ignores_late_hover_and_click() {
    let card = NotificationCard::new(urn("retired"), "Test", "", &[], &[], Some(80), None);
    let outputs = Rc::new(Cell::new(0));
    let received = outputs.clone();
    card.connect_output(move |_| received.set(received.get() + 1));
    let root = card.widget().clone();
    let hover = motion(root.upcast_ref());
    card.show();
    enter(&hover);
    drop(card);

    // GTK can retain a widget/controller after its Rust owner has been retired.
    leave(&hover);
    let controllers = root.observe_controllers();
    for i in 0..controllers.n_items() {
        if let Some(click) = controllers
            .item(i)
            .and_then(|o| o.downcast::<gtk::GestureClick>().ok())
        {
            click.emit_by_name::<()>("pressed", &[&1_i32, &0.0_f64, &0.0_f64]);
        }
    }
    pump_for(Duration::from_millis(200));
    assert_eq!(outputs.get(), 0);
}

fn countdown_clone_and_stop_lifetimes() {
    let elapsed = Rc::new(Cell::new(0));
    let bar = CountdownBarWidget::new(80);
    let received = elapsed.clone();
    bar.connect_output(move |_| received.set(received.get() + 1));
    bar.start();
    drop(bar.clone());
    pump_for(Duration::from_millis(200));
    assert_eq!(
        elapsed.get(),
        1,
        "dropping one clone must not cancel the owner"
    );

    bar.start();
    bar.stop();
    bar.pause();
    bar.resume();
    pump_for(Duration::from_millis(200));
    assert_eq!(elapsed.get(), 1, "stop must make late hover-leave inert");

    bar.start();
    drop(bar);
    pump_for(Duration::from_millis(200));
    assert_eq!(elapsed.get(), 1, "last-owner drop must remove the source");
}

fn reconnect_discards_old_ids_and_callbacks() {
    let (manager, container, actions) = manager();
    let store = EntityStore::new();
    let id = urn("1");
    let data = notification(Some(80));
    store.handle_notification(AppNotification::EntityUpdated {
        urn: id.clone(),
        entity_type: Some("notification".into()),
        data: serde_json::to_value(&data).expect("notification JSON"),
    });
    manager.handle_notification(id.clone(), data.clone());
    let old_root = container.first_child().expect("old toast");
    let old_hover = motion(&old_root);
    enter(&old_hover);
    reset_notification_session(&store, &manager);
    assert!(store.get_entities_raw("notification").is_empty());
    assert_eq!(manager.test_state(), (0, 0, 0, false, true));
    assert_eq!(container.observe_children().n_items(), 0);

    // The new daemon may immediately issue the same numeric ID.
    manager.handle_notification(id, data);
    let new_root = container.first_child().expect("new toast");
    assert_ne!(new_root, old_root);
    enter(&motion(&new_root));
    leave(&old_hover);
    pump_for(Duration::from_millis(200));
    assert_eq!(manager.test_state(), (1, 0, 1, false, true));
    assert_eq!(container.observe_children().n_items(), 1);
    assert!(actions.borrow().is_empty());
}

fn default_toast_timeout_preserves_notification() {
    let (manager, container, actions) = manager();
    let store = EntityStore::new();
    let id = urn("retained");
    let data = notification(None);
    store.handle_notification(AppNotification::EntityUpdated {
        urn: id.clone(),
        entity_type: Some("notification".into()),
        data: serde_json::to_value(&data).expect("notification JSON"),
    });
    manager.handle_notification(id.clone(), data.clone());
    pump_for(Duration::from_millis(5_200));
    assert_eq!(manager.test_state().0, 0);
    assert_eq!(container.observe_children().n_items(), 0);
    assert!(
        actions.borrow().is_empty(),
        "local timeout must not send expire/dismiss"
    );
    assert!(store.has_entity(&id));
    manager.handle_notification(id, data);
    assert_eq!(manager.test_state().0, 0);
}
