pub mod dbus;
pub mod store;
pub mod menu_state;

use std::cell::RefCell;
use std::rc::Rc;

/// Type alias for optional callback functions with a parameter.
///
/// This pattern is used throughout the codebase for widget output callbacks.
/// Example: `Callback<FeatureToggleOutput>` for a callback that receives toggle events.
pub type Callback<T> = Rc<RefCell<Option<Box<dyn Fn(T)>>>>;

/// Type alias for optional callback functions without parameters.
///
/// Used for simple event callbacks that don't pass any data.
pub type VoidCallback = Rc<RefCell<Option<Box<dyn Fn()>>>>;

// Re-export commonly used types from sub-crates
pub use waft_config;
