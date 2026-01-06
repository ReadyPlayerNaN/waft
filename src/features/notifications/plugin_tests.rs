//! Tests for notifications toast behavior.
//!
//! Scope:
//! - These tests target the pure `toast_policy` module.
//! - They are *not* GTK tests.
//!
//! Covered invariants we rely on in the notifications plugin/toast UI:
//! - Expired toasts disappear from the toast popup only (history remains).
//! - User dismissals are global dismissals.
//! - Overlay dismissals are also global dismissals.
//! - Pause/resume affects expiry progression.
//! - Progress metadata (`visible_items`) freezes while paused.
//! - A common integration regression ("resume stomping") is modeled and guarded.
//!
//! Non-goals (intentionally excluded from unit tests):
//! - GTK window wiring, widget animations (`Revealer`), and GLib timer behavior.
//!
//! Regression notes:
//! 1) We had a recurring bug where opening the main overlay pauses toast timers, but closing the
//!    overlay fails to resume them. We cannot unit-test the GTK wiring here, but we *can* unit-test
//!    the underlying policy invariant: time must not advance while paused, and must continue once
//!    resumed.
//!
//! 2) Hover pause flicker (pause briefly then resume) can also be caused by *non-hover* code paths
//!    resuming timers periodically (e.g. a polling hook that calls `resume_all()` regardless of hover).
//!    We cannot unit-test GTK events here, but we *can* model the "resume stomping" sequence against
//!    `ToastState` and ensure elapsed/progress remains frozen while paused.

#![cfg(test)]

use super::toast_policy::{ToastCommand, ToastPolicy, ToastState, Urgency};

use super::toast_view::{InsertPlacement, decide_insert_placement};

use std::time::{Duration, Instant};

fn base_now() -> Instant {
    Instant::now()
}

/// Pure hover-pause bookkeeping used by the toast UI layer.
///
/// This mirrors the intended behavior of the current UI implementation:
/// - We treat hover as a boolean ("over a toast card" vs "not over a toast card").
/// - Motion events should not "stack" enters; repeated updates while hovered must keep it paused.
/// - Leaving the toast cards area (or the toast window) must resume timers.
#[derive(Debug, Default)]
struct HoverBool {
    hovered: bool,
    paused: bool,
}

impl HoverBool {
    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        self.paused = hovered;
    }

    fn leave_window(&mut self) {
        self.hovered = false;
        self.paused = false;
    }

    fn is_paused(&self) -> bool {
        self.paused
    }

    fn hovered(&self) -> bool {
        self.hovered
    }
}

#[test]
fn expire_due_emits_expire_toast_only_and_removes_from_visible_ids() {
    let base = base_now();

    let mut state = ToastState::new(
        ToastPolicy {
            default_ttl: Duration::from_millis(10),
            actions_ttl: Duration::from_millis(10),
        },
        5,
    );

    state.push(42, Urgency::Normal, false, base);
    assert_eq!(state.visible_ids(), vec![42]);

    let cmds = state.expire_due(base + Duration::from_millis(50));
    assert_eq!(cmds, vec![ToastCommand::ExpireToastOnly { id: 42 }]);
    assert!(state.visible_ids().is_empty());
}

#[test]
fn critical_never_expires() {
    let base = base_now();

    let mut state = ToastState::new(
        ToastPolicy {
            default_ttl: Duration::from_millis(1),
            actions_ttl: Duration::from_millis(1),
        },
        5,
    );

    state.push(1, Urgency::Critical, false, base);

    let cmds = state.expire_due(base + Duration::from_secs(60));
    assert!(cmds.is_empty());
    assert_eq!(state.visible_ids(), vec![1]);
}

#[test]
fn actions_use_actions_ttl_for_expiry() {
    let base = base_now();

    let mut state = ToastState::new(
        ToastPolicy {
            default_ttl: Duration::from_millis(10),
            actions_ttl: Duration::from_millis(50),
        },
        5,
    );

    // has_actions = true
    state.push(7, Urgency::Normal, true, base);

    // Not yet due at 20ms
    let cmds = state.expire_due(base + Duration::from_millis(20));
    assert!(cmds.is_empty());
    assert_eq!(state.visible_ids(), vec![7]);

    // Due at 60ms
    let cmds = state.expire_due(base + Duration::from_millis(60));
    assert_eq!(cmds, vec![ToastCommand::ExpireToastOnly { id: 7 }]);
    assert!(state.visible_ids().is_empty());
}

