//! A countdown progress bar widget for notification toast expiry.
//!
//! Ticks every 60ms via `glib::timeout_add_local`, decrementing a progress bar
//! from 1.0 to 0.0. Fires `CountdownBarOutput::Elapsed` when the countdown
//! reaches zero.
//!
//! The visual bar is rendered by `CountdownBarRender` (a `RenderFn`); the timer
//! logic remains in `CountdownBarWidget`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::vdom::{Component, RenderCallback, RenderComponent, RenderFn, VNode, VProgressBar};

/// Output events from a countdown bar.
pub enum CountdownBarOutput {
    Elapsed,
}

/// Props for the countdown bar visual.
#[derive(Clone, PartialEq)]
pub struct CountdownBarProps {
    pub fraction: f64,
    pub paused: bool,
}

/// Pure render function for the countdown bar visual.
pub struct CountdownBarRender;

impl RenderFn for CountdownBarRender {
    type Props = CountdownBarProps;
    type Output = ();

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        VNode::progress_bar(
            VProgressBar::new(props.fraction)
                .css_class("notification-progress")
                .css_class_if(props.paused, "paused"),
        )
    }
}

/// Type alias for the render component.
pub type CountdownBarComponent = RenderComponent<CountdownBarRender>;

/// Type alias for output callback.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(CountdownBarOutput)>>>>;

const TICK_INTERVAL_MS: u64 = 60;

/// A progress bar that counts down from full to empty over `ttl_ms` milliseconds.
pub struct CountdownBarWidget {
    inner: Rc<CountdownBarComponent>,
    ttl_ms: u64,
    elapsed_ms: Rc<Cell<u64>>,
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    timer_source: Rc<RefCell<Option<gtk::glib::SourceId>>>,
    on_output: OutputCallback,
}

impl CountdownBarWidget {
    pub fn new(ttl_ms: u64) -> Self {
        let inner = Rc::new(CountdownBarComponent::build(&CountdownBarProps {
            fraction: 1.0,
            paused: false,
        }));

        Self {
            inner,
            ttl_ms,
            elapsed_ms: Rc::new(Cell::new(0)),
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            timer_source: Rc::new(RefCell::new(None)),
            on_output: Rc::new(RefCell::new(None)),
        }
    }

    /// Get the root widget.
    pub fn root(&self) -> gtk::Widget {
        self.inner.widget()
    }

    /// Set the callback for output events.
    pub fn connect_output<F: Fn(CountdownBarOutput) + 'static>(&self, f: F) {
        *self.on_output.borrow_mut() = Some(Box::new(f));
    }

    /// Start the countdown timer from the beginning.
    pub fn start(&self) {
        self.stop_timer();
        self.elapsed_ms.set(0);
        self.running.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
        self.inner.update(&CountdownBarProps {
            fraction: 1.0,
            paused: false,
        });
        self.start_timer();
    }

    /// Stop the countdown timer completely.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.stop_timer();
    }

    /// Pause the countdown (timer keeps ticking but elapsed does not advance).
    pub fn pause(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume the countdown after a pause.
    pub fn resume(&self) {
        self.running.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Get a clone of the running flag for external pause/resume control.
    pub fn running_handle(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    /// Get a clone of the paused flag for external pause/resume control.
    pub fn paused_handle(&self) -> Arc<AtomicBool> {
        self.paused.clone()
    }

    fn start_timer(&self) {
        let elapsed_ms = self.elapsed_ms.clone();
        let ttl_ms = self.ttl_ms;
        let running = self.running.clone();
        let paused = self.paused.clone();
        let on_output = self.on_output.clone();
        let timer_source = self.timer_source.clone();
        let inner = Rc::clone(&self.inner);

        let source_id = gtk::glib::timeout_add_local(
            std::time::Duration::from_millis(TICK_INTERVAL_MS),
            move || {
                let is_paused = paused.load(Ordering::SeqCst);

                if !running.load(Ordering::SeqCst) {
                    // Paused — keep the timer alive but don't advance.
                    // Update UI to reflect paused state.
                    let fraction = 1.0 - (elapsed_ms.get() as f64 / ttl_ms as f64);
                    inner.update(&CountdownBarProps {
                        fraction,
                        paused: is_paused,
                    });
                    return gtk::glib::ControlFlow::Continue;
                }

                let new_elapsed = elapsed_ms.get() + TICK_INTERVAL_MS;
                elapsed_ms.set(new_elapsed);

                if new_elapsed >= ttl_ms {
                    inner.update(&CountdownBarProps {
                        fraction: 0.0,
                        paused: false,
                    });
                    // Clear the source before firing callback to prevent
                    // double-removal in Drop
                    *timer_source.borrow_mut() = None;
                    if let Some(ref cb) = *on_output.borrow() {
                        cb(CountdownBarOutput::Elapsed);
                    }
                    return gtk::glib::ControlFlow::Break;
                }

                let fraction = 1.0 - (new_elapsed as f64 / ttl_ms as f64);
                inner.update(&CountdownBarProps {
                    fraction,
                    paused: is_paused,
                });
                gtk::glib::ControlFlow::Continue
            },
        );

        *self.timer_source.borrow_mut() = Some(source_id);
    }

    fn stop_timer(&self) {
        if let Some(source_id) = self.timer_source.borrow_mut().take() {
            source_id.remove();
        }
    }
}

impl Drop for CountdownBarWidget {
    fn drop(&mut self) {
        self.stop_timer();
    }
}
