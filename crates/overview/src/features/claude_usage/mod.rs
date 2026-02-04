//! Claude usage plugin - displays API usage metrics.
use crate::menu_state::MenuStore;

mod api;
pub mod values;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use log::{debug, error};
use serde::Deserialize;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gtk::prelude::*;

use crate::plugin::{Plugin, PluginId, Slot, Widget, WidgetRegistrar};
use crate::ui::claude_usage::{ClaudeUsageState, ClaudeUsageWidget, MetricsConfig};

use self::api::fetch_usage;
use self::values::UsageWindow;

/// Configuration for metrics display.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MetricsSettings {
    pub show_message_count: bool,
    pub show_token_usage: bool,
    pub show_rate_info: bool,
}

impl Default for MetricsSettings {
    fn default() -> Self {
        Self {
            show_message_count: true,
            show_token_usage: true,
            show_rate_info: false,
        }
    }
}

impl From<MetricsSettings> for MetricsConfig {
    fn from(settings: MetricsSettings) -> Self {
        Self {
            show_message_count: settings.show_message_count,
            show_token_usage: settings.show_token_usage,
            show_rate_info: settings.show_rate_info,
        }
    }
}

/// Configuration for the Claude usage plugin.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ClaudeUsageConfig {
    /// Admin API key (required).
    pub api_key: String,
    /// Update interval in seconds (default: 300 = 5 minutes).
    pub update_interval: u64,
    /// Usage window: "session", "hourly", "daily", or "weekly".
    pub window: String,
    /// Metrics display configuration.
    pub metrics: MetricsSettings,
}

impl Default for ClaudeUsageConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            update_interval: 300,
            window: "session".to_string(),
            metrics: MetricsSettings::default(),
        }
    }
}

pub struct ClaudeUsagePlugin {
    widget: Rc<RefCell<Option<ClaudeUsageWidget>>>,
    config: ClaudeUsageConfig,
    app_start_time: DateTime<Utc>,
}

impl ClaudeUsagePlugin {
    pub fn new() -> Self {
        Self {
            widget: Rc::new(RefCell::new(None)),
            config: ClaudeUsageConfig::default(),
            app_start_time: Utc::now(),
        }
    }

    fn window(&self) -> UsageWindow {
        UsageWindow::from_str(&self.config.window)
    }
}

#[async_trait(?Send)]
impl Plugin for ClaudeUsagePlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::claude-usage")
    }

    fn configure(&mut self, settings: &toml::Table) -> Result<()> {
        self.config = settings.clone().try_into()?;

        // Validate API key
        if self.config.api_key.is_empty() {
            anyhow::bail!("Claude usage plugin requires an API key");
        }

        if !self.config.api_key.starts_with("sk-ant-admin-") {
            anyhow::bail!("Invalid API key format - must start with 'sk-ant-admin-'");
        }

        debug!("Configured claude-usage plugin: window={}, update_interval={}s",
               self.config.window, self.config.update_interval);
        Ok(())
    }

    async fn init(&mut self) -> Result<()> {
        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        _menu_store: Arc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let usage_widget = ClaudeUsageWidget::new();

        // Register the widget
        registrar.register_widget(Arc::new(Widget {
            id: "claude-usage:main".to_string(),
            slot: Slot::Header,
            el: usage_widget.root.clone().upcast::<gtk::Widget>(),
            weight: 15,
        }));

        // Store the widget
        *self.widget.borrow_mut() = Some(usage_widget);

        // Initial fetch
        let widget_ref = self.widget.clone();
        let api_key = self.config.api_key.clone();
        let window = self.window();
        let app_start_time = self.app_start_time;
        let metrics_config: MetricsConfig = self.config.metrics.clone().into();

        // Fetch usage in background using glib spawn
        {
            let widget_ref = widget_ref.clone();
            let metrics_config = metrics_config.clone();
            let api_key = api_key.clone();
            glib::spawn_future_local(async move {
                debug!("[claude-usage] Fetching initial usage for window: {:?}", window);
                match fetch_usage(&api_key, window, app_start_time).await {
                    Ok(data) => {
                        debug!("[claude-usage] Loaded: {} messages, {} tokens",
                               data.message_count, data.total_tokens);
                        if let Some(ref widget) = *widget_ref.borrow() {
                            widget.update(&ClaudeUsageState::Loaded(data, metrics_config));
                        }
                    }
                    Err(e) => {
                        error!("[claude-usage] Failed to fetch usage: {:?}", e);
                        if let Some(ref widget) = *widget_ref.borrow() {
                            widget.update(&ClaudeUsageState::Error(
                                "Failed to load usage".to_string(),
                            ));
                        }
                    }
                }
            });
        }

        // Schedule periodic updates
        let update_interval = self.config.update_interval;
        glib::timeout_add_local(Duration::from_secs(update_interval), move || {
            let widget_ref = widget_ref.clone();
            let metrics_config = metrics_config.clone();
            let api_key = api_key.clone();
            glib::spawn_future_local(async move {
                debug!("[claude-usage] Fetching usage update");
                match fetch_usage(&api_key, window, app_start_time).await {
                    Ok(data) => {
                        if let Some(ref widget) = *widget_ref.borrow() {
                            widget.update(&ClaudeUsageState::Loaded(data, metrics_config));
                        }
                    }
                    Err(e) => {
                        error!("[claude-usage] Failed to fetch usage: {:?}", e);
                        // Don't update to error state on refresh failures,
                        // keep showing the last known good data
                    }
                }
            });
            glib::ControlFlow::Continue
        });

        Ok(())
    }
}
