//! VPN connection menu widget.
//!
//! Displays a list of configured VPN connections with connect/disconnect toggles.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glib::SignalHandlerId;
use gtk::prelude::*;

use super::store::VpnState;
use crate::ui::menu_item::MenuItemWidget;

/// Output events from the VPN menu.
#[derive(Debug, Clone)]
pub enum VpnMenuOutput {
    Connect(String),    // connection path
    Disconnect(String), // connection path
}

/// A single VPN connection row in the menu.
struct VpnRow {
    menu_item: MenuItemWidget,
    #[allow(dead_code)]
    content: gtk::Box,
    spinner: gtk::Spinner,
    switch: gtk::Switch,
    vpn_state: Rc<RefCell<VpnState>>,
    switch_handler_id: SignalHandlerId,
}

impl VpnRow {
    fn new(
        path: String,
        name: &str,
        state: VpnState,
        on_output: Rc<RefCell<Option<Box<dyn Fn(VpnMenuOutput)>>>>,
    ) -> Self {
        // Build content structure
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["vpn-row"])
            .build();

        // VPN icon
        let icon_image = gtk::Image::builder()
            .icon_name("network-vpn-symbolic")
            .pixel_size(20)
            .build();

        // VPN name
        let name_label = gtk::Label::builder()
            .label(name)
            .hexpand(true)
            .xalign(0.0)
            .build();

        // Spinner (for connecting/disconnecting state)
        let spinner = gtk::Spinner::builder()
            .spinning(false)
            .visible(false)
            .build();

        // Switch for connect/disconnect
        let switch = gtk::Switch::builder()
            .valign(gtk::Align::Center)
            .css_classes(["vpn-switch"])
            .build();

        content.append(&icon_image);
        content.append(&name_label);
        content.append(&spinner);
        content.append(&switch);

        let vpn_state = Rc::new(RefCell::new(state.clone()));

        // Set initial state (before connecting handler)
        Self::apply_switch_state(&switch, &spinner, &state);

        // Connect switch handler
        let path_clone = path.clone();
        let vpn_state_ref = vpn_state.clone();
        let switch_handler_id = switch.connect_state_set(move |_switch, active| {
            let current = vpn_state_ref.borrow().clone();
            // Only react if not already in a transitional state
            if matches!(current, VpnState::Connecting | VpnState::Disconnecting) {
                // Prevent the state change
                return glib::Propagation::Stop;
            }

            if let Some(ref callback) = *on_output.borrow() {
                if active {
                    callback(VpnMenuOutput::Connect(path_clone.clone()));
                } else {
                    callback(VpnMenuOutput::Disconnect(path_clone.clone()));
                }
            }

            // Prevent automatic state change - we'll update it when the connection succeeds
            glib::Propagation::Stop
        });

        // Wrap content with MenuItemWidget
        let switch_clone = switch.clone();
        let menu_item = MenuItemWidget::new(&content, move || {
            // Toggle the switch when row is clicked
            switch_clone.set_active(!switch_clone.is_active());
        });

        Self {
            menu_item,
            content,
            spinner,
            switch,
            vpn_state,
            switch_handler_id,
        }
    }

    fn update_state(&self, state: VpnState) {
        *self.vpn_state.borrow_mut() = state.clone();

        // Block the signal handler while updating
        self.switch.block_signal(&self.switch_handler_id);
        Self::apply_switch_state(&self.switch, &self.spinner, &state);
        self.switch.unblock_signal(&self.switch_handler_id);
    }

    fn apply_switch_state(switch: &gtk::Switch, spinner: &gtk::Spinner, state: &VpnState) {
        match state {
            VpnState::Disconnected => {
                switch.set_active(false);
                switch.set_sensitive(true);
                spinner.set_visible(false);
                spinner.set_spinning(false);
            }
            VpnState::Connecting => {
                switch.set_active(false);
                switch.set_sensitive(false);
                spinner.set_visible(true);
                spinner.set_spinning(true);
            }
            VpnState::Connected => {
                switch.set_active(true);
                switch.set_sensitive(true);
                spinner.set_visible(false);
                spinner.set_spinning(false);
            }
            VpnState::Disconnecting => {
                switch.set_active(true);
                switch.set_sensitive(false);
                spinner.set_visible(true);
                spinner.set_spinning(true);
            }
        }
    }
}

/// Widget displaying a list of VPN connections.
#[derive(Clone)]
pub struct VpnMenuWidget {
    pub root: gtk::Box,
    rows: Rc<RefCell<HashMap<String, VpnRow>>>,
    on_output: Rc<RefCell<Option<Box<dyn Fn(VpnMenuOutput)>>>>,
}

impl VpnMenuWidget {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(["vpn-menu"])
            .build();

        Self {
            root,
            rows: Rc::new(RefCell::new(HashMap::new())),
            on_output: Rc::new(RefCell::new(None)),
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(VpnMenuOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the list of VPN connections.
    pub fn set_connections(&self, connections: Vec<(String, String, VpnState)>) {
        let mut rows = self.rows.borrow_mut();

        // Remove rows for connections that no longer exist
        let current_paths: std::collections::HashSet<String> =
            connections.iter().map(|(p, _, _)| p.clone()).collect();

        let removed: Vec<String> = rows
            .keys()
            .filter(|p| !current_paths.contains(*p))
            .cloned()
            .collect();

        for path in removed {
            if let Some(row) = rows.remove(&path) {
                self.root.remove(row.menu_item.widget());
            }
        }

        // Add or update rows
        for (path, name, state) in connections {
            if let Some(row) = rows.get(&path) {
                // Update existing row
                row.update_state(state);
            } else {
                // Create new row
                let row = VpnRow::new(path.clone(), &name, state, self.on_output.clone());
                self.root.append(row.menu_item.widget());
                rows.insert(path, row);
            }
        }
    }

    /// Update a single VPN connection's state.
    pub fn set_connection_state(&self, path: &str, state: VpnState) {
        if let Some(row) = self.rows.borrow().get(path) {
            row.update_state(state);
        }
    }

    /// Get the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}

impl Default for VpnMenuWidget {
    fn default() -> Self {
        Self::new()
    }
}
