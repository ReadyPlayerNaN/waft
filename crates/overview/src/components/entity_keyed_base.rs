//! Generic entity-keyed container for managing a `HashMap<String, Entry>` of
//! GTK widgets that follow a common add/update/remove lifecycle.
//!
//! Several overview components (brightness sliders, audio sliders) share an
//! identical pattern: maintain a map of entries keyed by a string identifier,
//! reconcile against a set of desired keys on each entity-store notification,
//! remove stale entries from the container, and toggle container visibility.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::components::entity_keyed_base::{ContainerEntry, EntityKeyedContainer};
//!
//! struct MyEntry { widget: gtk::Label }
//!
//! impl ContainerEntry for MyEntry {
//!     fn widget(&self) -> gtk::Widget { self.widget.clone().upcast() }
//! }
//!
//! let base = EntityKeyedContainer::<MyEntry>::new(8);
//! let (container, entries) = base.refs();
//!
//! // Inside an entity-store subscription:
//! let mut map = entries.borrow_mut();
//! EntityKeyedContainer::reconcile(
//!     &container,
//!     &mut map,
//!     &desired_keys,
//!     |_key, entry| { /* update existing */ },
//!     |_key| { /* create new */ Some(MyEntry { .. }) },
//! );
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;

/// Trait for entries managed by [`EntityKeyedContainer`].
///
/// Each entry must provide access to its root GTK widget so the container
/// can append/remove it.
pub trait ContainerEntry {
    fn widget(&self) -> gtk::Widget;
}

/// A vertical `gtk::Box` paired with a `HashMap<String, E>` that tracks
/// keyed widget entries with automatic stale-entry cleanup and visibility
/// coordination.
pub struct EntityKeyedContainer<E> {
    container: gtk::Box,
    entries: Rc<RefCell<HashMap<String, E>>>,
}

impl<E: ContainerEntry> EntityKeyedContainer<E> {
    /// Create a new container with the given vertical spacing, initially invisible.
    pub fn new(spacing: i32) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(spacing)
            .visible(false)
            .build();
        Self {
            container,
            entries: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Returns the container widget.
    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }

    /// Returns clones of the container and entries map for use in
    /// subscription closures.
    pub fn refs(&self) -> (gtk::Box, Rc<RefCell<HashMap<String, E>>>) {
        (self.container.clone(), self.entries.clone())
    }

