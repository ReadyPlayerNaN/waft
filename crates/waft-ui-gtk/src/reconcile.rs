//! Reconcilable trait for GTK widgets that support in-place property updates.
//!
//! When a daemon pushes updated widget state, we want to avoid tearing down and
//! recreating the entire GTK tree. `Reconcilable` allows compatible widgets to
//! apply property changes (active, value, icon, etc.) directly on the live GTK
//! objects, producing smooth UI transitions.

use waft_ipc::Widget as IpcWidget;

/// Outcome of a reconciliation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileOutcome {
    /// Properties were applied in-place — no GTK rebuild needed.
    Updated,
    /// The widget must be torn down and rebuilt (structural change).
    Recreate,
}

/// Widget that can update its GTK properties in-place from a new IPC description.
///
/// Implementors compare the old and new descriptions. If compatible (same action
/// callbacks, same structural shape), they apply property diffs and return
/// `Updated`. Otherwise they return `Recreate` without mutating any state.
pub trait Reconcilable {
    /// Try to reconcile the old description to the new one.
    ///
    /// **Contract**: MUST NOT mutate the widget if returning `Recreate`.
    fn try_reconcile(&self, old_desc: &IpcWidget, new_desc: &IpcWidget) -> ReconcileOutcome;
}
