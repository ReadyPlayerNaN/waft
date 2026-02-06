//! Battery store module.
//!
//! Manages battery state with subscription-based notifications.

use super::values::BatteryInfo;
use waft_core::store::{PluginStore, StoreOp, StoreState};

// Re-export the macro from waft_core
pub use waft_core::set_field;

/// State for the battery plugin.
#[derive(Clone, Default)]
pub struct BatteryStoreState {
    pub info: BatteryInfo,
}

/// Operations for the battery store.
#[derive(Clone)]
pub enum BatteryOp {
    SetInfo(BatteryInfo),
}

impl StoreOp for BatteryOp {}

impl StoreState for BatteryStoreState {
    type Config = ();
    fn configure(&mut self, _: &()) {}
}

/// Type alias for the battery store.
pub type BatteryStore = PluginStore<BatteryOp, BatteryStoreState>;

/// Create a new battery store instance.
pub fn create_battery_store() -> BatteryStore {
    PluginStore::new(|state: &mut BatteryStoreState, op: BatteryOp| match op {
        BatteryOp::SetInfo(info) => set_field!(state.info, info),
    })
}
