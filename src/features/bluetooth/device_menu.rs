//! Bluetooth device menu widget.
//!
//! Displays a list of paired Bluetooth devices with connect/disconnect toggles.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glib::SignalHandlerId;
use gtk::prelude::*;

use super::store::DeviceConnectionState;

/// Output events from the device menu.
#[derive(Debug, Clone)]
pub enum DeviceMenuOutput {
    Connect(String),    // device path
    Disconnect(String), // device path
}

/// A single device row in the menu.
struct DeviceRow {
    root: gtk::Box,
    spinner: gtk::Spinner,
    switch: gtk::Switch,
    connection_state: Rc<RefCell<DeviceConnectionState>>,
    switch_handler_id: SignalHandlerId,
}

impl DeviceRow {
    fn new(
        path: String,
        name: &str,
        icon: &str,
        connection: DeviceConnectionState,
        on_output: Rc<RefCell<Option<Box<dyn Fn(DeviceMenuOutput)>>>>,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["device-row"])
            .build();

        // Device icon
        let icon_image = gtk::Image::builder()
            .icon_name(icon)
            .pixel_size(20)
            .build();

        // Device name
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
            .css_classes(["device-switch"])
            .build();

        root.append(&icon_image);
        root.append(&name_label);
        root.append(&spinner);
        root.append(&switch);

        let connection_state = Rc::new(RefCell::new(connection.clone()));

        // Set initial state (before connecting handler)
        Self::apply_switch_state(&switch, &spinner, &connection);

        // Connect switch handler
        let path_clone = path.clone();
        let connection_state_ref = connection_state.clone();
        let switch_handler_id = switch.connect_state_set(move |_switch, active| {
            let current = connection_state_ref.borrow().clone();
            // Only react if not already in a transitional state
            if current == DeviceConnectionState::Connecting
                || current == DeviceConnectionState::Disconnecting
            {
                // Prevent the state change
                return glib::Propagation::Stop;
            }

            if let Some(ref callback) = *on_output.borrow() {
                if active {
                    callback(DeviceMenuOutput::Connect(path_clone.clone()));
                } else {
                    callback(DeviceMenuOutput::Disconnect(path_clone.clone()));
                }
            }

            // Prevent automatic state change - we'll update it when the connection succeeds
            glib::Propagation::Stop
        });

        Self {
            root,
            spinner,
            switch,
            connection_state,
            switch_handler_id,
        }
    }

    fn update_connection(&self, connection: DeviceConnectionState) {
        *self.connection_state.borrow_mut() = connection.clone();

        // Block the signal handler while updating
        self.switch.block_signal(&self.switch_handler_id);
        Self::apply_switch_state(&self.switch, &self.spinner, &connection);
        self.switch.unblock_signal(&self.switch_handler_id);
    }

    fn apply_switch_state(
        switch: &gtk::Switch,
        spinner: &gtk::Spinner,
        connection: &DeviceConnectionState,
    ) {
        match connection {
            DeviceConnectionState::Disconnected => {
                switch.set_active(false);
                switch.set_sensitive(true);
                spinner.set_visible(false);
                spinner.set_spinning(false);
            }
            DeviceConnectionState::Connecting => {
                switch.set_active(false);
                switch.set_sensitive(false);
                spinner.set_visible(true);
                spinner.set_spinning(true);
            }
            DeviceConnectionState::Connected => {
                switch.set_active(true);
                switch.set_sensitive(true);
                spinner.set_visible(false);
                spinner.set_spinning(false);
            }
            DeviceConnectionState::Disconnecting => {
                switch.set_active(true);
                switch.set_sensitive(false);
                spinner.set_visible(true);
                spinner.set_spinning(true);
            }
        }
    }
}

/// Widget displaying a list of paired Bluetooth devices.
pub struct DeviceMenuWidget {
    pub root: gtk::Box,
    rows: Rc<RefCell<HashMap<String, DeviceRow>>>,
    on_output: Rc<RefCell<Option<Box<dyn Fn(DeviceMenuOutput)>>>>,
}

impl DeviceMenuWidget {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(["device-menu"])
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
        F: Fn(DeviceMenuOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the list of devices.
    pub fn set_devices(&self, devices: Vec<(String, String, String, DeviceConnectionState)>) {
        let mut rows = self.rows.borrow_mut();

        // Remove rows for devices that no longer exist
        let current_paths: std::collections::HashSet<String> =
            devices.iter().map(|(p, _, _, _)| p.clone()).collect();

        let removed: Vec<String> = rows
            .keys()
            .filter(|p| !current_paths.contains(*p))
            .cloned()
            .collect();

        for path in removed {
            if let Some(row) = rows.remove(&path) {
                self.root.remove(&row.root);
            }
        }

        // Add or update rows
        for (path, name, icon, connection) in devices {
            if let Some(row) = rows.get(&path) {
                // Update existing row
                row.update_connection(connection);
            } else {
                // Create new row
                let row = DeviceRow::new(
                    path.clone(),
                    &name,
                    &icon,
                    connection,
                    self.on_output.clone(),
                );
                self.root.append(&row.root);
                rows.insert(path, row);
            }
        }
    }

    /// Update a single device's connection state.
    pub fn set_device_connection(&self, path: &str, connection: DeviceConnectionState) {
        if let Some(row) = self.rows.borrow().get(path) {
            row.update_connection(connection);
        }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

impl Default for DeviceMenuWidget {
    fn default() -> Self {
        Self::new()
    }
}
