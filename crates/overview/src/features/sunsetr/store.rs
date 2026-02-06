//! Sunsetr store module.
//!
//! Manages sunsetr state with instance-based stores.

use crate::set_field;
use crate::store::{PluginStore, StoreOp, StoreState};

/// State for the sunsetr plugin.
#[derive(Clone, Default)]
pub struct SunsetrState {
    /// True if sunsetr process is running
    pub active: bool,
    /// Current period ("day", "night", or custom)
    pub period: Option<String>,
    /// Next transition time (HH:MM)
    pub next_transition: Option<String>,
    pub busy: bool,
    /// True if presets are available
    pub has_presets: bool,
    /// Currently active preset (None = default, Some(name) = preset active)
    pub active_preset: Option<String>,
}

/// Operations for the sunsetr store.
#[derive(Clone)]
#[allow(clippy::enum_variant_names)]
pub enum SunsetrOp {
    SetStatus {
        active: bool,
        period: Option<String>,
        next_transition: Option<String>,
    },
    SetBusy(bool),
    SetHasPresets(bool),
    SetActivePreset(Option<String>),
}

impl StoreOp for SunsetrOp {}

impl StoreState for SunsetrState {
    type Config = ();
    fn configure(&mut self, _: &()) {}
}

/// Type alias for the sunsetr store.
pub type SunsetrStore = PluginStore<SunsetrOp, SunsetrState>;

/// Create a new sunsetr store instance.
pub fn create_sunsetr_store() -> SunsetrStore {
    PluginStore::new(|state: &mut SunsetrState, op: SunsetrOp| match op {
        SunsetrOp::SetStatus {
            active,
            period,
            next_transition,
        } => {
            let changed = state.active != active
                || state.period != period
                || state.next_transition != next_transition;
            state.active = active;
            state.period = period;
            state.next_transition = next_transition;
            changed
        }
        SunsetrOp::SetBusy(busy) => set_field!(state.busy, busy),
        SunsetrOp::SetHasPresets(has_presets) => set_field!(state.has_presets, has_presets),
        SunsetrOp::SetActivePreset(preset) => set_field!(state.active_preset, preset),
    })
}
