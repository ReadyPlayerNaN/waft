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
    pub root: gtk::Box,
    fixer: gtk::Fixed,
    fill: gtk::Box,
    ttl: u64,
    elapsed: Rc<Cell<u64>>,
    progress: Rc<Cell<f32>>,
    running: Arc<AtomicBool>,
    timer_source: Rc<RefCell<Option<glib::SourceId>>>,
    on_output: Rc<RefCell<Option<Box<dyn Fn(CountdownBarOutput)>>>>,
}

impl CountdownBarWidget {
    /// Create a new countdown bar with the given TTL in milliseconds.
    pub fn new(ttl: u64) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["notification-progress"])
            .hexpand(true)
            .vexpand(false)
            .height_request(2)
            .build();

        let fixer = gtk::Fixed::builder()
            .hexpand(true)
            .vexpand(false)
            .css_classes(["notification-progress-fix"])
            .build();

        let fill = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(false)
            .vexpand(false)
            .height_request(2)
            .css_classes(["notification-progress-fill"])
            .build();

        fixer.put(&fill, 0.0, 0.0);
        root.append(&fixer);

        Self {
            root,
            fixer,
            fill,
            ttl,
            elapsed: Rc::new(Cell::new(0)),
            progress: Rc::new(Cell::new(1.0)),
            running: Arc::new(AtomicBool::new(false)),
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
        self.progress.set(1.0);
        self.start_timer();
    }

    /// Stop the countdown and reset.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        self.stop_timer();
        self.elapsed.set(0);
        self.progress.set(1.0);
        self.update_fill_width();
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
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    fn start_timer(&self) {
        self.stop_timer();

        let tick_ms: u64 = 60;
        let running = self.running.clone();
        let elapsed = self.elapsed.clone();
        let progress = self.progress.clone();
        let ttl = self.ttl;
        let on_output = self.on_output.clone();
        let fill = self.fill.clone();
        let fixer = self.fixer.clone();
        let root = self.root.clone();

        let source_id = glib::timeout_add_local(Duration::from_millis(tick_ms), move || {
            if !running.load(Ordering::Relaxed) {
                return glib::ControlFlow::Break;
            }

            let new_elapsed = (elapsed.get() + tick_ms).clamp(0, ttl);
            elapsed.set(new_elapsed);

            let new_progress = (new_elapsed as f32 / ttl as f32).clamp(0.0, 1.0);
            progress.set(new_progress);

            // Update fill width
            let bar_w = root.allocated_width().max(0);
            if bar_w > 0 {
                let remaining = 1.0 - new_progress;
                let target_w = ((bar_w as f32) * remaining).round().max(0.0) as i32;
                fixer.move_(&fill, 0.0, 0.0);
                fill.set_size_request(target_w, 2);
            }

            if new_progress >= 1.0 {
                running.store(false, Ordering::Relaxed);
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
            source_id.remove();
        }
    }

    fn update_fill_width(&self) {
        let bar_w = self.root.allocated_width().max(0);
        if bar_w > 0 {
            let remaining = 1.0 - self.progress.get();
            let target_w = ((bar_w as f32) * remaining).round().max(0.0) as i32;
            self.fixer.move_(&self.fill, 0.0, 0.0);
            self.fill.set_size_request(target_w, 2);
        }
    }
}

impl Drop for CountdownBarWidget {
    fn drop(&mut self) {
        self.stop_timer();
    }
}
