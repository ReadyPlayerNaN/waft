# Night Light first-click diagnostic

## Summary

The most likely issue is not that the first click is ignored. Instead, the first click starts a relatively slow async toggle operation, but the overview feature toggle does not show any pending or busy state. That makes the control appear non-responsive, and repeated rapid clicks can queue overlapping toggle actions that race with each other.

## Main UI-side issue

Relevant file:

- `crates/overview/src/ui/feature_toggles/simple_toggle.rs`

Observed behavior in code:

- the generic simple feature toggle is created with `busy: false`
- click handling always dispatches `"toggle"`
- there is no optimistic state update
- there is no temporary disable / debounce / action-in-flight guard
- there is no visual pending indicator while the backend action is still running

Effect:

- on the first click, the action may be running correctly
- but the toggle can still look unchanged for a while
- the user naturally clicks again

## Why Night Light is especially vulnerable

Relevant file:

- `plugins/sunsetr/bin/waft-sunsetr-daemon.rs`

When toggling on:

- `handle_action("toggle")` checks cached `state.active`
- if inactive, it calls `ipc_start()`
- then it waits in `refresh_after_start()`

`refresh_after_start()` is not instant:

- it repeatedly queries `sunsetr` status
- it retries with backoff sleeps
- it only completes once `sunsetr` is considered ready

So the first click can legitimately take noticeable time before the entity update returns to the UI.

## Why rapid repeated clicking can make it start working

Relevant file:

- `crates/plugin/src/runtime.rs`

Plugin actions are handled concurrently:

- each `TriggerAction` is spawned into its own async task

That means repeated clicks can produce overlapping `toggle` actions while the first action is still in progress.

Relevant plugin logic:

- `plugins/sunsetr/bin/waft-sunsetr-daemon.rs`

The sunsetr plugin decides whether to start or stop based on the current cached `state.active` value at the start of each action.

Implication:

- click 1 may start the slow enable flow
- before state is updated visibly, click 2 may also observe stale state
- multiple toggle actions can overlap and race
- eventually one action completes and the UI begins reflecting the new state

This matches the observed symptom that multiple rapid clicks can appear to “unstick” the toggle.

## Secondary concern

Relevant file:

- `plugins/sunsetr/bin/waft-sunsetr-daemon.rs`

The follow task (`sunsetr S --json --follow`) is spawned once at plugin startup.

Potential consequence:

- if the follow subprocess exits when sunsetr is stopped
- or if sunsetr was not running when the plugin started
- later passive updates may be less reliable than expected

This does not appear to be the primary cause of the first-click problem, but it could contribute to overall flakiness.

## Conclusion

This looks primarily like an action orchestration / UX problem:

1. first click triggers a real backend action
2. backend action can take noticeable time
3. UI shows no pending/busy state
4. user clicks repeatedly
5. overlapping toggle actions race against cached plugin state
6. behavior appears flaky or “does nothing on first click”

## Likely fix direction

Do not implement yet, but likely areas to address are:

1. add pending/busy state or temporary disable behavior to `SimpleToggle`
2. prevent overlapping night-light toggle actions while one is already in flight
3. consider whether the sunsetr follow/status refresh lifecycle needs to be made more robust across stop/start cycles
