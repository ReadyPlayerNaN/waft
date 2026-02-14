//! Menu state coordination utilities.
//!
//! Provides helper functions for managing expandable widget menu state
//! through waft-core's MenuStore. Ensures only one menu is open at a time.

use waft_core::menu_state::{MenuOp, MenuStore};

/// Helper to check if a specific menu is currently open.
///
/// # Arguments
/// * `menu_store` - The shared MenuStore instance
/// * `menu_id` - The unique identifier for this menu
///
/// # Returns
/// `true` if this menu is currently the active menu, `false` otherwise
pub fn is_menu_open(menu_store: &MenuStore, menu_id: &str) -> bool {
    let state = menu_store.get_state();
    state.active_menu_id.as_deref() == Some(menu_id)
}

/// Toggle a menu's open/closed state.
///
/// If the menu is currently open, it will be closed.
/// If the menu is currently closed, it will be opened (closing any other open menu).
///
/// This is the most common operation for expandable widgets.
///
/// # Arguments
/// * `menu_store` - The shared MenuStore instance
/// * `menu_id` - The unique identifier for this menu
pub fn toggle_menu(menu_store: &MenuStore, menu_id: &str) {
    menu_store.emit(MenuOp::OpenMenu(menu_id.to_string()));
}

/// Generate a deterministic menu ID from a widget ID.
///
/// This ensures consistent menu IDs across re-renders.
///
/// # Arguments
/// * `widget_id` - The unique identifier for the widget
///
/// # Returns
/// A menu ID string in the format "{widget_id}_menu"
///
/// # Example
/// ```ignore
/// let widget_id = "bluetooth:adapter0";
/// let menu_id = menu_id_for_widget(widget_id);
/// assert_eq!(menu_id, "bluetooth:adapter0_menu");
/// ```
pub fn menu_id_for_widget(widget_id: &str) -> String {
    format!("{}_menu", widget_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_core::menu_state::create_menu_store;

    #[test]
    fn test_menu_id_generation() {
        assert_eq!(
            menu_id_for_widget("bluetooth:adapter0"),
            "bluetooth:adapter0_menu"
        );
        assert_eq!(menu_id_for_widget("network:wifi"), "network:wifi_menu");
    }

    #[test]
    fn test_is_menu_open_when_closed() {
        let store = create_menu_store();
        assert!(!is_menu_open(&store, "test_menu"));
    }

    #[test]
    fn test_toggle_menu_opens_when_closed() {
        let store = create_menu_store();
        toggle_menu(&store, "test_menu");
        assert!(is_menu_open(&store, "test_menu"));
    }

    #[test]
    fn test_toggle_menu_closes_when_open() {
        let store = create_menu_store();
        toggle_menu(&store, "test_menu");
        assert!(is_menu_open(&store, "test_menu"));

        toggle_menu(&store, "test_menu");
        assert!(!is_menu_open(&store, "test_menu"));
    }

    #[test]
    fn test_toggle_menu_opens_and_closes_other() {
        let store = create_menu_store();
        toggle_menu(&store, "menu1");
        assert!(is_menu_open(&store, "menu1"));

        toggle_menu(&store, "menu2");
        assert!(!is_menu_open(&store, "menu1"));
        assert!(is_menu_open(&store, "menu2"));
    }
}
