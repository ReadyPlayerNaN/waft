# CLAUDE.md

See AGENTS.md for architecture overview, build commands, and project rules.

## Coding Rules: Prevent Silent Hangs

This app is a long-running daemon. A silent failure in any async loop, channel consumer, or background task makes the overlay permanently unresponsive with no clue in the logs. Follow these rules to keep every failure path visible.

### Never discard Results with `let _ =`

Silent `let _ = expr` on fallible operations hides the exact moment something breaks. Always log or act on the error.

```rust
// BAD — silent failure, invisible in logs
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

Exception: `let _ =` is acceptable for best-effort cleanup where the outcome genuinely doesn't matter (e.g. removing a stale socket file).

### Log when async loops exit

Every `while let Ok(...) = rx.recv().await` loop is a critical event pump. When the channel closes, the loop exits silently and the feature stops responding. Always add a log line after the loop.

```rust
glib::spawn_future_local(async move {
    while let Ok(input) = rx.recv().await {
        handle(input);
    }
    warn!("[feature] receiver loop exited — feature is now unresponsive");
});
```

### Log when background tasks exit

Wrap `tokio::spawn` calls so unexpected exits are visible.

```rust
// BAD — task exits silently
tokio::spawn(my_task(rx));

// GOOD
tokio::spawn(async move {
    if let Err(e) = my_task(rx).await {
        warn!("[feature] task error: {e}");
    }
    debug!("[feature] task stopped");
});
```

### Break send loops when nobody is listening

When a broadcast/channel sender fails, it means all receivers are gone. Continuing to loop wastes resources. Break out and log.

```rust
// BAD — loops forever sending into the void
let _ = tx.send(msg);

// GOOD
if tx.send(msg).is_err() {
    break;
}
// after loop:
debug!("[feature] listener stopped");
```

### Recover from mutex poison, never panic

A poisoned mutex means a thread panicked while holding the lock. In a long-running app, recovering with `e.into_inner()` is better than crashing the entire process.

```rust
// BAD — panics the app
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

### Reap child processes

Dropping a `std::process::Child` without calling `wait()` creates zombie processes. Spawn a thread to reap.

```rust
// BAD — creates zombie
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

### Log before panic in bridge code

When a bridge between runtimes (e.g. tokio-to-glib) uses `expect()`, the panic message may never reach logs. Log the error explicitly first.

```rust
// BAD — panic message may be swallowed
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

### Guard against None in late-init fields

When a field is set to `Some(...)` during `create_elements()` and accessed later, use `match` instead of `.unwrap()` to avoid a panic if initialization order changes.

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
