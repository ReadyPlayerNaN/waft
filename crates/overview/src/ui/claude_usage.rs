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
    requests_label: gtk::Label,
    requests_reset: gtk::Label,
    tokens_label: gtk::Label,
    tokens_reset: gtk::Label,
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

        // Requests limit section
        let requests_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .valign(gtk::Align::Center)
            .build();

        let requests_label = gtk::Label::builder()
            .label("Requests: --")
            .xalign(0.0)
            .css_classes(["title-3"])
            .build();

        let requests_reset = gtk::Label::builder()
            .label("Resets --")
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .build();

        requests_box.append(&requests_label);
        requests_box.append(&requests_reset);

        // Tokens limit section
        let tokens_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .valign(gtk::Align::Center)
            .build();

        let tokens_label = gtk::Label::builder()
            .label("Tokens: --")
            .xalign(0.0)
            .css_classes(["title-3"])
            .build();

        let tokens_reset = gtk::Label::builder()
            .label("Resets --")
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .build();

        tokens_box.append(&tokens_label);
        tokens_box.append(&tokens_reset);

        // Content box (both sections)
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(16)
            .build();
        content_box.append(&requests_box);
        content_box.append(&tokens_box);

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
            requests_label,
            requests_reset,
            tokens_label,
            tokens_reset,
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

                // Update request limits
                if let Some(ref requests) = data.requests {
                    self.requests_label.set_label(&format!("Requests: {:.0}%", requests.utilization()));
                    self.requests_reset.set_label(&format!("Resets {}", requests.format_reset_time()));
                } else {
                    self.requests_label.set_label("Requests: N/A");
                    self.requests_reset.set_label("");
                }

                // Update token limits
                if let Some(ref tokens) = data.tokens {
                    self.tokens_label.set_label(&format!("Tokens: {:.0}%", tokens.utilization()));
                    self.tokens_reset.set_label(&format!("Resets {}", tokens.format_reset_time()));
                } else {
                    self.tokens_label.set_label("Tokens: N/A");
                    self.tokens_reset.set_label("");
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
