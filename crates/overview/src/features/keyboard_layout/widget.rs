//! GTK widget for keyboard layout indicator and switching.
//!
//! This module implements the visual button that displays the current keyboard layout
//! and handles user interactions for cycling through layouts.

use gtk::prelude::*;
use log::{error, info, warn};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::backends::KeyboardLayoutBackend;

/// Keyboard layout indicator and switcher widget.
///
/// Displays a keyboard icon and the current keyboard layout as an uppercase abbreviation
/// (e.g., "US", "DE") and cycles through available layouts when clicked.
pub struct KeyboardLayoutWidget {
    pub root: gtk::Button,
    pub label: gtk::Label,
    #[allow(dead_code)] // Icon is displayed in UI, field keeps it alive
    icon: gtk::Image,
    backend: Arc<Mutex<Option<Arc<dyn KeyboardLayoutBackend>>>>,
}

impl KeyboardLayoutWidget {
    /// Create a new keyboard layout widget.
    ///
    /// # Arguments
    ///
    /// * `backend` - Optional keyboard layout backend. If None, shows fallback label.
    /// * `app` - GTK application for error dialogs
    pub fn new(
        backend: Arc<Mutex<Option<Arc<dyn KeyboardLayoutBackend>>>>,
        app: gtk::Application,
    ) -> Self {
        // Create button
        let root = gtk::Button::builder()
            .css_classes(["keyboard-layout-button"])
            .build();

        // Create content box for icon and label
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        // Create keyboard icon
        let icon = gtk::Image::builder()
            .icon_name("input-keyboard-symbolic")
            .css_classes(["keyboard-layout-icon"])
            .build();

        // Create label
        let label = gtk::Label::builder()
            .css_classes(["keyboard-layout-label"])
            .label("??") // Fallback label
            .build();

        content_box.append(&icon);
        content_box.append(&label);
        root.set_child(Some(&content_box));

        // Set accessible properties
        root.set_accessible_role(gtk::AccessibleRole::Button);
        root.update_property(&[gtk::accessible::Property::Label("Keyboard Layout")]);

        // Make button keyboard navigable (GTK buttons are focusable by default)
        root.set_can_focus(true);

        let widget = Self {
            root,
            label,
            icon,
            backend,
        };

        // Query initial layout
        widget.query_and_update_label();

        // Connect click handler (left-click for next layout)
        widget.connect_click_handler(app.clone());

        // Connect secondary click handler (right-click for previous layout)
        widget.connect_secondary_click_handler(app);

        widget
    }

    /// Query the current layout and update the button label.
    fn query_and_update_label(&self) {
        let backend = self.backend.clone();
        let label = self.label.clone();
        let root = self.root.clone();

        glib::spawn_future_local(async move {
            // Spawn on tokio runtime for async backend call
            let result: anyhow::Result<String> = crate::runtime::spawn_on_tokio(async move {
                let backend_guard = backend.lock().await;
                if let Some(ref backend) = *backend_guard {
                    let info = backend.get_layout_info().await?;
                    Ok(info.current)
                } else {
                    // No backend available
                    Ok("??".to_string())
                }
            })
            .await;

            match result {
                Ok(layout) => {
                    label.set_label(&layout);
                    // Update accessible description
                    root.update_property(&[gtk::accessible::Property::Description(&format!(
                        "Current layout: {}",
                        layout
                    ))]);

                    // Enable button if we have a valid layout
                    if layout != "??" {
                        root.set_sensitive(true);
                    }
                }
                Err(e) => {
                    warn!("[keyboard-layout] Failed to query initial layout: {}", e);
                    label.set_label("??");
                    root.set_sensitive(false);
                }
            }
        });
    }