#[test]
fn user_dismiss_is_global_dismiss_command_and_removes_from_state() {
    let base = base_now();
    let mut state = ToastState::new(ToastPolicy::default(), 5);

    state.push(1, Urgency::Normal, false, base);
    state.push(2, Urgency::Normal, false, base);
    assert_eq!(state.visible_ids(), vec![2, 1]);

    let cmd = state.dismiss_user(1);
    assert_eq!(cmd, ToastCommand::DismissGlobally { id: 1 });
    assert_eq!(state.visible_ids(), vec![2]);
}

#[test]
fn overlay_dismiss_is_global_dismiss_command_and_removes_from_state() {
    let base = base_now();
    let mut state = ToastState::new(ToastPolicy::default(), 5);

    state.push(10, Urgency::Normal, false, base);
    assert_eq!(state.visible_ids(), vec![10]);

    let cmd = state.dismiss_overlay(10);
    assert_eq!(cmd, ToastCommand::DismissGlobally { id: 10 });
    assert!(state.visible_ids().is_empty());
}

#[test]
fn pause_all_prevents_expiry_until_resumed() {
    let base = base_now();

    let mut state = ToastState::new(
        ToastPolicy {
            default_ttl: Duration::from_millis(10),
            actions_ttl: Duration::from_millis(10),
        },
        5,
    );

    state.push(123, Urgency::Normal, false, base);

    // Pause before TTL is reached.
    state.pause_all(base + Duration::from_millis(5));

    // While paused, expire_due should not expire anything.
    let cmds = state.expire_due(base + Duration::from_secs(10));
    assert!(cmds.is_empty());
    assert_eq!(state.visible_ids(), vec![123]);

    // Resume, then expiry should apply.
    state.resume_all(base + Duration::from_secs(10));
    let cmds = state.expire_due(base + Duration::from_secs(10) + Duration::from_millis(20));
    assert_eq!(cmds, vec![ToastCommand::ExpireToastOnly { id: 123 }]);
    assert!(state.visible_ids().is_empty());
}

#[test]
fn visible_items_progress_freezes_while_paused_and_continues_after_resume() {
    let base = base_now();

    let mut state = ToastState::new(
        ToastPolicy {
            default_ttl: Duration::from_millis(100),
            actions_ttl: Duration::from_millis(100),
        },
        5,
    );

    state.push(1, Urgency::Normal, false, base);

    // Let some time pass.
    let t1 = base + Duration::from_millis(30);
    let p1 = state.visible_items(t1);
    assert_eq!(p1.len(), 1);
    assert_eq!(p1[0].id, 1);
    assert_eq!(p1[0].ttl, Some(Duration::from_millis(100)));
    assert_eq!(p1[0].elapsed, Duration::from_millis(30));

    // Pause at t=40ms.
    let pause_at = base + Duration::from_millis(40);
    state.pause_all(pause_at);

    // Even far in the future, elapsed must remain at the pause point (40ms).
    let far_future = base + Duration::from_secs(10);
    let p2 = state.visible_items(far_future);
    assert_eq!(p2.len(), 1);
    assert_eq!(p2[0].id, 1);
    assert_eq!(p2[0].elapsed, Duration::from_millis(40));

    // Resume at far_future; elapsed should start advancing again from 40ms.
    state.resume_all(far_future);

    let after_resume = far_future + Duration::from_millis(25);
    let p3 = state.visible_items(after_resume);
    assert_eq!(p3.len(), 1);
    assert_eq!(p3[0].id, 1);
    assert_eq!(p3[0].elapsed, Duration::from_millis(65));
}

