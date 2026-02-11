//! Simple example plugin daemon demonstrating the plugin SDK.
//!
//! This example creates a basic plugin with a single toggle widget.
//! Run with: cargo run --example simple_plugin

use waft_plugin_sdk::*;

/// A simple plugin that toggles a feature on/off.
struct SimplePlugin {
    enabled: std::sync::Mutex<bool>,
}

impl SimplePlugin {
    fn new() -> Self {
        Self {
            enabled: std::sync::Mutex::new(false),
        }
    }
}

#[async_trait::async_trait]
impl PluginDaemon for SimplePlugin {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let enabled = *self.enabled.lock().unwrap();
        vec![NamedWidget {
            id: "simple:toggle".into(),
            weight: 100,
            widget: Widget::FeatureToggle {
                title: "Simple Plugin".into(),
                icon: "emblem-system-symbolic".into(),
                details: Some(if enabled {
                    "Feature is enabled".into()
                } else {
                    "Feature is disabled".into()
                }),
                active: enabled,
                busy: false,
                expandable: false,
                expanded_content: None,
                on_toggle: Action {
                    id: "toggle".into(),
                    params: ActionParams::None,
                },
            },
        }]
    }

    async fn handle_action(
        &self,
        widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Received action: widget={}, action={:?}", widget_id, action.id);

        match action.id.as_str() {
            "toggle" => {
                let mut enabled = self.enabled.lock().unwrap();
                *enabled = !*enabled;
                log::info!("Toggled: enabled={}", *enabled);
                Ok(())
            }
            _ => {
                log::warn!("Unknown action: {}", action.id);
                Ok(())
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting simple plugin daemon...");

    // Create plugin daemon
    let daemon = SimplePlugin::new();

    // Create and run server
    let (server, _notifier) = PluginServer::new("simple", daemon);
    server.run().await?;

    Ok(())
}
