//! Pure, GTK-free app router reducer.
//!
//! This module encodes routing rules and emits non-UI "effects" that can be
//! executed by the Relm4 runtime / application layer in later migration steps.
//!
//! Step 03 requirements:
//! - Keep GTK-free
//! - Deterministic reducer: (state, msg) -> (state, effects)
//! - Overlay visibility drives toast gating:
//!   - OverlayShown  => toast gating disabled
//!   - OverlayHidden => toast gating enabled
//! - Ability to route plugin-directed messages via effects
//! - Unit-testable without initializing GTK/main loop

use crate::relm4_app::events::AppMsg;

/// Router state that is safe to keep outside GTK.
///
/// This is intentionally small in step 03. It will grow as we add more routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterState {
    /// Whether the overlay is currently visible.
    pub overlay_visible: bool,

    /// Whether toast popups are allowed to show while the overlay is hidden.
    ///
    /// Per policy: toasts should be gated off when overlay is shown.
    pub toast_gating_enabled: bool,
}

impl Default for RouterState {
    fn default() -> Self {
        Self {
            overlay_visible: false,
            // Initial assumption: overlay starts hidden in the real app, so toasts are allowed.
            // (This is just routing state, not behavior.)
            toast_gating_enabled: true,
        }
    }
}

/// Pure, non-UI effects emitted by the router.
///
/// These are "intents" that later layers can interpret.
///
/// Updated for Option 1.5A (typed plugin handles): the router no longer emits a
/// generic "send message to plugin" effect because plugin message typing lives
/// inside plugins (via `PluginSpec::Input`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouterEffect {
    /// Enable/disable toast gating in the toast subsystem.
    SetToastGating { enabled: bool },

    /// Optional: request a toast layout recomputation/invalidations.
    ///
    /// Not used yet in step 03, but included as a stable placeholder for later
    /// toast-window work (height/zero-height behavior).
    InvalidateToastLayout,
}

/// Reduce one message through the router.
///
/// Returns:
/// - updated `RouterState`
/// - list of `RouterEffect`s to execute (non-UI)
pub fn reduce_router(mut state: RouterState, msg: AppMsg) -> (RouterState, Vec<RouterEffect>) {
    let mut effects = Vec::new();

    match msg {
        AppMsg::OverlayShown => {
            state.overlay_visible = true;

            // Rule: overlay shown => disable toast gating (i.e. do not pop toasts).
            if state.toast_gating_enabled {
                state.toast_gating_enabled = false;
                effects.push(RouterEffect::SetToastGating { enabled: false });
            }
        }

        AppMsg::OverlayHidden => {
            state.overlay_visible = false;

            // Rule: overlay hidden => enable toast gating.
            if !state.toast_gating_enabled {
                state.toast_gating_enabled = true;
                effects.push(RouterEffect::SetToastGating { enabled: true });
            }
        }

        // Step 03: type plumbing only. We don't route notifications yet, but we must
        // be able to carry them through the app message type. Later steps (07+)
        // will decide how to map ingress to plugin components.
        AppMsg::NotificationsIngress(_ingress) => {}

        // Internal/derived surface (kept for forward-compat). For now, we treat this
        // as "already processed intent" and simply sync state.
        AppMsg::ToastGatingChanged { enabled } => {
            state.toast_gating_enabled = enabled;
            effects.push(RouterEffect::SetToastGating { enabled });
        }
    }

    (state, effects)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_shown_disables_toast_gating() {
        let state0 = RouterState {
            overlay_visible: false,
            toast_gating_enabled: true,
        };

        let (state1, effects) = reduce_router(state0, AppMsg::OverlayShown);

        assert!(state1.overlay_visible);
        assert!(!state1.toast_gating_enabled);
        assert_eq!(
            effects,
            vec![RouterEffect::SetToastGating { enabled: false }]
        );
    }

    #[test]
    fn overlay_hidden_enables_toast_gating() {
        let state0 = RouterState {
            overlay_visible: true,
            toast_gating_enabled: false,
        };

        let (state1, effects) = reduce_router(state0, AppMsg::OverlayHidden);

        assert!(!state1.overlay_visible);
        assert!(state1.toast_gating_enabled);
        assert_eq!(
            effects,
            vec![RouterEffect::SetToastGating { enabled: true }]
        );
    }

    #[test]
    fn overlay_shown_is_idempotent_for_gating_effect() {
        let state0 = RouterState {
            overlay_visible: false,
            toast_gating_enabled: true,
        };

        let (state1, effects1) = reduce_router(state0, AppMsg::OverlayShown);
        assert_eq!(
            effects1,
            vec![RouterEffect::SetToastGating { enabled: false }]
        );

        // Sending OverlayShown again should not re-emit gating effect.
        let (_state2, effects2) = reduce_router(state1, AppMsg::OverlayShown);
        assert_eq!(effects2, Vec::<RouterEffect>::new());
    }
}
