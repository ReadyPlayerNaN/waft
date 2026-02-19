---
name: entity-store-subscription
description: Pattern for subscribing to entity types in EntityStore with initial reconciliation. Use when building smart container pages in waft-settings or any GTK component that consumes entity data from the daemon.
---

# EntityStore Subscription with Initial Reconciliation

## The Problem

`EntityStore::subscribe_type()` only fires callbacks when entities change, not on initial subscription. If `EntityUpdated` notifications arrive before subscriptions are registered, the UI never sees the cached data.

## The Pattern

Every smart container that subscribes to entity types must do TWO things:

1. **Subscribe to changes** via `subscribe_type()`
2. **Trigger initial reconciliation** via `gtk::glib::idle_add_local_once()`

## Template

```rust
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::your_domain::{YourEntity, ENTITY_TYPE};

pub struct YourPage {
    pub root: gtk::Box,
}

struct YourPageState {
    widgets: HashMap<String, YourWidget>,
    container: gtk::Box,
}

impl YourPage {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .build();
        root.append(&container);

        let state = Rc::new(RefCell::new(YourPageState {
            widgets: HashMap::new(),
            container,
        }));

        // 1. Subscribe to entity changes
        {
            let store = entity_store.clone();
            let state = state.clone();
            let cb = action_callback.clone();
            entity_store.subscribe_type(ENTITY_TYPE, move || {
                let entities: Vec<(Urn, YourEntity)> =
                    store.get_entities_typed(ENTITY_TYPE);
                Self::reconcile(&state, &entities, &cb);
            });
        }

        // 2. Trigger initial reconciliation with cached data
        {
            let state_clone = state.clone();
            let store_clone = entity_store.clone();
            let cb_clone = action_callback.clone();

            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, YourEntity)> =
                    store_clone.get_entities_typed(ENTITY_TYPE);
                if !entities.is_empty() {
                    log::debug!(
                        "[your-page] Initial reconciliation: {} entities",
                        entities.len()
                    );
                    Self::reconcile(&state_clone, &entities, &cb_clone);
                }
            });
        }

        Self { root }
    }

    fn reconcile(
        state: &Rc<RefCell<YourPageState>>,
        entities: &[(Urn, YourEntity)],
        action_callback: &EntityActionCallback,
    ) {
        let mut state = state.borrow_mut();
        let mut seen = std::collections::HashSet::new();

        for (urn, entity) in entities {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            if let Some(existing) = state.widgets.get(&urn_str) {
                existing.apply_props(/* ... */);
            } else {
                let widget = YourWidget::new(/* props */);
                // Connect output callback
                let urn_clone = urn.clone();
                let cb = action_callback.clone();
                widget.connect_output(move |output| {
                    let (action, params) = match output {
                        // Map output events to entity actions
                    };
                    cb(urn_clone.clone(), action.to_string(), params);
                });
                state.container.append(&widget.root);
                state.widgets.insert(urn_str, widget);
            }
        }

        // Remove widgets for entities no longer present
        let to_remove: Vec<String> = state
            .widgets
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some(widget) = state.widgets.remove(&key) {
                state.container.remove(&widget.root);
            }
        }
    }
}
```

## Why `idle_add_local_once`?

- Defers execution until after current GTK event processing completes
- Ensures all subscription setup is complete before reading the store
- Prevents `RefCell` borrow conflicts (subscriptions may trigger during setup)

## Key Rules

1. Always subscribe BEFORE the idle_add initial reconciliation
2. The reconcile function must handle both create and update paths
3. Sort entities for stable ordering (prevents visual jumps)
4. Always remove stale widgets when entities disappear

## Reference Implementations

- `crates/settings/src/pages/bluetooth.rs` -- subscribes to two entity types (adapters + devices)
- `crates/settings/src/pages/wifi.rs` -- same pattern with WiFi adapters + networks
- `crates/settings/src/pages/wired.rs` -- Ethernet adapters + connection profiles
- `crates/settings/src/pages/plugins.rs` -- clean single entity type example with empty state
