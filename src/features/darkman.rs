use crate::plugins::FeatureToggle;
use crate::plugins::Plugin;
use crate::ui::FeatureSpec;
use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;

/// Dark mode plugin implementation
pub struct DarkmanPlugin {
    /// Current dark mode state (true = dark, false = light)
    enabled: bool,
    /// Sender for communicating state changes
    state_sender: Option<mpsc::UnboundedSender<bool>>,
    /// Whether the plugin is initialized
    initialized: bool,
}

impl DarkmanPlugin {
    /// Create a new dark mode plugin
    pub fn new() -> Self {
        Self {
            enabled: false,
            state_sender: None,
            initialized: false,
        }
    }

    /// Check the current darkman state
    async fn get_current_state() -> Result<bool> {
        let output = TokioCommand::new("darkman")
            .arg("get")
            .output()
            .await
            .context("Failed to execute 'darkman get'")?;

        if !output.status.success() {
            anyhow::bail!("darkman get failed with status: {}", output.status);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let state = stdout.trim().to_lowercase();

        match state.as_str() {
            "dark" => Ok(true),
            "light" => Ok(false),
            other => anyhow::bail!("Unexpected darkman state: {}", other),
        }
    }

    /// Set darkman state
    async fn set_state(dark: bool) -> Result<()> {
        let mode = if dark { "dark" } else { "light" };

        let output = TokioCommand::new("darkman")
            .arg("set")
            .arg(mode)
            .output()
            .await
            .with_context(|| format!("Failed to execute 'darkman set {}'", mode))?;

        if !output.status.success() {
            anyhow::bail!("darkman set {} failed with status: {}", mode, output.status);
        }

        Ok(())
    }

    fn feature_toggle(&self) -> FeatureToggle {
        let initial_state = self.initialized && self.enabled;
        FeatureToggle {
            id: "plugin::darkman".to_string(),
            el: FeatureSpec::contentless_with_toggle(
                "plugin::darkman",
                "Dark mode".to_string(),
                "weather-clear-night-symbolic".to_string(),
                initial_state,
                move |_key: &'static str,
                      _current_active: bool|
                      -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
                    // Use tokio::task::block_in_place to run async operations synchronously
                    tokio::task::block_in_place(|| {
                        // Always get fresh state before toggling to ensure consistency
                        tokio::runtime::Handle::current().block_on(async {
                            match Self::get_current_state().await {
                                Ok(current_darkman_state) => {
                                    let new_state = !current_darkman_state;

                                    // Set new dark mode state
                                    match Self::set_state(new_state).await {
                                        Ok(()) => Ok(new_state),
                                        Err(e) => {
                                            eprintln!("Failed to set dark mode: {}", e);
                                            // Return current state on error (toggle failed)
                                            Ok(current_darkman_state)
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to get current dark mode state: {}", e);
                                    // Return parameter state as fallback
                                    Ok(_current_active)
                                }
                            }
                        })
                    })
                },
            ),
            weight: 10,
        }
    }

    /// Start monitoring darkman for changes
    async fn start_monitoring(&mut self) -> Result<()> {
        let current_state = &self.enabled;
        let (tx, _rx) = mpsc::unbounded_channel::<bool>();

        // Store sender for cleanup
        self.state_sender = Some(tx.clone());

        // Initialize current state
        let initial_state = *current_state;
        println!(
            "Initial darkman state: {}",
            if initial_state { "dark" } else { "light" }
        );

        // Spawn monitoring task
        let tx_clone = tx.clone();
        // tokio::spawn(async move {
        //     let mut interval = interval(Duration::from_secs(2));

        //     loop {
        //         interval.tick().await;

        //         match Self::get_current_state().await {
        //             Ok(new_state) => {
        //                 let current = *current_state;
        //                 if current != new_state {
        //                     self.enabled = new_state;
        //                     println!(
        //                         "Darkman state changed to: {}",
        //                         if new_state { "dark" } else { "light" }
        //                     );

        //                     // Send update notification
        //                     if let Err(_) = tx_clone.send(new_state) {
        //                         // Channel closed, stop monitoring
        //                         break;
        //                     }
        //                 }
        //             }
        //             Err(e) => {
        //                 eprintln!("Failed to get darkman state: {}", e);
        //             }
        //         }
        //     }
        // });

        Ok(())
    }
}

impl Default for DarkmanPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for DarkmanPlugin {
    fn name(&self) -> &str {
        "dark-mode"
    }

    async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        // Check if darkman is available
        let output = TokioCommand::new("darkman").arg("--version").output().await;

        match output {
            Ok(output) if output.status.success() => {
                println!(
                    "darkman found: {}",
                    String::from_utf8_lossy(&output.stdout).trim()
                );
            }
            Ok(_) => {
                anyhow::bail!("darkman command failed (not installed?)");
            }
            Err(e) => {
                anyhow::bail!("darkman command not found: {}", e);
            }
        }

        // Start monitoring darkman
        self.start_monitoring().await?;
        self.initialized = true;
        self.enabled = Self::get_current_state().await?;

        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Close the monitoring channel
        if let Some(_sender) = self.state_sender.take() {
            // Channel will be closed when sender is dropped
        }

        self.initialized = false;
        Ok(())
    }

    fn feature_toggles(&self) -> Vec<FeatureToggle> {
        vec![self.feature_toggle()]
    }
}
