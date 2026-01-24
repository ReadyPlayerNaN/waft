//! Pure GTK4 Countdown Bar widget.
//!
//! A progress bar that counts down over a specified TTL (time to live).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gtk::prelude::*;

/// Output events from the countdown bar.
#[derive(Debug, Clone)]
pub enum CountdownBarOutput {
    Elapsed,
}

/// Pure GTK4 countdown bar widget.
pub struct CountdownBarWidget {
    pub root: gtk::ProgressBar,
    ttl: u64,
    elapsed: Rc<Cell<u64>>,
    running: Arc<AtomicBool>,
    /// Tracks if the timer completed naturally (so we don't try to remove an already-removed source)
    timer_completed: Rc<Cell<bool>>,
    timer_source: Rc<RefCell<Option<glib::SourceId>>>,
    on_output: Rc<RefCell<Option<Box<dyn Fn(CountdownBarOutput)>>>>,
}

impl CountdownBarWidget {
    /// Create a new countdown bar with the given TTL in milliseconds.
    pub fn new(ttl: u64) -> Self {
        let root = gtk::ProgressBar::builder()
            .css_classes(["notification-progress"])
            .hexpand(true)
            .vexpand(false)
            .fraction(1.0) // Start full
            .build();

        Self {
            root,
            ttl,
            elapsed: Rc::new(Cell::new(0)),
            running: Arc::new(AtomicBool::new(false)),
            timer_completed: Rc::new(Cell::new(false)),
            timer_source: Rc::new(RefCell::new(None)),
            on_output: Rc::new(RefCell::new(None)),
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(CountdownBarOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Start the countdown from the beginning.
    pub fn start(&self) {
        self.running.store(true, Ordering::Relaxed);
        self.elapsed.set(0);
        self.root.set_fraction(1.0);
        self.start_timer();
    }

    /// Stop the countdown and reset.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        self.stop_timer();
        self.elapsed.set(0);
        self.root.set_fraction(1.0);
    }

    /// Pause the countdown.
    pub fn pause(&self) {
        self.running.store(false, Ordering::Relaxed);
        self.stop_timer();
    }

    /// Continue the countdown from where it was paused.
    pub fn resume(&self) {
        self.running.store(true, Ordering::Relaxed);
        self.start_timer();
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> &gtk::ProgressBar {
        &self.root
    }

    fn start_timer(&self) {
        self.stop_timer();
        self.timer_completed.set(false);

        let tick_ms: u64 = 60;
        let running = self.running.clone();
        let elapsed = self.elapsed.clone();
        let timer_completed = self.timer_completed.clone();
        let ttl = self.ttl;
        let on_output = self.on_output.clone();
        let root = self.root.clone();

        let source_id = glib::timeout_add_local(Duration::from_millis(tick_ms), move || {
            if !running.load(Ordering::Relaxed) {
                // Timer was paused/stopped externally, mark as completed so stop_timer won't try to remove
                timer_completed.set(true);
                return glib::ControlFlow::Break;
            }

            let new_elapsed = (elapsed.get() + tick_ms).clamp(0, ttl);
            elapsed.set(new_elapsed);

            // Calculate remaining fraction (1.0 -> 0.0)
            let remaining = 1.0 - (new_elapsed as f64 / ttl as f64);
            root.set_fraction(remaining.clamp(0.0, 1.0));

            if new_elapsed >= ttl {
                running.store(false, Ordering::Relaxed);
                timer_completed.set(true);
                if let Some(ref callback) = *on_output.borrow() {
                    callback(CountdownBarOutput::Elapsed);
                }
                return glib::ControlFlow::Break;
            }

            glib::ControlFlow::Continue
        });

        *self.timer_source.borrow_mut() = Some(source_id);
    }

    fn stop_timer(&self) {
        if let Some(source_id) = self.timer_source.borrow_mut().take() {
            // Only remove if the timer hasn't already completed (which auto-removes the source)
            if !self.timer_completed.get() {
                source_id.remove();
            }
        }
    }
}

impl Drop for CountdownBarWidget {
    fn drop(&mut self) {
        self.stop_timer();
    }
}
