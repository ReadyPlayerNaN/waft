//! Mutex lock with automatic poison recovery.
//!
//! All waft plugins use `std::sync::Mutex` for shared state between the plugin
//! struct and background monitoring tasks. Because the daemon is long-running,
//! poisoned mutexes should be recovered rather than panicking the process.
//!
//! This trait eliminates the repetitive lock-recover pattern that otherwise
//! appears in every plugin.

use std::sync::{Mutex, MutexGuard};

/// Extension trait for `std::sync::Mutex` that recovers from poison on lock.
pub trait StateLocker<T> {
    /// Lock the mutex, recovering from poison if a thread panicked while
    /// holding the lock. Logs a warning on poison recovery.
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> StateLocker<T> for Mutex<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        match self.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn lock_or_recover_normal() {
        let m = Mutex::new(42);
        assert_eq!(*m.lock_or_recover(), 42);
    }

    #[test]
    fn lock_or_recover_after_poison() {
        let m = Arc::new(Mutex::new(42));
        let m2 = m.clone();
        let _ = std::thread::spawn(move || {
            let _g = m2.lock().unwrap();
            panic!("intentional poison");
        })
        .join();

        // Mutex is now poisoned
        assert!(m.lock().is_err());
        // lock_or_recover should still work
        assert_eq!(*m.lock_or_recover(), 42);
    }
}
