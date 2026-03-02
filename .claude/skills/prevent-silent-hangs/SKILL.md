---
name: prevent-silent-hangs
description: Use when writing async loops, channel consumers, background tasks, mutex locks, or child process spawning in waft. A silent failure in any of these makes the overlay permanently unresponsive with no log trace.
---

# Prevent Silent Hangs in Waft Daemons

## Overview

Waft is a long-running daemon. A silent failure in any async loop, channel consumer, or background task makes the overlay permanently unresponsive with no clue in the logs. These rules keep every failure path visible.

## Rules

### 1. Never Discard Results with `let _ =`

```rust
// BAD -- silent failure, invisible in logs
let _ = tx.send_blocking(value);
let _ = rt.block_on(server());

// GOOD
if let Err(e) = tx.send_blocking(value) {
    eprintln!("[ipc] failed to forward command: {e}");
}
match rt.block_on(server()) {
    Ok(()) => eprintln!("[ipc] server exited cleanly"),
    Err(e) => eprintln!("[ipc] server error: {e}"),
}
```

**Exception:** `let _ =` is acceptable for best-effort cleanup where the outcome genuinely doesn't matter (e.g. removing a stale socket file).

### 2. Log When Async Loops Exit

Every `while let Ok(...) = rx.recv().await` loop is a critical event pump. When the channel closes, the loop exits silently and the feature stops responding.

```rust
glib::spawn_future_local(async move {
    while let Ok(input) = rx.recv().await {
        handle(input);
    }
    warn!("[feature] receiver loop exited -- feature is now unresponsive");
});
```

### 3. Log When Background Tasks Exit

```rust
// BAD -- task exits silently
tokio::spawn(my_task(rx));

// GOOD
tokio::spawn(async move {
    if let Err(e) = my_task(rx).await {
        warn!("[feature] task error: {e}");
    }
    debug!("[feature] task stopped");
});
```

### 4. Break Send Loops When Nobody Is Listening

When a broadcast/channel sender fails, all receivers are gone. Continuing to loop wastes resources.

```rust
// BAD -- loops forever sending into the void
let _ = tx.send(msg);

// GOOD
if tx.send(msg).is_err() {
    break;
}
// after loop:
debug!("[feature] listener stopped");
```

### 5. Recover from Mutex Poison, Never Panic

A poisoned mutex means a thread panicked while holding the lock. Recover with `e.into_inner()`.

```rust
// BAD -- panics the app
let guard = mutex.lock().unwrap();

// GOOD
let guard = match mutex.lock() {
    Ok(g) => g,
    Err(e) => {
        warn!("[feature] mutex poisoned, recovering: {e}");
        e.into_inner()
    }
};
```

### 6. Reap Child Processes

Dropping a `std::process::Child` without calling `wait()` creates zombie processes.

```rust
// BAD -- creates zombie
Command::new("sh").arg("-c").arg(&cmd).spawn().ok();

// GOOD
match Command::new("sh").arg("-c").arg(&cmd).spawn() {
    Ok(child) => {
        std::thread::spawn(move || {
            let mut child = child;
            let _ = child.wait();
        });
    }
    Err(e) => error!("spawn failed: {e}"),
}
```

### 7. Log Before Panic in Bridge Code

When a bridge between runtimes (tokio-to-glib) uses `expect()`, the panic message may never reach logs.

```rust
// BAD -- panic message may be swallowed
rx.recv_async().await.expect("task panicked")

// GOOD
match rx.recv_async().await {
    Ok(val) => val,
    Err(e) => {
        error!("[runtime] task cancelled or panicked: {e}");
        panic!("task cancelled or panicked: {e}");
    }
}
```

### 8. Guard Against None in Late-Init Fields

When a field is set to `Some(...)` during initialization and accessed later, use `match` not `.unwrap()`.

```rust
// BAD
let handle = self.field.as_ref().unwrap().clone();

// GOOD
let handle = match self.field.as_ref() {
    Some(h) => h.clone(),
    None => {
        error!("[feature] field not initialized");
        return Ok(());
    }
};
```

## Checklist

- [ ] No `let _ =` on fallible operations (except cleanup)
- [ ] All `while let Ok(...)` loops log after exit
- [ ] All `tokio::spawn` calls wrapped with error logging
- [ ] Broadcast send failures `break` out of loops
- [ ] Mutex locks use `e.into_inner()` pattern instead of `.unwrap()`
- [ ] Child processes have a `std::thread::spawn` reaper
- [ ] Runtime bridge `expect()` calls log before panicking
- [ ] Late-init `Option` fields use `match` instead of `.unwrap()`