    /// Reconcile the entry map against a set of desired keys.
    ///
    /// 1. Removes entries whose keys are absent from `desired_keys`.
    /// 2. For each desired key, calls `update` if the entry exists, or
    ///    `create` if it does not. `create` returns `Option<E>` so callers
    ///    can skip creation when conditions aren't met.
    /// 3. Sets the container visible when at least one entry remains.
    pub fn reconcile(
        container: &gtk::Box,
        entries: &mut HashMap<String, E>,
        desired_keys: &[String],
        mut update: impl FnMut(&str, &mut E),
        mut create: impl FnMut(&str) -> Option<E>,
    ) {
        // Remove stale entries
        let stale: Vec<String> = entries
            .keys()
            .filter(|k| !desired_keys.contains(k))
            .cloned()
            .collect();
        for key in stale {
            if let Some(entry) = entries.remove(&key) {
                container.remove(&entry.widget());
            }
        }

        // Update existing or create new
        for key in desired_keys {
            if let Some(entry) = entries.get_mut(key.as_str()) {
                update(key, entry);
            } else if let Some(new_entry) = create(key) {
                container.append(&new_entry.widget());
                entries.insert(key.clone(), new_entry);
            }
        }

        container.set_visible(!entries.is_empty());
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    struct TestEntry {
        label: gtk::Label,
    }

    impl TestEntry {
        fn new(text: &str) -> Self {
            Self {
                label: gtk::Label::new(Some(text)),
            }
        }

        fn text(&self) -> String {
            self.label.text().to_string()
        }
    }

    impl ContainerEntry for TestEntry {
        fn widget(&self) -> gtk::Widget {
            self.label.clone().upcast()
        }
    }

    fn child_count(container: &gtk::Box) -> u32 {
        let mut count = 0u32;
        let mut child = container.first_child();
        while let Some(c) = child {
            count += 1;
            child = c.next_sibling();
        }
        count
    }

    /// Run all entity_keyed_base GTK tests from the single GTK test entry point.
    pub(crate) fn run_all() {
        test_new_container_invisible_and_empty();
        test_reconcile_creates_entries();
        test_reconcile_updates_existing();
        test_reconcile_removes_stale();
        test_reconcile_visibility_empty_after_removal();
        test_reconcile_create_returns_none_skips();
        test_reconcile_mixed_add_update_remove();
    }

    fn test_new_container_invisible_and_empty() {
        let base = EntityKeyedContainer::<TestEntry>::new(8);
        assert!(!base.widget().is_visible(), "container should start invisible");
        let (container, _) = base.refs();
        assert_eq!(child_count(&container), 0);
    }

    fn test_reconcile_creates_entries() {
        let base = EntityKeyedContainer::<TestEntry>::new(8);
        let (container, entries) = base.refs();
        let desired = vec!["a".to_string(), "b".to_string()];

        EntityKeyedContainer::reconcile(
            &container,
            &mut entries.borrow_mut(),
            &desired,
            |_, _| {},
            |key| Some(TestEntry::new(key)),
        );

        assert_eq!(child_count(&container), 2);
        assert!(base.widget().is_visible());
        assert_eq!(entries.borrow().len(), 2);
    }

    fn test_reconcile_updates_existing() {
        let base = EntityKeyedContainer::<TestEntry>::new(8);
        let (container, entries) = base.refs();
        let desired = vec!["a".to_string()];

        // Create
        EntityKeyedContainer::reconcile(
            &container,
            &mut entries.borrow_mut(),
            &desired,
            |_, _| {},
            |key| Some(TestEntry::new(key)),
        );
        assert_eq!(entries.borrow()["a"].text(), "a");

        // Update — change the label text
        EntityKeyedContainer::reconcile(
            &container,
            &mut entries.borrow_mut(),
            &desired,
            |_key, entry| {
                entry.label.set_text("updated");
            },
            |_| unreachable!("should not create"),
        );
        assert_eq!(child_count(&container), 1, "update must not add widgets");
        assert_eq!(entries.borrow()["a"].text(), "updated");
    }

    fn test_reconcile_removes_stale() {
        let base = EntityKeyedContainer::<TestEntry>::new(8);
        let (container, entries) = base.refs();

        // Create two entries
        EntityKeyedContainer::reconcile(
            &container,
            &mut entries.borrow_mut(),
            &vec!["a".to_string(), "b".to_string()],
            |_, _| {},
            |key| Some(TestEntry::new(key)),
        );
        assert_eq!(child_count(&container), 2);

        // Reconcile with only "b" — "a" should be removed
        EntityKeyedContainer::reconcile(
            &container,
            &mut entries.borrow_mut(),
            &vec!["b".to_string()],
            |_, _| {},
            |_| unreachable!("should not create"),
        );
        assert_eq!(child_count(&container), 1);
        assert!(!entries.borrow().contains_key("a"));
        assert!(entries.borrow().contains_key("b"));
    }

    fn test_reconcile_visibility_empty_after_removal() {
        let base = EntityKeyedContainer::<TestEntry>::new(8);
        let (container, entries) = base.refs();

        // Create one entry
        EntityKeyedContainer::reconcile(
            &container,
            &mut entries.borrow_mut(),
            &vec!["a".to_string()],
            |_, _| {},
            |key| Some(TestEntry::new(key)),
        );
        assert!(base.widget().is_visible());

        // Remove all
        EntityKeyedContainer::reconcile(
            &container,
            &mut entries.borrow_mut(),
            &vec![],
            |_, _| {},
            |_| unreachable!("should not create"),
        );
        assert!(!base.widget().is_visible(), "should be invisible when empty");
        assert_eq!(child_count(&container), 0);
    }

    fn test_reconcile_create_returns_none_skips() {
        let base = EntityKeyedContainer::<TestEntry>::new(8);
        let (container, entries) = base.refs();

        EntityKeyedContainer::reconcile(
            &container,
            &mut entries.borrow_mut(),
            &vec!["a".to_string()],
            |_, _| {},
            |_| None, // skip creation
        );

        assert_eq!(child_count(&container), 0);
        assert!(!base.widget().is_visible());
    }

    fn test_reconcile_mixed_add_update_remove() {
        let base = EntityKeyedContainer::<TestEntry>::new(8);
        let (container, entries) = base.refs();

        // Start with a, b
        EntityKeyedContainer::reconcile(
            &container,
            &mut entries.borrow_mut(),
            &vec!["a".to_string(), "b".to_string()],
            |_, _| {},
            |key| Some(TestEntry::new(key)),
        );
        assert_eq!(child_count(&container), 2);

        // Now desired = b, c — removes a, keeps b, adds c
        let mut updated_keys = vec![];
        EntityKeyedContainer::reconcile(
            &container,
            &mut entries.borrow_mut(),
            &vec!["b".to_string(), "c".to_string()],
            |key, _| {
                updated_keys.push(key.to_string());
            },
            |key| Some(TestEntry::new(key)),
        );

        assert_eq!(child_count(&container), 2);
        assert!(!entries.borrow().contains_key("a"), "a should be removed");
        assert!(entries.borrow().contains_key("b"), "b should remain");
        assert!(entries.borrow().contains_key("c"), "c should be created");
        assert_eq!(updated_keys, vec!["b"]);
    }
}