    /// Connect the button click handler for cycling to the next layout.
    fn connect_click_handler(&self, app: gtk::Application) {
        let backend = self.backend.clone();
        let label = self.label.clone();
        let root_button = self.root.clone();

        self.root.connect_clicked(move |_| {
            let backend = backend.clone();
            let label = label.clone();
            let app = app.clone();
            let root_button = root_button.clone();

            // Store previous layout for error recovery
            let previous_label = label.label().to_string();

            glib::spawn_future_local(async move {
                // Spawn on tokio runtime for async backend call
                let result: anyhow::Result<String> = crate::runtime::spawn_on_tokio(async move {
                    let backend_guard = backend.lock().await;
                    if let Some(ref backend) = *backend_guard {
                        // Cycle to next layout
                        backend.switch_next().await?;
                        // Query new layout
                        let info = backend.get_layout_info().await?;
                        Ok(info.current)
                    } else {
                        Err(anyhow::anyhow!("Keyboard layout backend not available"))
                    }
                })
                .await;

                match result {
                    Ok(new_layout) => {
                        label.set_label(&new_layout);
                        // Update accessible description
                        root_button.update_property(&[gtk::accessible::Property::Description(
                            &format!("Current layout: {}", new_layout),
                        )]);
                        info!("[keyboard-layout] Cycled to layout: {}", new_layout);
                    }
                    Err(e) => {
                        error!("[keyboard-layout] Failed to cycle layout: {}", e);
                        // Revert to previous label
                        label.set_label(&previous_label);
                        // Show error dialog
                        Self::show_error_dialog(&app, &e);
                    }
                }
            });
        });
    }

    /// Connect a secondary click handler for cycling to the previous layout.
    fn connect_secondary_click_handler(&self, app: gtk::Application) {
        let backend = self.backend.clone();
        let label = self.label.clone();
        let root_button = self.root.clone();

        // Create a gesture for secondary (right) click
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3); // Secondary button (right-click)

        gesture.connect_released(move |gesture, _, _, _| {
            // Stop propagation
            gesture.set_state(gtk::EventSequenceState::Claimed);

            let backend = backend.clone();
            let label = label.clone();
            let app = app.clone();
            let root_button = root_button.clone();

            // Store previous layout for error recovery
            let previous_label = label.label().to_string();

            glib::spawn_future_local(async move {
                // Spawn on tokio runtime for async backend call
                let result: anyhow::Result<String> = crate::runtime::spawn_on_tokio(async move {
                    let backend_guard = backend.lock().await;
                    if let Some(ref backend) = *backend_guard {
                        // Cycle to previous layout
                        backend.switch_prev().await?;
                        // Query new layout
                        let info = backend.get_layout_info().await?;
                        Ok(info.current)
                    } else {
                        Err(anyhow::anyhow!("Keyboard layout backend not available"))
                    }
                })
                .await;

                match result {
                    Ok(new_layout) => {
                        label.set_label(&new_layout);
                        // Update accessible description
                        root_button.update_property(&[gtk::accessible::Property::Description(
                            &format!("Current layout: {}", new_layout),
                        )]);
                        info!("[keyboard-layout] Cycled to previous layout: {}", new_layout);
                    }
                    Err(e) => {
                        error!("[keyboard-layout] Failed to cycle to previous layout: {}", e);
                        // Revert to previous label
                        label.set_label(&previous_label);
                        // Show error dialog
                        Self::show_error_dialog(&app, &e);
                    }
                }
            });
        });

        self.root.add_controller(gesture);
    }

    /// Show an error dialog for layout switching failures.
    fn show_error_dialog(app: &gtk::Application, error: &anyhow::Error) {
        let (title, message) = Self::get_error_message(error);

        if let Some(window) = app.active_window() {
            let dialog = gtk::MessageDialog::builder()
                .transient_for(&window)
                .modal(true)
                .message_type(gtk::MessageType::Error)
                .buttons(gtk::ButtonsType::Ok)
                .text(title)
                .secondary_text(&message)
                .build();

            dialog.connect_response(move |dialog, _| {
                dialog.close();
            });

            dialog.present();
        }
    }

    /// Determine error message based on error type.
    fn get_error_message(error: &anyhow::Error) -> (&'static str, String) {
        let error_str = error.to_string();

        // Check for PolicyKit authorization errors
        if error_str.contains("org.freedesktop.PolicyKit")
            || error_str.contains("Not authorized")
            || error_str.contains("Authorization")
        {
            (
                "Permission Denied",
                "You don't have permission to change the keyboard layout. Contact your system administrator.".to_string(),
            )
        }
        // Check for connection errors
        else if error_str.contains("connection") || error_str.contains("D-Bus") {
            (
                "System Service Unavailable",
                "Could not connect to keyboard layout service.".to_string(),
            )
        }
        // Check for command execution errors
        else if error_str.contains("Failed to execute") {
            (
                "Command Failed",
                format!("Failed to communicate with compositor: {}", error),
            )
        }
        // Generic error
        else {
            (
                "Layout Switch Failed",
                format!("Failed to switch keyboard layout: {}", error),
            )
        }
    }
}