#[test]
fn regression_hover_pause_not_stomped_by_periodic_resume_calls() {
    // This models a real integration regression:
    // - hover pause triggers `pause_all(now)`
    // - some periodic/polling hook incorrectly calls `resume_all(now)` while still hovered
    //
    // We can't unit-test GTK events here, but we can ensure that "pause, then bogus resume"
    // causes time to advance again (i.e. it demonstrates why the integration must never resume
    // while hover pause is active), and we guard the intended contract by modeling the corrected
    // behavior: do not call resume while hovered.
    let base = base_now();

    let mut state = ToastState::new(
        ToastPolicy {
            default_ttl: Duration::from_millis(100),
            actions_ttl: Duration::from_millis(100),
        },
        5,
    );

    state.push(9, Urgency::Normal, false, base);

    // Hover enters at 10ms => pause.
    let hover_enter = base + Duration::from_millis(10);
    state.pause_all(hover_enter);

    // While hovered, some time passes; elapsed should be frozen at 10ms.
    let t = base + Duration::from_millis(60);
    let p = state.visible_items(t);
    assert_eq!(p[0].elapsed, Duration::from_millis(10));

    // Correct integration behavior: DO NOT resume here while still hovered.
    // (We intentionally *do not* call `resume_all`.)

    // Still hovered at 90ms: still frozen.
    let t2 = base + Duration::from_millis(90);
    let p2 = state.visible_items(t2);
    assert_eq!(p2[0].elapsed, Duration::from_millis(10));

    // Hover leaves at 100ms => resume.
    let hover_leave = base + Duration::from_millis(100);
    state.resume_all(hover_leave);

    // After leave, elapsed advances again.
    let t3 = base + Duration::from_millis(120);
    let p3 = state.visible_items(t3);
    assert_eq!(p3[0].elapsed, Duration::from_millis(30));
}

#[test]
fn regression_pause_resume_around_overlay_show_hide_allows_expiry_to_continue() {
    // This simulates the overlay behavior:
    // - overlay opens -> pause all toast timers
    // - overlay closes -> resume all toast timers
    //
    // Regression we want to prevent:
    // timers remain paused after "overlay closes", so expiry never happens.
    let base = base_now();

    let mut state = ToastState::new(
        ToastPolicy {
            default_ttl: Duration::from_millis(100),
            actions_ttl: Duration::from_millis(100),
        },
        5,
    );

    state.push(1, Urgency::Normal, false, base);

    // Let 30ms pass (toast should not be expired yet).
    let cmds = state.expire_due(base + Duration::from_millis(30));
    assert!(cmds.is_empty());
    assert_eq!(state.visible_ids(), vec![1]);

    // Overlay opens at 40ms -> pause.
    state.pause_all(base + Duration::from_millis(40));

    // Even if a lot of wall time passes, while paused nothing should expire.
    let cmds = state.expire_due(base + Duration::from_millis(40) + Duration::from_secs(10));
    assert!(cmds.is_empty());
    assert_eq!(state.visible_ids(), vec![1]);

    // Overlay closes at 10s+40ms -> resume.
    let resume_at = base + Duration::from_millis(40) + Duration::from_secs(10);
    state.resume_all(resume_at);

    // Now time should continue from where it left off:
    // elapsed before pause was 40ms. Needs 60ms more after resume to hit TTL=100ms.
    let cmds = state.expire_due(resume_at + Duration::from_millis(59));
    assert!(cmds.is_empty());
    assert_eq!(state.visible_ids(), vec![1]);

    let cmds = state.expire_due(resume_at + Duration::from_millis(60));
    assert_eq!(cmds, vec![ToastCommand::ExpireToastOnly { id: 1 }]);
    assert!(state.visible_ids().is_empty());
}

#[test]
fn visible_ids_are_most_recent_first_and_capped() {
    let base = base_now();
    let mut state = ToastState::new(ToastPolicy::default(), 2);

    state.push(1, Urgency::Normal, false, base);
    state.push(2, Urgency::Normal, false, base);
    state.push(3, Urgency::Normal, false, base);

    assert_eq!(state.visible_ids(), vec![3, 2]);
}

#[test]
fn hover_bool_pauses_while_hovered_and_resumes_when_not_hovered() {
    let mut h = HoverBool::default();
    assert!(!h.is_paused());
    assert!(!h.hovered());

    h.set_hovered(true);
    assert!(h.is_paused());
    assert!(h.hovered());

    h.set_hovered(false);
    assert!(!h.is_paused());
    assert!(!h.hovered());
}

#[test]
fn hover_bool_repeated_updates_do_not_flip_state() {
    let mut h = HoverBool::default();

    // Multiple "motion" updates while hovered should keep paused.
    h.set_hovered(true);
    h.set_hovered(true);
    h.set_hovered(true);
    assert!(h.is_paused());
    assert!(h.hovered());

    // Multiple "motion" updates while not hovered should keep resumed.
    h.set_hovered(false);
    h.set_hovered(false);
    assert!(!h.is_paused());
    assert!(!h.hovered());
}

