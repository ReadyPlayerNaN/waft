//! Plugin runner: eliminates boilerplate main() functions.
//!
//! Every plugin binary follows the same sequence: handle manifest CLI,
//! init logger, build tokio runtime, create plugin, create PluginRuntime,
//! optionally spawn background tasks, then run. This module provides
//! [`PluginRunner`] to express that in a few lines.

use std::future::Future;

use anyhow::{Context, Result};

use crate::manifest;
use crate::notifier::EntityNotifier;
use crate::plugin::Plugin;
use crate::runtime::PluginRuntime;

/// Builder for running a plugin with minimal boilerplate.
///
/// # Example
///
/// ```rust,no_run
/// # use anyhow::Result;
/// # use waft_plugin::runner::PluginRunner;
/// # struct MyPlugin;
/// # impl MyPlugin {
/// #     async fn new() -> Result<Self> { Ok(Self) }
/// # }
/// # #[async_trait::async_trait]
/// # impl waft_plugin::Plugin for MyPlugin {
/// #     fn get_entities(&self) -> Vec<waft_plugin::Entity> { vec![] }
/// #     async fn handle_action(&self, _: waft_plugin::Urn, _: String, _: serde_json::Value)
/// #         -> Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
/// # }
/// fn main() -> Result<()> {
///     PluginRunner::new("my-plugin", &["my-entity"])
///         .run(|_notifier| async {
///             MyPlugin::new().await
///         })
/// }
/// ```
pub struct PluginRunner<'a> {
    name: &'a str,
    entity_types: &'a [&'a str],
    i18n: Option<(&'a waft_i18n::I18n, &'a str, &'a str)>,
    plain_meta: Option<(&'a str, &'a str)>,
}

impl<'a> PluginRunner<'a> {
    /// Create a runner for a plugin with the given daemon name and entity types.
    pub fn new(name: &'a str, entity_types: &'a [&'a str]) -> Self {
        Self {
            name,
            entity_types,
            i18n: None,
            plain_meta: None,
        }
    }

    /// Set i18n-based manifest metadata (most plugins use this).
    pub fn i18n(
        mut self,
        i18n: &'a waft_i18n::I18n,
        name_key: &'a str,
        description_key: &'a str,
    ) -> Self {
        self.i18n = Some((i18n, name_key, description_key));
        self
    }

    /// Set plain-string manifest metadata (no i18n).
    pub fn meta(mut self, name: &'a str, description: &'a str) -> Self {
        self.plain_meta = Some((name, description));
        self
    }

    /// Handle manifest, init logger, build tokio runtime, and run the plugin.
    ///
    /// The `build` closure receives an [`EntityNotifier`] and must return the
    /// constructed plugin. Spawn any background tasks inside this closure
    /// before returning the plugin.
    pub fn run<P, F, Fut>(self, build: F) -> Result<()>
    where
        P: Plugin + 'static,
        F: FnOnce(EntityNotifier) -> Fut,
        Fut: Future<Output = Result<P>>,
    {
        // Handle manifest CLI
        if let Some((i18n, name_key, desc_key)) = self.i18n {
            if manifest::handle_provides_i18n(self.entity_types, i18n, name_key, desc_key) {
                return Ok(());
            }
        } else if let Some((name, description)) = self.plain_meta {
            if manifest::handle_provides_full(self.entity_types, name, description) {
                return Ok(());
            }
        } else if manifest::handle_provides_full(self.entity_types, "", "") {
            return Ok(());
        }

        crate::init_plugin_logger("info");
        log::info!("Starting {} plugin...", self.name);

        let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
        rt.block_on(async {
            let (notifier, notifier_rx) = EntityNotifier::new_pair();
            // Keep a clone of the notifier alive for the duration of the
            // runtime. Plugins that don't need background notifications
            // (e.g. caffeine, brightness) ignore the notifier in their build
            // closure, which drops the only Sender and causes the runtime's
            // notifier_rx.changed() to error out immediately. This keepalive
            // prevents that — same pattern as the claim_tx keepalive in
            // PluginRuntime::run().
            let _notifier_keepalive = notifier.clone();
            let plugin = build(notifier).await?;
            let runtime = PluginRuntime::from_parts(self.name, plugin, notifier_rx);
            runtime.run().await
        })
    }
}

/// Helper to spawn a monitored background task that logs errors and exit.
///
/// Use this instead of bare `tokio::spawn` for plugin background tasks
/// (D-Bus signal monitors, event loops, etc.) to ensure failures are visible.
pub fn spawn_monitored<F>(label: &'static str, fut: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = fut.await {
            log::error!("[{label}] task failed: {e}");
        }
        log::debug!("[{label}] task stopped");
    })
}

/// Like [`spawn_monitored`] but accepts `anyhow::Result`.
pub fn spawn_monitored_anyhow<F>(label: &'static str, fut: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = Result<()>> + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = fut.await {
            log::error!("[{label}] task failed: {e}");
        }
        log::debug!("[{label}] task stopped");
    })
}
