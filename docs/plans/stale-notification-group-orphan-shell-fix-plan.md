# Plan: Fix stale orphan notification group shells after burst dismissals

## Status

Oracle-passing, implementation-ready plan.

## Triggering evidence

User reproduction:

- send many notifications from the same app (e.g. `notify-send ... --app-name "zz"`)
- open overview
- open the grouped notifications for that app
- dismiss them one-by-one from overview
- `waft query notification` is empty
- but the `zz` group shell still remains visible in overview

This proves the canonical backend notification state is already empty while overview UI still shows stale state.

## Scope

This plan fixes the remaining stale group shell bug only.

It does **not**:
- change notification ownership
- change plugin/store/protocol semantics broadly
- redesign grouped notifications UI
- rework toast policy/config beyond ensuring presenter convergence

## Oracle-reviewed diagnosis summary

The strongest narrow seam is **not** the child group widget, and not the backend plugin.

The likely problem is at the overview-side cache/reconciliation trigger boundary:

- `EntityStore` only notifies subscribers on `EntityRemoved` when a cache entry was actually removed
- in burst dismiss/remove scenarios, later remove events can become no-ops at the cache layer
- if the final convergence-triggering notification callback is skipped, overview presenters may not reconcile to the final empty canonical state

That can leave:
- stale notification group shells in overview
- and potentially stale toast presenter state too

## Core diagnosis

Current `EntityStore` behavior:

- `handle_entity_removed()` does:
  - try `cache.remove(urn)`
  - only `notify_type(entity_type)` if the remove returned `Some(...)`

This is safe for deduplication, but risky for convergence.

In a burst dismiss scenario:
- presenters may mutate local structures over several callbacks
- a later redundant/remove-after-local-change event may still be the event needed to trigger final canonical reconciliation
- suppressing subscriber notification on no-op remove can leave UI stale even though canonical state is already empty

## Design principle

For entity-removal notifications, **canonical convergence matters more than local cache deduplication**.

That means:
- overview notification presenters should get a reconciliation pulse for notification removals even if the cache entry is already absent
- the fix should be as narrow as possible
- avoid making every entity type noisier unless needed

## Recommended fix seam

Primary seam:

- `crates/client/src/entity_store.rs`

The likely right fix is to make `EntityStore` still notify `notification` subscribers on remove events even when the cache entry is already absent, so grouped overview UI converges to the final empty canonical snapshot.

## Non-goals

- no plugin-side or daemon-side redesign
- no child-driven group shell deletion model
- no notification protocol changes
- no broad “always notify on every remove for every entity type” change unless justified by evidence

## Implementation plan

### Phase 1 — Add regression test at the store/reconciliation seam

Add a regression test that models the exact failure class:

1. subscribe to notification-type changes
2. populate notification entities
3. remove them in a burst / repeated-remove style sequence
4. ensure the subscriber still receives the final reconciliation-triggering notification even if a later remove is redundant at cache level

Two useful test shapes:

#### 1A. EntityStore notification test

In `crates/client/src/entity_store.rs` tests:

- add a test showing that `EntityRemoved(notification)` still triggers notification-type subscribers even when the cache entry is already absent, if that is the new contract for notification entities

This is the tightest seam for the actual suspected bug.

#### 1B. Overview GTK regression test

Keep or extend overview GTK regression coverage so the end symptom stays covered:

- many notifications in one app group
- dismiss/remove sequence to zero
- assert no stale group shell remains

The EntityStore test proves the seam; the overview GTK test proves the symptom.

### Phase 2 — Narrowly adjust `EntityStore` remove notification behavior

In `crates/client/src/entity_store.rs`:

Current:

- only notify on actual cache deletion

Target:

- for `notification` entity type, still notify subscribers on remove events even if the cache did not contain the URN

Recommended implementation shape:

- keep existing behavior for most entity types
- special-case the notification entity type, or add a narrowly documented helper/policy branch

Reason:
- this preserves the narrow fix seam
- avoids broad churn for unrelated entity types
- matches the concrete bug evidence

If during implementation it turns out that stale toast presenter state also depends on the same missing remove pulse, that is acceptable and desirable: both overview presenters should converge from the same notification-type subscriber pulse.

### Phase 3 — Revalidate overview notification-list convergence

With the new EntityStore remove-notify contract in place, verify that:

- `NotificationsComponent` parent reconciliation runs to final empty state
- stale group shells disappear
- empty placeholder/container visibility is correct

Primary file to inspect/validate:

- `crates/overview/src/components/notification_list.rs`

Do **not** move shell lifetime ownership into `NotificationGroup` unless the new store behavior proves insufficient.

### Phase 4 — Revalidate toast presenter convergence as a side effect

Because toast presentation also depends on notification remove events in overview’s event flow, confirm that the same fix does not regress toast cleanup and may improve stale toast convergence.

Relevant file to validate:

- `crates/overview/src/features/toasts/toast_manager.rs`

Do not broaden into additional toast redesign unless a failing test shows the stale-state bug persists independently.

## File-by-file expected touch map

Primary expected file:

- `crates/client/src/entity_store.rs`

Likely test/update files:

- `crates/overview/src/components/mod.rs`
- possibly `crates/overview/src/components/notification_list.rs` if regression tests need helper extraction

Potentially unchanged unless evidence requires it:

- `crates/overview/src/components/notification_group.rs`
- `crates/overview/src/features/toasts/toast_manager.rs`

## Validation strategy

### Focused commands

```bash
cargo build -p waft-client -p waft-overview
cargo test -p waft-client -p waft-overview
cargo clippy -p waft-client -p waft-overview --all-targets -- -D warnings
```

### Manual smoke checks

Re-run the exact user scenario:

1. send many notifications from same app name, e.g. `zz`
2. open overview
3. expand the `zz` group
4. dismiss notifications one-by-one
5. confirm:
   - `waft query notification` is empty
   - the `zz` group shell disappears completely

Also check:

- clear-all on grouped notifications
- toast presenter cleanup still works
- no obvious extra churn in unrelated entity subscribers

## Risks

### Risk 1 — over-notifying unrelated entity types

Mitigation:
- keep the contract change narrow to notification entities unless broader evidence appears

### Risk 2 — hiding a deeper missed-event bug

Mitigation:
- pair the EntityStore seam test with the overview symptom regression test
- if the bug persists after this seam fix, then revisit overview reconciliation itself

### Risk 3 — duplicate UI churn

Extra notification-type subscriber callbacks may cause more reconcile passes.

Mitigation:
- grouped notification reconciliation is already snapshot-driven and idempotent
- acceptable tradeoff for correctness

## Acceptance criteria

This fix is done when all are true:

1. if `waft query notification` is empty after burst dismissals, no app notification group shell remains in overview
2. a regression test exists at the EntityStore seam or equivalent proving notification-type subscribers still reconcile correctly on redundant/no-op remove cases
3. the existing or extended overview GTK regression test covers the stale-shell symptom
4. `cargo build/test/clippy -p waft-client -p waft-overview` pass
5. manual repro with many notifications from one app no longer leaves the orphan group shell

## Oracle sign-off summary

Oracle judged the revised plan implementation-ready.

Key oracle conclusions incorporated:
- do not push shell ownership into `NotificationGroup`
- do not broaden into plugin/protocol redesign
- treat this as a canonical-store-to-presenter convergence bug
- use `EntityStore` remove-notify semantics as the primary narrow fix seam
- keep the fix narrowly scoped, with notification-specific over-notify if needed for correctness
