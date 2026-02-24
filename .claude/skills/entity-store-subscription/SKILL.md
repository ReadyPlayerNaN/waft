---
name: entity-store-subscription
description: Pattern for subscribing to entity types in EntityStore with Reconciler-based widget management and initial reconciliation.
---

# EntityStore Subscription with Initial Reconciliation

## The Problem

`EntityStore::subscribe_type()` only fires callbacks when entities change, not on initial subscription. If `EntityUpdated` notifications arrive before subscriptions are registered, the UI never sees the cached data.

## The Pattern

Every smart container that subscribes to entity types must do TWO things:

1. **Subscribe to changes** via `subscribe_type()`
2. **Trigger initial reconciliation** via `gtk::glib::idle_add_local_once()`

## Template (Reconciler-Based)

```rust
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_ui_gtk::vdom::{Reconciler, VNode};
use waft_protocol::Urn;
use waft_protocol::entity::your_domain::{YourEntity, ENTITY_TYPE};

use crate::your_widget::{YourWidget, YourWidgetOutput, YourWidgetProps};

pub struct YourPage {
    pub root: gtk::Box,
}

struct YourPageState {
    reconciler: Reconciler,
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

        let reconciler = Reconciler::new(container);

        let state = Rc::new(RefCell::new(YourPageState {
            reconciler,
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

        state.reconciler.reconcile(
            entities.iter().map(|(urn, entity)| {
                let urn_key = urn.as_str().to_string();
                let urn_clone = urn.clone();
                let cb = action_callback.clone();

                VNode::with_output::<YourWidget>(
                    YourWidgetProps {
                        // ... map entity fields to props ...
                    },
                    move |output| {
                        let (action, params) = match output {
                            YourWidgetOutput::Toggle => ("toggle", serde_json::Value::Null),
                            // ... map other outputs to actions ...
                        };
                        cb(urn_clone.clone(), action.to_string(), params);
                    },
                )
                .key(urn_key)  // Stable key for diffing
            }),
        );
    }
}
```

The `Reconciler` automatically handles:
- **Adding** widgets for new keys
- **Updating** widgets when props change (skips if unchanged)
- **Removing** widgets when keys disappear

No manual `HashMap<String, Widget>`, no `seen` HashSet, no `to_remove` loop.

## Why `idle_add_local_once`?

- Defers execution until after current GTK event processing completes
- Ensures all subscription setup is complete before reading the store
- Prevents `RefCell` borrow conflicts (subscriptions may trigger during setup)

## Key Rules

1. Always subscribe BEFORE the idle_add initial reconciliation
2. The Reconciler handles create/update/remove via VNode keys
3. Use `.key(urn.as_str())` for stable entity identity
4. Sort entities before passing to reconciler if stable visual ordering matters

## Reference Implementations

- `crates/settings/src/pages/wired.rs` -- primary Reconciler example with two entity types
- `crates/settings/src/pages/bluetooth.rs` -- multi-entity-type with Reconciler
- `crates/settings/src/pages/wifi.rs` -- WiFi adapters + networks with Reconciler
- `crates/settings/src/pages/plugins.rs` -- simpler example using direct Component::build/update (no Reconciler)
