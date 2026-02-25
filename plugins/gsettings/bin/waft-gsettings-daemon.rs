//! GSettings plugin -- GTK appearance settings.
//!
//! Provides a `gtk-appearance` entity with the current accent colour.
//! Monitors changes via the XDG Desktop Portal `SettingChanged` D-Bus signal
//! and reads/writes via the `gsettings` CLI.

use std::sync::OnceLock;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use waft_i18n::I18n;
use waft_plugin::dbus_monitor::{SignalMonitorConfig, monitor_signal_async};
use waft_plugin::*;
use zbus::Connection;

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/gsettings.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/gsettings.ftl")),
        ])
    })
}

/// Valid accent colour values accepted by GTK.
const VALID_ACCENT_COLORS: &[&str] = &[
    "blue", "teal", "green", "yellow", "orange", "red", "pink", "purple", "slate",
];

/// Plugin state.
struct AppearanceState {
    accent_color: Option<String>,
}

/// Read the current accent colour via `gsettings get`.
async fn read_accent_color() -> Option<String> {
    let output = tokio::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "accent-color"])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        log::debug!(
            "[gsettings] gsettings get failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // gsettings outputs values like 'blue' (with quotes) -- strip them
    let color = raw.trim_matches('\'').trim().to_string();

    if VALID_ACCENT_COLORS.contains(&color.as_str()) {
        Some(color)
    } else {
        log::debug!("[gsettings] Unknown accent colour value: {raw}");
        // Still return it -- the schema may have been extended
        if color.is_empty() { None } else { Some(color) }
    }
}

/// Write accent colour via `gsettings set`.
async fn write_accent_color(color: &str) -> Result<(), String> {
    let output = tokio::process::Command::new("gsettings")
        .args([
            "set",
            "org.gnome.desktop.interface",
            "accent-color",
            color,
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to spawn gsettings: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("gsettings set failed: {stderr}"));
    }

    Ok(())
}

struct GsettingsPlugin {
    state: Arc<StdMutex<AppearanceState>>,
}

impl GsettingsPlugin {
    async fn new() -> Self {
        let accent_color = read_accent_color().await;
        log::info!("[gsettings] Initial accent colour: {accent_color:?}");

        Self {
            state: Arc::new(StdMutex::new(AppearanceState { accent_color })),
        }
    }

    fn current_color(&self) -> Option<String> {
        match self.state.lock() {
            Ok(g) => g.accent_color.clone(),
            Err(e) => {
                log::warn!("[gsettings] Mutex poisoned, recovering: {e}");
                e.into_inner().accent_color.clone()
            }
        }
    }
}

#[async_trait::async_trait]
impl Plugin for GsettingsPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let color = match self.current_color() {
            Some(c) => c,
            None => return Vec::new(),
        };

        let appearance = entity::appearance::GtkAppearance {
            accent_color: color,
        };

        vec![Entity::new(
            Urn::new(
                "gsettings",
                entity::appearance::GTK_APPEARANCE_ENTITY_TYPE,
                "default",
            ),
            entity::appearance::GTK_APPEARANCE_ENTITY_TYPE,
            &appearance,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.as_str() {
            "set-accent-color" => {
                let color = params
                    .get("color")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'color' parameter")?;

                if !VALID_ACCENT_COLORS.contains(&color) {
                    return Err(format!(
                        "Invalid accent colour '{}'. Valid values: {}",
                        color,
                        VALID_ACCENT_COLORS.join(", ")
                    )
                    .into());
                }

                write_accent_color(color)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

                // Update local state
                match self.state.lock() {
                    Ok(mut g) => g.accent_color = Some(color.to_string()),
                    Err(e) => {
                        log::warn!("[gsettings] Mutex poisoned, recovering: {e}");
                        e.into_inner().accent_color = Some(color.to_string());
                    }
                }

                log::info!("[gsettings] Accent colour set to: {color}");
            }
            _ => {
                log::warn!("[gsettings] Unknown action: {action}");
            }
        }

        Ok(())
    }

    fn can_stop(&self) -> bool {
        true
    }
}

/// Monitor the XDG Desktop Portal for accent colour changes.
///
/// The portal maps `org.gnome.desktop.interface` to the namespace
/// `org.freedesktop.appearance`. The signal is `SettingChanged` with
/// args (namespace: str, key: str, value: variant).
async fn monitor_portal_settings(
    conn: Connection,
    state: Arc<StdMutex<AppearanceState>>,
    notifier: EntityNotifier,
) -> Result<()> {
    let config = SignalMonitorConfig::builder()
        .sender("org.freedesktop.portal.Desktop")
        .path("/org/freedesktop/portal/desktop")
        .interface("org.freedesktop.portal.Settings")
        .member("SettingChanged")
        .build()?;

    monitor_signal_async(conn, config, state, notifier, |msg, _| {
        // Extract signal body synchronously before the async boundary
        let body_result: Result<(String, String, zbus::zvariant::OwnedValue), _> =
            msg.body().deserialize();

        async move {
            let (namespace, key, _value) = body_result?;

            if namespace != "org.freedesktop.appearance" || key != "accent-color" {
                return Ok(None);
            }

            log::debug!("[gsettings] Portal SettingChanged: {namespace}.{key}");

            // Re-read via gsettings to get the canonical value
            match read_accent_color().await {
                Some(color) => {
                    log::info!("[gsettings] Accent colour changed externally to: {color}");
                    Ok(Some(AppearanceState {
                        accent_color: Some(color),
                    }))
                }
                None => {
                    log::debug!(
                        "[gsettings] Could not read accent colour after portal signal"
                    );
                    Ok(None)
                }
            }
        }
    })
    .await
}

fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides_i18n(
        &[entity::appearance::GTK_APPEARANCE_ENTITY_TYPE],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
        return Ok(());
    }

    waft_plugin::init_plugin_logger("info");

    log::info!("[gsettings] Starting gsettings plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = GsettingsPlugin::new().await;
        let shared_state = plugin.state.clone();

        let conn = Connection::session()
            .await
            .context("failed to connect to session bus")?;

        let (runtime, notifier) = PluginRuntime::new("gsettings", plugin);

        // Monitor portal for external accent colour changes
        tokio::spawn(async move {
            if let Err(e) = monitor_portal_settings(conn, shared_state, notifier).await {
                log::error!("[gsettings] Portal signal monitoring failed: {e}");
            }
            log::debug!("[gsettings] Portal monitor task stopped");
        });

        runtime.run().await?;
        Ok(())
    })
}
