//! Sunsetr store module.
//!
//! Manages sunsetr state with instance-based stores.

use crate::set_field;
use crate::store::{PluginStore, StoreOp, StoreState};

/// State for the sunsetr plugin.
#[derive(Clone, Default)]
pub struct SunsetrState {
    pub active: bool,
    pub next_transition: Option<String>,
    pub busy: bool,
}

/// Operations for the sunsetr store.
#[derive(Clone)]
pub enum SunsetrOp {
    SetStatus {
        active: bool,
        next_transition: Option<String>,
    },
    SetBusy(bool),
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
            next_transition,
        } => {
            let changed = state.active != active || state.next_transition != next_transition;
            state.active = active;
            state.next_transition = next_transition;
            changed
        }
        SunsetrOp::SetBusy(busy) => set_field!(state.busy, busy),
    })
}
