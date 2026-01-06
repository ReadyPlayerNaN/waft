use super::bindings::FeatureToggle;
use super::bindings::Widget;
use anyhow::Result;
use async_trait::async_trait;

/// Plugin interface that all plugins must implement
#[async_trait(?Send)]
pub trait Plugin {
    /// Enable downcasting of plugin trait objects.
    ///
    /// This allows the app/registry to call plugin-specific hooks without baking them into the
    /// `Plugin` trait itself.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Get the unique name of this plugin
    fn name(&self) -> &str;

    /// Initialize the plugin. Called once on startup.
    async fn initialize(&mut self) -> Result<()>;

    /// Clean up resources. Called on shutdown.
    async fn cleanup(&mut self) -> Result<()> {
        // Default implementation does nothing
        Ok(())
    }

    /// Get all feature toggles provided by this plugin
    fn feature_toggles(&self) -> Vec<FeatureToggle>;

    /// Get all feature toggles provided by this plugin
    fn widgets(&self) -> Vec<Widget>;
}
