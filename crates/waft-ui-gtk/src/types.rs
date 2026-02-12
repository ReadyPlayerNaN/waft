// Re-export protocol types from waft-ipc
//
// All widget protocol types are now defined in the waft-ipc crate and
// re-exported here for backwards compatibility with existing code.

pub use waft_ipc::widget::*;

use std::rc::Rc;

/// Type alias for the action callback function.
pub type ActionCallback = Rc<dyn Fn(String, Action)>;
