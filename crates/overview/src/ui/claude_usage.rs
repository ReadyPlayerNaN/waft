//! Pure GTK4 Claude usage widget.
//!
//! Displays Claude API usage metrics with icon and statistics.

use gtk::prelude::*;

use crate::features::claude_usage::values::UsageData;

/// State of the Claude usage widget.
#[derive(Debug, Clone)]
pub enum ClaudeUsageState {
    Loading,
    Loaded(UsageData, MetricsConfig),
    Error(String),
}

/// Configuration for which metrics to display.
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    pub show_message_count: bool,
    pub show_token_usage: bool,
    pub show_rate_info: bool,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            show_message_count: true,
            show_token_usage: true,
            show_rate_info: false,
        }
    }
}

/// Pure GTK4 Claude usage widget - displays usage icon and metrics.
pub struct ClaudeUsageWidget {
    pub root: gtk::Box,
    icon: gtk::Image,
    metrics_box: gtk::Box,
    message_label: gtk::Label,
    token_label: gtk::Label,
    spinner: gtk::Spinner,
    error_label: gtk::Label,
    content_box: gtk::Box,
}

impl ClaudeUsageWidget {
    /// Create a new Claude usage widget.
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["claude-usage-container"])
            .build();

        // Claude icon
        let icon = gtk::Image::builder()
            .icon_name("user-info-symbolic")
            .pixel_size(32)
            .css_classes(["claude-usage-icon"])
            .build();

        // Metrics labels
        let metrics_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .valign(gtk::Align::Center)
            .build();

        let message_label = gtk::Label::builder()
            .label("Messages: --")
            .xalign(0.0)
            .css_classes(["title-3"])
            .build();

        let token_label = gtk::Label::builder()
            .label("Tokens: --")
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();

        metrics_box.append(&message_label);
        metrics_box.append(&token_label);

        // Content box (icon + metrics)
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        content_box.append(&icon);
        content_box.append(&metrics_box);

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
            icon,
            metrics_box,
            message_label,
            token_label,
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
            ClaudeUsageState::Loaded(data, config) => {
                self.spinner.set_visible(false);
                self.spinner.set_spinning(false);
                self.content_box.set_visible(true);
                self.error_label.set_visible(false);

                // Update message count
                if config.show_message_count {
                    let msg_text = format!("Messages: {}", UsageData::format_messages(data.message_count));
                    self.message_label.set_label(&msg_text);
                    self.message_label.set_visible(true);
                } else {
                    self.message_label.set_visible(false);
                }

                // Update token usage
                if config.show_token_usage {
                    let token_text = format!("Tokens: {}", UsageData::format_tokens(data.total_tokens));
                    self.token_label.set_label(&token_text);
                    self.token_label.set_visible(true);
                } else {
                    self.token_label.set_visible(false);
                }

                // Hide metrics box if no metrics are shown
                self.metrics_box.set_visible(config.show_message_count || config.show_token_usage);
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
