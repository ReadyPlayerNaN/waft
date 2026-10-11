mod toast_manager;
mod toast_window;

pub use toast_manager::ToastManager;
pub use toast_window::ToastWindow;

/// Drop notification state at a daemon-session boundary before accepting reused IDs.
/// Use normal removal events so notification-list subscribers reconcile as well.
pub(crate) fn reset_notification_session(store: &waft_client::EntityStore, toasts: &ToastManager) {
    toasts.reset();
    for (urn, _) in
        store.get_entities_raw(waft_protocol::entity::notification::NOTIFICATION_ENTITY_TYPE)
    {
        store.handle_notification(waft_protocol::AppNotification::EntityRemoved {
            urn,
            entity_type: Some(waft_protocol::entity::notification::NOTIFICATION_ENTITY_TYPE.into()),
        });
    }
}

#[cfg(test)]
pub(crate) mod tests;
