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
