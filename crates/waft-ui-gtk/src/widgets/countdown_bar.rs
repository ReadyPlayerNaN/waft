//! A countdown progress bar widget for notification toast expiry.
//!
//! Ticks via `glib::timeout_add_local`, deriving progress from monotonic time
//! so the bar stays accurate even if the main loop is delayed. Fires
//! `CountdownBarOutput::Elapsed` when the countdown reaches zero.
//!
//! The visual bar is rendered by `CountdownBarRender` (a `RenderFn`); the timer
//! logic remains in `CountdownBarWidget`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

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

pub type CountdownBarComponent = RenderComponent<CountdownBarRender>;

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(CountdownBarOutput)>>>>;

const TICK_INTERVAL_MS: u64 = 60;

#[derive(Debug, Default)]
struct CountdownState {
    elapsed_before_run: Duration,
    run_started_at: Option<Instant>,
    finished: bool,
}

impl CountdownState {
    fn start(&mut self) {
        self.elapsed_before_run = Duration::ZERO;
        self.run_started_at = Some(Instant::now());
        self.finished = false;
    }

    fn pause(&mut self, ttl: Duration) -> Duration {
        if let Some(started_at) = self.run_started_at.take() {
            self.elapsed_before_run = (self.elapsed_before_run + started_at.elapsed()).min(ttl);
        }
        self.remaining(ttl)
    }

    fn resume(&mut self, ttl: Duration) -> Duration {
        if self.finished {
            return Duration::ZERO;
        }
        if self.run_started_at.is_none() && self.remaining(ttl) > Duration::ZERO {
            self.run_started_at = Some(Instant::now());
        }
        self.remaining(ttl)
    }

    fn stop(&mut self) {
        self.run_started_at = None;
        self.finished = true;
    }

    fn mark_elapsed(&mut self, ttl: Duration) {
        self.elapsed_before_run = ttl;
        self.run_started_at = None;
        self.finished = true;
    }

    fn elapsed(&self) -> Duration {
        self.elapsed_before_run
            + self.run_started_at.map(|started_at| started_at.elapsed()).unwrap_or(Duration::ZERO)
    }

    fn remaining(&self, ttl: Duration) -> Duration {
        ttl.saturating_sub(self.elapsed())
    }

    fn fraction(&self, ttl: Duration) -> f64 {
        if ttl.is_zero() {
            return 0.0;
        }
        let remaining = self.remaining(ttl).as_secs_f64();
        (remaining / ttl.as_secs_f64()).clamp(0.0, 1.0)
    }

    fn is_running(&self) -> bool {
        self.run_started_at.is_some() && !self.finished
    }
}

/// A progress bar that counts down from full to empty over `ttl_ms` milliseconds.
#[derive(Clone)]
pub struct CountdownBarWidget {
    inner: Rc<CountdownBarComponent>,
    ttl: Duration,
    state: Rc<RefCell<CountdownState>>,
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
            ttl: Duration::from_millis(ttl_ms),
            state: Rc::new(RefCell::new(CountdownState::default())),
            timer_source: Rc::new(RefCell::new(None)),
            on_output: Rc::new(RefCell::new(None)),
        }
    }

    pub fn root(&self) -> gtk::Widget {
        self.inner.widget()
    }

    pub fn connect_output<F: Fn(CountdownBarOutput) + 'static>(&self, f: F) {
        *self.on_output.borrow_mut() = Some(Box::new(f));
    }

    pub fn start(&self) {
        self.stop_timer();
        self.state.borrow_mut().start();
        self.inner.update(&CountdownBarProps { fraction: 1.0, paused: false });
        self.start_timer();
    }

    pub fn stop(&self) {
        self.stop_timer();
        self.state.borrow_mut().stop();
    }

    pub fn pause(&self) {
        self.stop_timer();
        let fraction = self.state.borrow_mut().pause(self.ttl);
        self.inner.update(&CountdownBarProps { fraction: fraction.as_secs_f64() / self.ttl.as_secs_f64(), paused: true });
    }

    pub fn resume(&self) {
        let fraction = {
            let mut state = self.state.borrow_mut();
            if state.finished || state.is_running() {
                return;
            }
            state.resume(self.ttl);
            state.fraction(self.ttl)
        };
        self.inner.update(&CountdownBarProps { fraction, paused: false });
        self.start_timer();
    }

    fn start_timer(&self) {
        if self.timer_source.borrow().is_some() || self.state.borrow().finished {
            return;
        }

        let state = Rc::clone(&self.state);
        let ttl = self.ttl;
        let on_output = self.on_output.clone();
        let timer_source = self.timer_source.clone();
        let inner = Rc::clone(&self.inner);

        let source_id = gtk::glib::timeout_add_local(Duration::from_millis(TICK_INTERVAL_MS), move || {
            let (finished, fraction) = {
                let mut state = state.borrow_mut();
                if state.finished {
                    return gtk::glib::ControlFlow::Break;
                }

                let remaining = state.remaining(ttl);
                let fraction = state.fraction(ttl);
                let finished = remaining.is_zero();
                if finished {
                    state.mark_elapsed(ttl);
                }
                (finished, fraction)
            };

            if finished {
                inner.update(&CountdownBarProps { fraction: 0.0, paused: false });
                *timer_source.borrow_mut() = None;
                if let Some(ref cb) = *on_output.borrow() {
                    cb(CountdownBarOutput::Elapsed);
                }
                return gtk::glib::ControlFlow::Break;
            }

            inner.update(&CountdownBarProps { fraction, paused: false });
            gtk::glib::ControlFlow::Continue
        });

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

#[cfg(test)]
mod tests {
    use super::CountdownState;
    use std::time::Duration;

    #[test]
    fn countdown_state_tracks_pause_and_resume_without_fixed_steps() {
        let ttl = Duration::from_millis(1_000);
        let mut state = CountdownState::default();
        state.start();
        state.elapsed_before_run = Duration::from_millis(400);
        state.run_started_at = None;

        let remaining = state.pause(ttl);
        assert!((remaining.as_secs_f64() - 0.6).abs() < 0.01);

        let resumed = state.resume(ttl);
        assert!((resumed.as_secs_f64() - 0.6).abs() < 0.01);
        assert!(!state.finished);
    }

    #[test]
    fn countdown_state_fraction_is_derived_from_elapsed_time() {
        let ttl = Duration::from_millis(1_000);
        let mut state = CountdownState::default();
        state.start();
        state.elapsed_before_run = Duration::from_millis(250);
        state.run_started_at = None;

        assert!((state.fraction(ttl) - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn countdown_state_stop_is_inert() {
        let mut state = CountdownState::default();
        state.start();
        state.stop();
        assert!(state.finished);
        assert!(!state.is_running());
    }
}
