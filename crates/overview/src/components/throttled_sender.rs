//! Throttled callback for GTK slider value changes.
//!
//! Wraps a closure so that it fires at most once per `min_interval`, discarding
//! intermediate calls. Used to avoid flooding backends (pactl, brightnessctl)
//! with rapid value changes during slider drag interactions.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

type ValueCallback = Rc<RefCell<Option<Box<dyn Fn(f64)>>>>;

/// A throttled sender that limits how frequently a callback fires.
///
/// Calls arriving before `min_interval` has elapsed since the last send are
/// silently dropped. The caller is responsible for ensuring the final value
/// is eventually sent (typically via `connect_value_commit`).
pub struct ThrottledSender {
    last_sent: Rc<RefCell<Instant>>,
    min_interval: Duration,
    callback: ValueCallback,
}

impl ThrottledSender {
    /// Create a new throttled sender with the given minimum interval between sends.
    pub fn new(min_interval: Duration) -> Self {
        Self {
            // Start in the past so the first call always fires
            last_sent: Rc::new(RefCell::new(Instant::now() - min_interval)),
            min_interval,
            callback: Rc::new(RefCell::new(None)),
        }
    }

    /// Set the callback that will be invoked on throttled sends.
    pub fn set_callback<F: Fn(f64) + 'static>(&self, callback: F) {
        *self.callback.borrow_mut() = Some(Box::new(callback));
    }

    /// Return a closure suitable for `SliderWidget::connect_value_change`.
    ///
    /// The returned closure captures the throttle state and invokes the
    /// callback at most once per `min_interval`.
    pub fn throttle_fn(&self) -> impl Fn(f64) + 'static {
        let last_sent = self.last_sent.clone();
        let min_interval = self.min_interval;
        let callback = self.callback.clone();

        move |value: f64| {
            let now = Instant::now();
            let mut last = last_sent.borrow_mut();
            if now.duration_since(*last) >= min_interval {
                *last = now;
                if let Some(ref cb) = *callback.borrow() {
                    cb(value);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn first_call_always_fires() {
        let sender = ThrottledSender::new(Duration::from_secs(10));
        let called = Rc::new(Cell::new(false));
        let called_ref = called.clone();
        sender.set_callback(move |_| called_ref.set(true));

        let throttle = sender.throttle_fn();
        throttle(0.5);
        assert!(called.get(), "first call should always fire");
    }

    #[test]
    fn call_within_interval_is_dropped() {
        let sender = ThrottledSender::new(Duration::from_secs(10));
        let count = Rc::new(Cell::new(0u32));
        let count_ref = count.clone();
        sender.set_callback(move |_| count_ref.set(count_ref.get() + 1));

        let throttle = sender.throttle_fn();
        throttle(0.1);
        throttle(0.2);
        throttle(0.3);
        assert_eq!(count.get(), 1, "only the first call should fire within interval");
    }

    #[test]
    fn call_after_interval_fires() {
        let sender = ThrottledSender::new(Duration::from_millis(10));
        let values = Rc::new(RefCell::new(Vec::new()));
        let values_ref = values.clone();
        sender.set_callback(move |v| values_ref.borrow_mut().push(v));

        let throttle = sender.throttle_fn();
        throttle(0.1);
        std::thread::sleep(Duration::from_millis(20));
        throttle(0.9);

        let vals = values.borrow();
        assert_eq!(vals.len(), 2);
        assert!((vals[0] - 0.1).abs() < f64::EPSILON);
        assert!((vals[1] - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn no_callback_does_not_panic() {
        let sender = ThrottledSender::new(Duration::from_millis(10));
        let throttle = sender.throttle_fn();
        throttle(0.5); // should not panic
    }

    #[test]
    fn set_callback_replaces_previous() {
        let sender = ThrottledSender::new(Duration::from_millis(10));
        let first_called = Rc::new(Cell::new(false));
        let second_called = Rc::new(Cell::new(false));

        let first_ref = first_called.clone();
        sender.set_callback(move |_| first_ref.set(true));

        let throttle = sender.throttle_fn();
        throttle(0.1);
        assert!(first_called.get());

        // Replace callback
        let second_ref = second_called.clone();
        sender.set_callback(move |_| second_ref.set(true));

        std::thread::sleep(Duration::from_millis(20));
        throttle(0.2);
        assert!(second_called.get(), "new callback should fire after replacement");
    }

    #[test]
    fn throttle_fn_captures_shared_state() {
        let sender = ThrottledSender::new(Duration::from_secs(10));
        let count = Rc::new(Cell::new(0u32));
        let count_ref = count.clone();
        sender.set_callback(move |_| count_ref.set(count_ref.get() + 1));

        let fn1 = sender.throttle_fn();
        let fn2 = sender.throttle_fn();

        fn1(0.1); // fires (first call)
        fn2(0.2); // dropped (shares same last_sent state)
        assert_eq!(count.get(), 1, "two throttle_fn closures share state");
    }
}