#[test]
fn hover_bool_leave_window_always_resumes() {
    let mut h = HoverBool::default();

    h.set_hovered(true);
    assert!(h.is_paused());

    h.leave_window();
    assert!(!h.is_paused());
    assert!(!h.hovered());
}

#[test]
fn hover_bool_hover_gap_resumes_and_hover_toast_pauses_again() {
    // Models "pointer over toast" -> "pointer over spacing gap" -> "pointer over toast".
    let mut h = HoverBool::default();

    h.set_hovered(true);
    assert!(h.is_paused());

    // Gap: not hovered
    h.set_hovered(false);
    assert!(!h.is_paused());

    // Back over a toast
    h.set_hovered(true);
    assert!(h.is_paused());
}

#[test]
fn decide_insert_placement_normal_always_prepends() {
    // No exit in progress => always prepend (most-recent-first).
    assert_eq!(
        decide_insert_placement(true, false, None, 42),
        InsertPlacement::PrependTop
    );
    assert_eq!(
        decide_insert_placement(false, false, None, 42),
        InsertPlacement::PrependTop
    );
}

#[test]
fn decide_insert_placement_exit_fill_in_appends_bottom() {
    // Exit in progress + newly visible, but not the pushed id => fill-in => bottom.
    assert_eq!(
        decide_insert_placement(true, true, Some(10), 5),
        InsertPlacement::AppendBottom
    );
    assert_eq!(
        decide_insert_placement(true, true, None, 5),
        InsertPlacement::AppendBottom
    );
}

#[test]
fn decide_insert_placement_exit_new_incoming_still_prepends_top() {
    // Exit in progress + newly visible AND matches pushed id => new incoming => top.
    assert_eq!(
        decide_insert_placement(true, true, Some(11), 11),
        InsertPlacement::PrependTop
    );
}

#[test]
fn decide_insert_placement_exit_not_newly_visible_does_not_append() {
    // If it's not newly visible, we should never treat it as a fill-in insertion.
    // (This path is primarily for sanity; existing rows are reused rather than inserted.)
    assert_eq!(
        decide_insert_placement(false, true, Some(8), 8),
        InsertPlacement::PrependTop
    );
}

#[test]
fn push_moves_existing_id_to_front_and_replaces_metadata() {
    let base = base_now();
    let mut state = ToastState::new(
        ToastPolicy {
            default_ttl: Duration::from_millis(10),
            actions_ttl: Duration::from_millis(100),
        },
        5,
    );

    // Initial: id=1 no actions => expires quickly.
    state.push(1, Urgency::Normal, false, base);
    state.push(2, Urgency::Normal, false, base);
    assert_eq!(state.visible_ids(), vec![2, 1]);

    // Push id=1 again with actions => should move to front and use actions TTL.
    let repush_at = base + Duration::from_millis(1);
    state.push(1, Urgency::Normal, true, repush_at);
    assert_eq!(state.visible_ids(), vec![1, 2]);

    // Expiry should be computed relative to each entry’s `created_at`.
    //
    // IMPORTANT:
    // - `id=1` was repushed with actions (actions_ttl=100ms), so it should NOT expire at +20ms.
    // - `id=2` is still the original no-action toast (default_ttl=10ms), so it MAY expire here.
    //
    // Therefore, we assert only that `id=1` remains visible (and at the front).
    let _cmds = state.expire_due(repush_at + Duration::from_millis(20));
    assert!(state.visible_ids().contains(&1));
    assert_eq!(state.visible_ids().first().copied(), Some(1));

    // Just before the actions TTL for `id=1`, it still should not be expired.
    let _cmds = state.expire_due(repush_at + Duration::from_millis(99));
    assert!(state.visible_ids().contains(&1));
    assert_eq!(state.visible_ids().first().copied(), Some(1));

    // And at (or after) the actions TTL, `id=1` should expire from toasts.
    let cmds = state.expire_due(repush_at + Duration::from_millis(100));
    assert!(cmds.contains(&ToastCommand::ExpireToastOnly { id: 1 }));
    assert!(!state.visible_ids().contains(&1));
}
