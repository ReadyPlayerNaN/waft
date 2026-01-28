//! Global menu state management.
//!
//! Ensures only one collapsible menu is open at a time across the entire application.
//! All menus coordinate through MenuStore using a unidirectional data flow pattern.

use crate::store::{PluginStore, StoreOp, StoreState};

/// State tracking which menu (if any) is currently open.
#[derive(Clone, Debug)]
pub struct MenuState {
    /// UUID of the currently active menu, or None if all menus are closed.
    pub active_menu_id: Option<String>,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            active_menu_id: None,
        }
    }
}

impl StoreState for MenuState {
    type Config = ();

    fn configure(&mut self, _config: &Self::Config) {
        // No configuration needed for menu state
    }
}

/// Operations that can be performed on menu state.
#[derive(Clone, Debug)]
pub enum MenuOp {
    /// Open a specific menu (closes any other open menu).
    OpenMenu(String),
    /// Close a specific menu (only if it's currently active).
    CloseMenu(String),
    /// Close all menus.
    CloseAll,
}

impl StoreOp for MenuOp {}

/// Store for global menu state coordination.
pub type MenuStore = PluginStore<MenuOp, MenuState>;

/// Create a new MenuStore with the standard menu coordination logic.
pub fn create_menu_store() -> MenuStore {
    MenuStore::new(|state, op| match op {
        MenuOp::OpenMenu(id) => {
            let changed = state.active_menu_id.as_ref() != Some(&id);
            state.active_menu_id = Some(id);
            changed
        }
        MenuOp::CloseMenu(id) => {
            if state.active_menu_id.as_ref() == Some(&id) {
                state.active_menu_id = None;
                true
            } else {
                false
            }
        }
        MenuOp::CloseAll => {
            let changed = state.active_menu_id.is_some();
            state.active_menu_id = None;
            changed
        }
    })
}

#[cfg(test)]
#[path = "menu_state_tests.rs"]
mod tests;
