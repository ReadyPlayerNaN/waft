pub mod agenda;
pub mod audio_sliders;
pub mod battery;
pub mod brightness_sliders;
pub mod calendar;
pub mod claude;
pub mod clock;
pub mod entity_keyed_base;
pub mod events;
pub mod keyboard_layout;
pub mod notification_group;
pub mod notification_list;
pub mod right_column_stack;
pub mod session_actions;
pub mod settings_button;
pub mod system_actions;
pub mod throttled_sender;
pub mod toggles;
pub mod weather;

/// Single GTK test entry point for component tests that create widgets.
///
/// GTK can only be initialized once per process on the main thread.
/// All GTK widget tests must run from this single `#[test]` function
/// to avoid thread contention on the GLib main context.
#[cfg(test)]
mod gtk_component_tests {
    use std::rc::Rc;
    use std::sync::Once;

    use glib::object::Cast;
    use gtk::prelude::{ListModelExt, WidgetExt};
    use waft_client::EntityStore;
    use waft_config::ToastPosition;
    use waft_protocol::Urn;
    use waft_protocol::entity::notification::{Notification, NotificationUrgency};
    use waft_protocol::message::AppNotification;

    use crate::features::toasts::ToastManager;

    fn init_gtk() -> bool {
        static GTK_INIT: Once = Once::new();
        static GTK_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        GTK_INIT.call_once(|| {
            if gtk::init().is_ok() {
                GTK_READY.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
        GTK_READY.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[test]
    fn all_gtk_component_tests() {
        if !init_gtk() {
            eprintln!("Skipping GTK component tests: GTK unavailable in this environment");
            return;
        }

        super::brightness_sliders::tests::run_all();
        super::audio_sliders::tests::run_all_gtk();
        super::entity_keyed_base::tests::run_all();
        notification_list_removes_empty_group_shells();
        toast_manager_visibility_policy_and_removal_cleanup();
        toast_manager_keeps_local_state_when_clear_disabled();
    }

    fn notification_list_removes_empty_group_shells() {
        let store = Rc::new(EntityStore::new());
        let action_callback: waft_client::EntityActionCallback = Rc::new(|_, _, _| None);
        let menu_store = Rc::new(crate::menu_state::create_menu_store());
        let component = super::notification_list::NotificationsComponent::new(
            &store,
            &action_callback,
            &menu_store,
        );

        let groups_container = component
            .widget()
            .first_child()
            .and_then(|header| header.next_sibling())
            .and_then(|w| w.downcast::<gtk::Box>().ok())
            .expect("groups container");

        let placeholder = groups_container
            .next_sibling()
            .and_then(|w| w.downcast::<gtk::Box>().ok())
            .expect("empty placeholder");

        let make_notification = |id: &str, created_at_ms: i64| Notification {
            title: format!("Title {id}"),
            description: format!("Body {id}"),
            app_name: Some("App".to_string()),
            app_id: Some("app".to_string()),
            urgency: NotificationUrgency::Normal,
            actions: Vec::new(),
            icon_hints: Vec::new(),
            created_at_ms,
            resident: false,
            workspace: None,
            suppress_toast: false,
            ttl: None,
        };

        let urn1 = Urn::new("notifications", "notification", "1");
        let urn2 = Urn::new("notifications", "notification", "2");

        store.handle_notification(AppNotification::EntityUpdated {
            urn: urn1.clone(),
            entity_type: Some("notification".to_string()),
            data: serde_json::to_value(make_notification("1", 2)).expect("json"),
        });
        store.handle_notification(AppNotification::EntityUpdated {
            urn: urn2.clone(),
            entity_type: Some("notification".to_string()),
            data: serde_json::to_value(make_notification("2", 1)).expect("json"),
        });

        assert_eq!(groups_container.observe_children().n_items(), 1);
        assert!(groups_container.is_visible());
        assert!(!placeholder.is_visible());

        store.handle_notification(AppNotification::EntityRemoved {
            urn: urn1,
            entity_type: Some("notification".to_string()),
        });
        assert_eq!(groups_container.observe_children().n_items(), 1);

        store.handle_notification(AppNotification::EntityRemoved {
            urn: urn2.clone(),
            entity_type: Some("notification".to_string()),
        });

        assert_eq!(groups_container.observe_children().n_items(), 0);
        assert!(!groups_container.is_visible());
        assert!(placeholder.is_visible());

        store.handle_notification(AppNotification::EntityRemoved {
            urn: urn2,
            entity_type: Some("notification".to_string()),
        });

        assert_eq!(groups_container.observe_children().n_items(), 0);
        assert!(!groups_container.is_visible());
        assert!(placeholder.is_visible());
    }

    fn toast_manager_visibility_policy_and_removal_cleanup() {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let visibility = Rc::new(std::cell::Cell::new(true));
        let mgr = Rc::new(ToastManager::new(
            container,
            Rc::new(|_, _, _| None),
            Rc::new(|| {}),
            {
                let visibility = visibility.clone();
                Rc::new(move |visible| visibility.set(visible))
            },
            ToastPosition::TopRight,
            true,
        ));

        let notification = Notification {
            title: "Title".into(),
            description: "Body".into(),
            app_name: Some("App".into()),
            app_id: Some("app".into()),
            urgency: NotificationUrgency::Normal,
            actions: vec![],
            icon_hints: vec![],
            created_at_ms: 1,
            resident: false,
            workspace: None,
            suppress_toast: false,
            ttl: None,
        };

        let urn = Urn::new("notifications", "notification", "1");
        mgr.handle_notification(urn.clone(), notification.clone());
        assert_eq!(mgr.test_state().0, 1);
        assert_eq!(mgr.test_state().2, 1);

        mgr.set_overview_visible(true);
        assert_eq!(mgr.test_state(), (0, 0, 0, true, true));
        assert!(!visibility.get());

        mgr.set_overview_visible(false);
        mgr.handle_notification(urn.clone(), notification);
        assert_eq!(mgr.test_state().0, 1);
        assert_eq!(mgr.test_state().2, 1);

        mgr.handle_entity_removed(&urn);
        assert_eq!(mgr.test_state().0, 0);
        assert_eq!(mgr.test_state().1, 0);
        assert_eq!(mgr.test_state().2, 0);
    }

    fn toast_manager_keeps_local_state_when_clear_disabled() {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let visibility = Rc::new(std::cell::Cell::new(true));
        let mgr = Rc::new(ToastManager::new(
            container,
            Rc::new(|_, _, _| None),
            Rc::new(|| {}),
            {
                let visibility = visibility.clone();
                Rc::new(move |visible| visibility.set(visible))
            },
            ToastPosition::TopRight,
            false,
        ));

        let notification = Notification {
            title: "Title".into(),
            description: "Body".into(),
            app_name: Some("App".into()),
            app_id: Some("app".into()),
            urgency: NotificationUrgency::Normal,
            actions: vec![],
            icon_hints: vec![],
            created_at_ms: 1,
            resident: false,
            workspace: None,
            suppress_toast: false,
            ttl: None,
        };

        let urn = Urn::new("notifications", "notification", "2");
        mgr.handle_notification(urn.clone(), notification);
        assert_eq!(mgr.test_state().0, 1);

        mgr.set_overview_visible(true);
        assert_eq!(mgr.test_state(), (1, 0, 1, true, false));
        assert!(!visibility.get());
    }
}
