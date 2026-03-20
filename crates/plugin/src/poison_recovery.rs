//! Mutex poison recovery for long-running plugin daemons.
//!
//! A poisoned mutex means a thread panicked while holding the lock. In a
//! long-running daemon, recovering with [`PoisonError::into_inner`] is
//! preferable to crashing the entire process.

use std::sync::{Mutex, MutexGuard};

/// Lock a [`Mutex`], recovering from poison by logging a warning and
/// returning the inner guard.
///
/// This avoids the repeated match-on-lock pattern found across plugins:
///
/// ```rust,ignore
/// let guard = match mutex.lock() {
///     Ok(g) => g,
///     Err(e) => {
///         warn!("mutex poisoned, recovering: {e}");
///         e.into_inner()
///     }
/// };
/// ```
pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(g) => g,
        Err(e) => {
            log::warn!("mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    }
}
