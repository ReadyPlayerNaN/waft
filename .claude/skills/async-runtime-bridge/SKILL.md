---
name: async-runtime-bridge
description: Safety patterns for async callbacks, D-Bus signal handlers, and glib/tokio bridging. Use when writing closures that run in signal callbacks, glib spawn contexts, or any code mixing locks with async operations.
---

# Async Callback Safety: glib + tokio + Locks

## Overview

This skill covers two critical patterns that cause application freezes:
1. **Runtime mismatch**: tokio-dependent code in glib context → CPU spin
2. **Lock ordering violations**: holding read lock while acquiring write lock → deadlock

---

## Pattern 1: glib/tokio Runtime Bridge

### The Problem

When awaiting tokio-dependent futures (zbus, reqwest, tokio::process) from `glib::spawn_future_local`, glib's poll loop doesn't integrate with tokio's I/O driver.

- **Symptom**: 100% CPU usage, app becomes unresponsive
- **Cause**: glib busy-polls with zero-timeout `ppoll()` calls

### The Solution: spawn_on_tokio()

Use `crate::runtime::spawn_on_tokio()` for tokio-dependent work.

```rust
// WRONG - causes CPU spin
glib::spawn_future_local(async move {
    let result = zbus_call().await;  // tokio-dependent!
});

// CORRECT
glib::spawn_future_local(async move {
    let result = crate::runtime::spawn_on_tokio(async move {
        zbus_call().await
    }).await;
});
```

### When to Use spawn_on_tokio()

**Always use for**: zbus D-Bus calls, reqwest HTTP, tokio::process, any tokio I/O

**Not needed for**: pure computation, glib-native async, flume channels

---

## Pattern 2: RwLock Deadlock in Callbacks

### The Problem

**CRITICAL**: RwLock does NOT allow upgrading read locks to write locks. Attempting this causes **immediate deadlock**.

```rust
// DEADLOCK - thread blocks forever
dbus::subscribe_signal(move |event| {
    let state = store.get_state();  // Takes READ lock, holds RwLockReadGuard
    if state.items.contains_key(&id) {
        store.emit(Op::Update(id));  // Tries to take WRITE lock = DEADLOCK
    }
});
```

**Stack trace signature**:
```
#1 std::sys::sync::rwlock::futex::RwLock::write_contended
#2 RwLock<...>::write
#3 PluginStore<...>::emit
```

### The Solution: Release Before Write

**Always release read locks before acquiring write locks**:

```rust
// CORRECT - release read lock before emit
dbus::subscribe_signal(move |event| {
    // Check condition, release lock immediately via scoped block
    let should_update = {
        let state = store.get_state();
        state.items.contains_key(&id)
    }; // READ lock released here

    if should_update {
        store.emit(Op::Update(id));  // Now safe to take WRITE lock

        // Get fresh state after write if needed
        let state = store.get_state();
        if let Some(item) = state.items.get(&id) {
            widget.sync_state(item);
        }
    }
});
```

### Key Rules

1. **Never hold a read guard across an emit() call**
2. **Use scoped blocks `{ }` to explicitly release guards**
3. **After emit(), get fresh state** - the old guard is stale anyway
4. **Watch for implicit guards** - `if let Some(x) = store.get_state().items.get(&id)` holds the lock!

---

## Combined Pattern: D-Bus Signal Callbacks

D-Bus signal handlers often need both patterns:

```rust
glib::spawn_future_local(async move {
    if let Err(e) = dbus::subscribe_state_changed(dbus.clone(), move |path, new_state| {
        // Pattern 2: Check with scoped read lock
        let is_tracked = {
            let state = store.get_state();
            state.adapters.contains_key(&path)
        }; // Lock released

        if is_tracked {
            // Safe to emit now
            store.emit(Op::SetState(path.clone(), new_state));

            // Update UI with fresh state
            if let Some(widget) = widgets.borrow().get(&path) {
                let state = store.get_state();
                if let Some(adapter) = state.adapters.get(&path) {
                    widget.sync_state(adapter);
                }
            }
        }
    }).await {
        error!("Failed to subscribe: {}", e);
    }
});
```

---

## Checklist Before Committing Callback Code

### Async Runtime
- [ ] Identify all `glib::spawn_future_local` blocks
- [ ] Check if any `.await` inside uses tokio-dependent libraries
- [ ] Wrap tokio-dependent awaits with `spawn_on_tokio()`
- [ ] Test with CPU monitoring (`htop`) to verify no busy-wait

### Lock Ordering
- [ ] Find all closures/callbacks that access stores or RwLock-protected state
- [ ] Verify NO `emit()`, `write()`, or write-lock calls while holding read guards
- [ ] Use scoped blocks to explicitly release read guards before writes
- [ ] After `emit()`, get fresh state - don't rely on pre-emit values

---

## Debugging Symptoms

| Symptom | Likely Cause | Check |
|---------|--------------|-------|
| 100% CPU, unresponsive | Runtime mismatch | Look for bare `.await` in glib context |
| Complete freeze, 0% CPU | RwLock deadlock | Check `get_state()` followed by `emit()` |
| Intermittent hangs | Lock contention | Check lock scope in signal callbacks |
