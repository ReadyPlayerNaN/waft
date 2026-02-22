//! Entity change notifier for plugins.
//!
//! `EntityNotifier` uses a `watch::channel` counter to signal that plugin
//! state has changed. The runtime calls `get_entities()` and sends updates
//! to the daemon when the counter changes.

use tokio::sync::watch;

/// Notifier that plugins use to signal that entities have changed.
///
/// When `notify()` is called, the runtime re-reads all entities from the
/// plugin and sends updates to the waft daemon.
#[derive(Clone)]
pub struct EntityNotifier {
    tx: watch::Sender<u64>,
}

impl EntityNotifier {
    pub(crate) fn new() -> (Self, watch::Receiver<u64>) {
        let (tx, rx) = watch::channel(0u64);
        (Self { tx }, rx)
    }

    /// Signal that entity state has changed.
    ///
    /// Returns `true` if the runtime is still running, `false` if it has
    /// stopped (receiver dropped). Background tasks should break their loop
    /// when this returns `false`.
    pub fn notify(&self) -> bool {
        let cur = *self.tx.borrow();
        if self.tx.send(cur.wrapping_add(1)).is_err() {
            log::warn!("[plugin] notifier send failed — runtime may have stopped");
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_increments_counter() {
        let (notifier, mut rx) = EntityNotifier::new();
        assert_eq!(*rx.borrow(), 0);

        notifier.notify();
        assert!(rx.has_changed().unwrap());
        assert_eq!(*rx.borrow_and_update(), 1);

        notifier.notify();
        notifier.notify();
        assert_eq!(*rx.borrow_and_update(), 3);
    }

    #[test]
    fn notify_returns_true_while_runtime_alive() {
        let (notifier, _rx) = EntityNotifier::new();
        assert!(notifier.notify());
    }

    #[test]
    fn notify_returns_false_after_receiver_drop() {
        let (notifier, rx) = EntityNotifier::new();
        drop(rx);
        assert!(!notifier.notify());
    }
}
