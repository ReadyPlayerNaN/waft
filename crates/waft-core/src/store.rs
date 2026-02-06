//! Generic plugin store infrastructure.
//!
//! Provides a reusable store pattern for plugins with:
//! - State management via `RwLock`
//! - Subscription-based notifications
//! - Configuration support via traits

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::RwLock;

/// Helper macro for setting a field if its value changes.
///
/// Returns `true` if the value changed, `false` otherwise.
/// This is the most common pattern in store operations.
///
/// # Examples
///
/// ```ignore
/// set_field!(state.enabled, enabled) // Sets state.enabled = enabled if changed
/// set_field!(state.volume, volume, |v| (v - old_v).abs() > f64::EPSILON) // Custom comparison
/// ```
#[macro_export]
macro_rules! set_field {
    // Simple field setter with default equality check
    ($state_field:expr, $value:expr) => {{
        if $state_field != $value {
            $state_field = $value;
            true
        } else {
            false
        }
    }};
    // Field setter with custom comparison function
    ($state_field:expr, $value:expr, $cmp:expr) => {{
        if $cmp(&$state_field, &$value) {
            $state_field = $value;
            true
        } else {
            false
        }
    }};
}

/// Marker trait for store operations.
///
/// Operations are dispatched to the store and processed by the processor function.
/// Each operation may or may not result in state changes.
pub trait StoreOp: Clone + 'static {}

/// Trait for store state with configuration support.
///
/// States must be `Default` for initial creation and support configuration updates.
pub trait StoreState: Default {
    /// Configuration type for this state.
    type Config: Default + Clone;

    /// Apply configuration to the state.
    fn configure(&mut self, config: &Self::Config);
}

/// Generic plugin store with subscription support.
///
/// The store manages state and notifies subscribers when state changes.
/// Operations are processed by a custom processor function that determines
/// whether the state changed.
///
/// # Thread Safety
///
/// The store uses `RwLock` for state access, making it safe to read state
/// from multiple threads. However, subscribers are stored in `RefCell`
/// and must only be accessed from the main thread.
///
/// # Usage Pattern
///
/// Background threads should send operations via channels to the main thread,
/// which then calls `emit()`. This ensures subscribers (which may capture
/// GTK widgets) are only called from the main thread.
/// Type alias for the store operation processor function.
type StoreProcessor<Op, State> = Box<dyn Fn(&mut State, Op) -> bool>;

pub struct PluginStore<Op, State>
where
    Op: StoreOp,
    State: StoreState,
{
    state: RwLock<State>,
    subscribers: RefCell<Vec<Rc<dyn Fn()>>>,
    processor: StoreProcessor<Op, State>,
}

impl<Op, State> PluginStore<Op, State>
where
    Op: StoreOp,
    State: StoreState,
{
    /// Create a new store with the given operation processor.
    ///
    /// The processor function receives mutable state and an operation,
    /// returning `true` if the state changed (triggering subscriber notification).
    pub fn new<F>(processor: F) -> Self
    where
        F: Fn(&mut State, Op) -> bool + 'static,
    {
        Self {
            state: RwLock::new(State::default()),
            subscribers: RefCell::new(Vec::new()),
            processor: Box::new(processor),
        }
    }

    /// Get read access to the current state.
    pub fn get_state(&self) -> std::sync::RwLockReadGuard<'_, State> {
        self.state.read().unwrap()
    }

    /// Apply configuration to the state.
    pub fn configure(&self, config: State::Config) {
        let mut state = self.state.write().unwrap();
        state.configure(&config);
    }

    /// Emit an operation to modify state.
    ///
    /// The operation is processed by the processor function.
    /// If the processor returns `true` (indicating state changed),
    /// all subscribers are notified.
    pub fn emit(&self, op: Op) {
        let changed = {
            let mut state = self.state.write().unwrap();
            (self.processor)(&mut state, op)
        };
        if changed {
            self.notify_subscribers();
        }
    }

    /// Subscribe to state changes with a callback.
    ///
    /// The callback will be called whenever state changes after an `emit()`.
    /// Callbacks should be lightweight and avoid blocking operations.
    ///
    /// # Main Thread Only
    ///
    /// Subscribers must be registered and called from the main thread only,
    /// as they may capture GTK widgets which are `!Send`.
    pub fn subscribe<F>(&self, callback: F)
    where
        F: Fn() + 'static,
    {
        self.subscribers.borrow_mut().push(Rc::new(callback));
    }

    /// Notify all subscribers of a state change.
    fn notify_subscribers(&self) {
        for callback in self.subscribers.borrow().iter() {
            callback();
        }
    }
}
