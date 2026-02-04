//! Darkman store module.
//!
//! Manages darkman state with instance-based stores.

use super::values::DarkmanMode;
use crate::set_field;
use crate::store::{PluginStore, StoreOp, StoreState};

/// State for the darkman plugin.
#[derive(Clone, Default)]
pub struct DarkmanState {
    pub mode: DarkmanMode,
    pub busy: bool,
}

/// Operations for the darkman store.
#[derive(Clone)]
pub enum DarkmanOp {
    SetMode(DarkmanMode),
    SetBusy(bool),
}

impl StoreOp for DarkmanOp {}

impl StoreState for DarkmanState {
    type Config = ();
    fn configure(&mut self, _: &()) {}
}

/// Type alias for the darkman store.
pub type DarkmanStore = PluginStore<DarkmanOp, DarkmanState>;

/// Create a new darkman store instance.
pub fn create_darkman_store() -> DarkmanStore {
    PluginStore::new(|state: &mut DarkmanState, op: DarkmanOp| match op {
        DarkmanOp::SetMode(mode) => set_field!(state.mode, mode),
        DarkmanOp::SetBusy(busy) => set_field!(state.busy, busy),
    })
}
