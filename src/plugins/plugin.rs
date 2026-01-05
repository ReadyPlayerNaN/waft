use super::bindings::FeatureToggle;
use anyhow::Result;
use async_trait::async_trait;

/// Plugin interface that all plugins must implement
#[async_trait]
pub trait Plugin: Send + Sync {
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
}
