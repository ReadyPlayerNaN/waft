//! Caffeine store module.
//!
//! Manages caffeine state with instance-based stores.

use waft_core::set_field;
use waft_core::store::{PluginStore, StoreOp, StoreState};

/// State for the caffeine plugin.
#[derive(Clone, Default)]
pub struct CaffeineState {
    pub active: bool,
    pub busy: bool,
}

/// Operations for the caffeine store.
#[derive(Clone)]
pub enum CaffeineOp {
    SetActive(bool),
    SetBusy(bool),
}

impl StoreOp for CaffeineOp {}

impl StoreState for CaffeineState {
    type Config = ();
    fn configure(&mut self, _: &()) {}
}

/// Type alias for the caffeine store.
pub type CaffeineStore = PluginStore<CaffeineOp, CaffeineState>;

/// Create a new caffeine store instance.
pub fn create_caffeine_store() -> CaffeineStore {
    PluginStore::new(|state: &mut CaffeineState, op: CaffeineOp| match op {
        CaffeineOp::SetActive(active) => set_field!(state.active, active),
        CaffeineOp::SetBusy(busy) => set_field!(state.busy, busy),
    })
}
