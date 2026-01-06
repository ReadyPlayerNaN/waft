use crate::dbus::DbusHandle;
use crate::plugins::{FeatureToggle, Plugin, Widget};
use crate::ui::UiEvent;
use crate::ui::features::FeatureSpec;
use anyhow::Result;
use async_trait::async_trait;
use dbus::message::MatchRule;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;

const DARKMAN_INTERFACE: &str = "nl.whynothugo.darkman";
const DARKMAN_PATH: &str = "/nl/whynothugo/darkman";
const FEATURE_KEY: &str = "plugin::darkman";

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DarkmanMode {
    Dark = 1,
    Light = 2,
}

impl DarkmanMode {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }

    fn toggle(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }
}

/// Small, cloneable controller used by async UI callbacks and DBus signal handlers.
///
/// This avoids capturing `&mut self` (or `Arc<&mut Self>`) inside `FeatureSpec` closures,
/// keeps the closure `'static`, and centralizes "apply mode + emit UI event".
#[derive(Clone)]
struct DarkmanController {
    mode: Arc<AtomicU8>,
    dbus: Arc<DbusHandle>,
    ui_event_tx: Option<mpsc::UnboundedSender<UiEvent>>,
}

impl DarkmanController {
    fn current_mode(&self) -> DarkmanMode {
        match self.mode.load(Ordering::Relaxed) {
            x if x == DarkmanMode::Dark as u8 => DarkmanMode::Dark,
            _ => DarkmanMode::Light,
        }
    }

    fn apply_mode(&self, m: DarkmanMode) {
        self.mode.store(m as u8, Ordering::Relaxed);

        if let Some(tx) = self.ui_event_tx.as_ref() {
            let _ = tx.send(UiEvent::FeatureActiveChanged {
                key: FEATURE_KEY.to_string(),
                active: m.is_dark(),
            });
        }
    }

    fn apply_mode_from_str(&self, value: &str) {
        if let Some(m) = DarkmanMode::from_str(value) {
            self.apply_mode(m);
        }
    }

    async fn set_next_state(&self) {
        let next_state = self.current_mode().toggle();

        match DarkmanPlugin::set_state(&self.dbus, next_state).await {
            Ok(()) => self.apply_mode(next_state),
            Err(e) => eprintln!("Failed to set dark mode: {}", e),
        }
    }
}

/// Dark mode plugin implementation
pub struct DarkmanPlugin {
    mode: Arc<AtomicU8>,
    /// Shared async DBus handle (dbus-tokio + SyncConnection)
    dbus: Arc<DbusHandle>,
    initialized: bool,
    /// Declarative UI description for this plugin's feature tile
    toggle: Option<FeatureSpec>,
    /// Optional sender into the central UI event bus.
    ///
    /// Currently this is only a placeholder; wiring to the real UI event loop
    /// will be done at the application level.
    ui_event_tx: Option<mpsc::UnboundedSender<UiEvent>>,
}

impl DarkmanPlugin {
    /// Create a new dark mode plugin
    pub fn new(dbus: Arc<DbusHandle>) -> Self {
        Self {
            dbus,
            mode: Arc::new(AtomicU8::new(DarkmanMode::Light as u8)),
            initialized: false,
            toggle: None,
            ui_event_tx: None,
        }
    }

    /// Inject a UI event sender that will be used to notify the UI of
    /// state changes. This is intentionally optional and can be a no-op
    /// if not configured by the application.
    pub fn with_ui_event_sender(mut self, tx: mpsc::UnboundedSender<UiEvent>) -> Self {
        self.ui_event_tx = Some(tx);
        self
    }

    fn controller(&self) -> DarkmanController {
        DarkmanController {
            mode: self.mode.clone(),
            dbus: self.dbus.clone(),
            ui_event_tx: self.ui_event_tx.clone(),
        }
    }

    /// Check current darkman state via async DBus using a shared SyncConnection.
    async fn get_current_state(conn: &DbusHandle) -> Result<DarkmanMode> {
        let value = conn
            .get_property(DARKMAN_INTERFACE, DARKMAN_PATH, "Mode")
            .await?;

        Ok(value
            .as_deref()
            .and_then(DarkmanMode::from_str)
            .unwrap_or(DarkmanMode::Light))
    }

    /// Set darkman state via async DBus using a shared SyncConnection.
    ///
    /// NOTE: The `conn` is passed as the first argument as requested.
    async fn set_state(conn: &DbusHandle, mode: DarkmanMode) -> Result<()> {
        conn.set_property(DARKMAN_INTERFACE, DARKMAN_PATH, "Mode", mode.as_str())
            .await
    }

    fn create_feature_toggle_el(&mut self) {
        let enabled_flag = self.controller().current_mode().is_dark();
        let ctl = self.controller();

        let el = FeatureSpec::contentless_with_toggle(
            FEATURE_KEY,
            "Dark mode".to_string(),
            "weather-clear-night-symbolic".to_string(),
            enabled_flag,
            move |_key: &'static str, _current_active: bool| {
                let ctl = ctl.clone();
                async move {
                    ctl.set_next_state().await;
                }
            },
        );

        self.toggle = Some(el);
    }

    fn feature_toggle_el(&self) -> &FeatureSpec {
        self.toggle
            .as_ref()
            .expect("DarkmanPlugin toggle not initialized")
    }

    fn feature_toggle(&self) -> FeatureToggle {
        FeatureToggle {
            el: self.feature_toggle_el().clone(),
            weight: 10,
        }
    }

    async fn start_monitoring(&mut self) -> Result<()> {
        let ctl = self.controller();
        let handle_value = move |value: Option<String>| {
            if let Some(value) = value {
                ctl.apply_mode_from_str(value.as_ref());
            }
        };

        let rule = MatchRule::new_signal(DARKMAN_INTERFACE, "ModeChanged");
        self.dbus.listen_for_values(rule, handle_value).await?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl Plugin for DarkmanPlugin {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn name(&self) -> &str {
        FEATURE_KEY
    }

    async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        self.create_feature_toggle_el();

        // Check if darkman DBus service is available and also fetch initial mode once.
        let initial_mode = match Self::get_current_state(&self.dbus).await {
            Ok(mode) => mode,
            Err(e) => {
                anyhow::bail!("darkman DBus service not available: {}", e);
            }
        };

        self.controller().apply_mode(initial_mode);

        // Start monitoring darkman
        self.start_monitoring().await?;

        self.initialized = true;
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        self.initialized = false;
        Ok(())
    }

    fn feature_toggles(&self) -> Vec<FeatureToggle> {
        vec![self.feature_toggle()]
    }

    fn widgets(&self) -> Vec<Widget> {
        vec![]
    }
}
