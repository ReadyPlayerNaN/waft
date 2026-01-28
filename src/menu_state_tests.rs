use super::*;

#[test]
fn test_open_menu_sets_active() {
    let store = create_menu_store();
    store.emit(MenuOp::OpenMenu("menu-1".to_string()));
    let state = store.get_state();
    assert_eq!(state.active_menu_id, Some("menu-1".to_string()));
}

#[test]
fn test_open_different_menu_replaces_active() {
    let store = create_menu_store();
    store.emit(MenuOp::OpenMenu("menu-1".to_string()));
    store.emit(MenuOp::OpenMenu("menu-2".to_string()));
    let state = store.get_state();
    assert_eq!(state.active_menu_id, Some("menu-2".to_string()));
}

#[test]
fn test_close_menu_only_if_active() {
    let store = create_menu_store();
    store.emit(MenuOp::OpenMenu("menu-1".to_string()));
    store.emit(MenuOp::CloseMenu("menu-2".to_string()));
    let state = store.get_state();
    assert_eq!(state.active_menu_id, Some("menu-1".to_string()));
}

#[test]
fn test_close_active_menu() {
    let store = create_menu_store();
    store.emit(MenuOp::OpenMenu("menu-1".to_string()));
    store.emit(MenuOp::CloseMenu("menu-1".to_string()));
    let state = store.get_state();
    assert_eq!(state.active_menu_id, None);
}

#[test]
fn test_close_all() {
    let store = create_menu_store();
    store.emit(MenuOp::OpenMenu("menu-1".to_string()));
    store.emit(MenuOp::CloseAll);
    let state = store.get_state();
    assert_eq!(state.active_menu_id, None);
}
