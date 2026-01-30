---
name: async-runtime-bridge
description: Bridge async code between glib and tokio runtimes. Use when implementing any async code that runs inside glib contexts, D-Bus handlers, or signal callbacks.
---

# Async Runtime Bridge: glib + tokio

## The Problem

When awaiting tokio-dependent futures (zbus, reqwest, tokio::process, etc.) from inside `glib::spawn_future_local`, glib's poll loop doesn't integrate with tokio's I/O driver. This causes:
- **Symptom**: 100% CPU usage, app becomes unresponsive
- **Cause**: glib busy-polls with zero-timeout `ppoll()` calls

## The Solution: spawn_on_tokio()

Use `crate::runtime::spawn_on_tokio()` to run tokio-dependent work on the tokio runtime.

### Pattern: WRONG (causes CPU spin)
```rust
glib::spawn_future_local(async move {
    let result = zbus_call().await;  // tokio-dependent!
    // ...
});
```

### Pattern: CORRECT
```rust
glib::spawn_future_local(async move {
    let result = crate::runtime::spawn_on_tokio(async move {
        zbus_call().await
    }).await;
    // ...
});
```

## When to Use spawn_on_tokio()

**Always use it for**:
- zbus D-Bus calls
- reqwest HTTP requests
- tokio::process operations
- Any async code using tokio I/O

**Not needed for**:
- Pure computation
- glib-native async operations
- flume channel receives (already executor-agnostic)

## Checklist Before Committing Async Code

- [ ] Identify all `glib::spawn_future_local` blocks
- [ ] Check if any `.await` inside uses tokio-dependent libraries
- [ ] Wrap tokio-dependent awaits with `spawn_on_tokio()`
- [ ] Test with CPU monitoring (`htop`) to verify no busy-wait
