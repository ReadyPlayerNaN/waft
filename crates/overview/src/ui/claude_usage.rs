//! Pure GTK4 Claude usage widget.
//!
//! Displays Claude API usage limits with percentages and reset times.

use gtk::prelude::*;

use crate::features::claude_usage::values::UsageData;

/// State of the Claude usage widget.
#[derive(Debug, Clone)]
pub enum ClaudeUsageState {
    Loading,
    Loaded(UsageData),
    Error(String),
}

/// Pure GTK4 Claude usage widget - displays usage limits and reset times.
pub struct ClaudeUsageWidget {
    pub root: gtk::Box,
    session_label: gtk::Label,
    session_reset: gtk::Label,
    weekly_label: gtk::Label,
    weekly_reset: gtk::Label,
    spinner: gtk::Spinner,
    error_label: gtk::Label,
    content_box: gtk::Box,
}

impl ClaudeUsageWidget {
    /// Create a new Claude usage widget.
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["claude-usage-container"])
            .build();

        // Anthropic logo
        let logo_path = "crates/overview/resources/anthropic-logo.svg";
        let logo = if let Ok(texture) = gtk::gdk::Texture::from_filename(logo_path) {
            gtk::Image::builder()
                .paintable(&texture)
                .pixel_size(24)
                .valign(gtk::Align::Center)
                .build()
        } else {
            // Fallback to generic icon
            gtk::Image::builder()
                .icon_name("user-info-symbolic")
                .pixel_size(24)
                .valign(gtk::Align::Center)
                .build()
        };

        root.append(&logo);

        // Session limit section
        let session_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .valign(gtk::Align::Center)
            .build();

        let session_label = gtk::Label::builder()
            .label("Session: --")
            .xalign(0.0)
            .css_classes(["title-3"])
            .build();

        let session_reset = gtk::Label::builder()
            .label("Resets --")
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .build();

        session_box.append(&session_label);
        session_box.append(&session_reset);

        // Weekly limit section
        let weekly_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .valign(gtk::Align::Center)
            .build();

        let weekly_label = gtk::Label::builder()
            .label("Weekly: --")
            .xalign(0.0)
            .css_classes(["title-3"])
            .build();

        let weekly_reset = gtk::Label::builder()
            .label("Resets --")
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .build();

        weekly_box.append(&weekly_label);
        weekly_box.append(&weekly_reset);

        // Content box (both sections)
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(16)
            .build();
        content_box.append(&session_box);
        content_box.append(&weekly_box);

        // Loading spinner
        let spinner = gtk::Spinner::builder().spinning(true).build();

        // Error label
        let error_label = gtk::Label::builder()
            .label("")
            .css_classes(["error"])
            .visible(false)
            .build();

        // Initially show loading state
        root.append(&spinner);
        root.append(&content_box);
        root.append(&error_label);

        content_box.set_visible(false);

        Self {
            root,
            session_label,
            session_reset,
            weekly_label,
            weekly_reset,
            spinner,
            error_label,
            content_box,
        }
    }

    /// Update the widget with new usage state.
    pub fn update(&self, state: &ClaudeUsageState) {
        match state {
            ClaudeUsageState::Loading => {
                self.spinner.set_visible(true);
                self.spinner.set_spinning(true);
                self.content_box.set_visible(false);
                self.error_label.set_visible(false);
            }
            ClaudeUsageState::Loaded(data) => {
                self.spinner.set_visible(false);
                self.spinner.set_spinning(false);
                self.content_box.set_visible(true);
                self.error_label.set_visible(false);

                // Update session limit (5-hour)
                if let Some(ref five_hour) = data.five_hour {
                    self.session_label.set_label(&format!("Session: {:.0}%", five_hour.utilization));
                    self.session_reset.set_label(&format!("Resets {}", five_hour.format_reset_time()));
                } else {
                    self.session_label.set_label("Session: N/A");
                    self.session_reset.set_label("");
                }

                // Update weekly limit (7-day)
                if let Some(ref seven_day) = data.seven_day {
                    self.weekly_label.set_label(&format!("Weekly: {:.0}%", seven_day.utilization));
                    self.weekly_reset.set_label(&format!("Resets {}", seven_day.format_reset_time()));
                } else {
                    self.weekly_label.set_label("Weekly: N/A");
                    self.weekly_reset.set_label("");
                }
            }
            ClaudeUsageState::Error(msg) => {
                self.spinner.set_visible(false);
                self.spinner.set_spinning(false);
                self.content_box.set_visible(false);
                self.error_label.set_visible(true);
                self.error_label.set_label(msg);
            }
        }
    }
}
