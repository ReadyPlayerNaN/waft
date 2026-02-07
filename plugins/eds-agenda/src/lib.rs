//! Agenda plugin — displays upcoming calendar events from Evolution Data Server.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, warn};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gtk::prelude::*;

use waft_core::dbus::DbusHandle;
use waft_core::menu_state::MenuStore;
use waft_plugin_api::{OverviewPlugin, PluginId, PluginResources, Widget, WidgetRegistrar, Slot};

use self::dbus::{
    ViewSignal, create_view, discover_calendar_sources, listen_view_signals, open_calendar,
    start_view, stop_and_dispose_view,
};
use self::store::{AgendaOp, AgendaStore, create_agenda_store};
use self::values::{
    AgendaConfig, AgendaPeriod, compute_time_range, format_time_range_query,
    parse_iso8601_duration, parse_period,
};
use self::widget::AgendaWidget;

mod dbus;
pub mod store;
mod ui;
pub mod values;
mod widget;

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("waft::eds-agenda", "Agenda", "0.1.0");
waft_plugin_api::export_overview_plugin!(AgendaPlugin::new());

/// Holds an active calendar view session for cleanup.
struct ActiveView {
    bus_name: String,
    view_path: String,
}

pub struct AgendaPlugin {
    dbus: Option<Arc<DbusHandle>>,
    tokio_handle: Option<tokio::runtime::Handle>,
    store: Rc<AgendaStore>,
    widget: Rc<RefCell<Option<AgendaWidget>>>,
    config: AgendaConfig,
    period: AgendaPeriod,
    lookahead: Option<chrono::Duration>,
    signal_channel: (flume::Sender<ViewSignal>, flume::Receiver<ViewSignal>),
    active_views: Rc<RefCell<Vec<ActiveView>>>,
    view_paths: Arc<Mutex<HashSet<String>>>,
}

impl Default for AgendaPlugin {
    fn default() -> Self {
        Self {
            dbus: None,
            tokio_handle: None,
            store: Rc::new(create_agenda_store()),
            widget: Rc::new(RefCell::new(None)),
            config: AgendaConfig::default(),
            period: AgendaPeriod::Today,
            lookahead: None,
            signal_channel: flume::unbounded(),
            active_views: Rc::new(RefCell::new(Vec::new())),
            view_paths: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

impl AgendaPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set up calendar views for all discovered sources.
    async fn setup_views(&self) -> Result<()> {
        let dbus = self.dbus.as_ref().expect("dbus not initialized");
        let handle = self.tokio_handle.as_ref().expect("tokio_handle not initialized");

        let sources = match discover_calendar_sources(dbus, handle).await {
            Ok(s) => s,
            Err(e) => {
                warn!("[agenda] Failed to discover calendar sources: {:?}", e);
                self.store
                    .emit(AgendaOp::SetError(Some("Calendar not available".into())));
                self.store.emit(AgendaOp::SetLoading(false));
                return Ok(());
            }
        };

        self.store.emit(AgendaOp::SetSources(sources.clone()));
        self.store.emit(AgendaOp::SetAvailable(true));

        debug!("[agenda] Found {} calendar source(s):", sources.len());
        for source in &sources {
            debug!(
                "[agenda]   - '{}' (uid: {})",
                source.display_name, source.uid
            );
        }

        if sources.is_empty() {
            debug!("[agenda] No calendar sources found");
            self.store.emit(AgendaOp::SetLoading(false));
            return Ok(());
        }

        let (since, until, next_period_start) =
            compute_time_range(&self.period, self.lookahead.as_ref());
        let query = format_time_range_query(since, until);
        debug!("[agenda] Query: {}", query);

        self.store
            .emit(AgendaOp::SetNextPeriodStart(next_period_start));
        self.store.emit(AgendaOp::SetQuerySince(since));

        // Clear previous views
        let views_to_stop: Vec<(String, String)> = self.active_views.borrow()
            .iter()
            .map(|view| (view.bus_name.clone(), view.view_path.clone()))
            .collect();

        for (bus_name, view_path) in views_to_stop {
            if let Err(e) = stop_and_dispose_view(dbus, handle, &bus_name, &view_path).await {
                debug!("[agenda] failed to stop/dispose view: {e}");
            }
        }
        self.active_views.borrow_mut().clear();
        match self.view_paths.lock() {
            Ok(mut paths) => paths.clear(),
            Err(e) => {
                warn!("[agenda] view_paths mutex poisoned, recovering: {e}");
                e.into_inner().clear();
            }
        }

        // Open calendars and create views
        for source in &sources {
            match open_calendar(dbus, handle, &source.uid).await {
                Ok((calendar_path, bus_name)) => {
                    match create_view(dbus, handle, &bus_name, &calendar_path, &query).await {
                        Ok(view_path) => {
                            if let Err(e) = start_view(dbus, handle, &bus_name, &view_path).await {
                                warn!(
                                    "[agenda] Failed to start view for '{}': {:?}",
                                    source.display_name, e
                                );
                                continue;
                            }
                            debug!(
                                "[agenda] View started for '{}' at {}",
                                source.display_name, view_path
                            );
                            match self.view_paths.lock() {
                                Ok(mut paths) => {
                                    paths.insert(view_path.clone());
                                }
                                Err(e) => {
                                    warn!("[agenda] view_paths mutex poisoned, recovering: {e}");
                                    e.into_inner().insert(view_path.clone());
                                }
                            }
                            self.active_views.borrow_mut().push(ActiveView {
                                bus_name,
                                view_path,
                            });
                        }
                        Err(e) => {
                            warn!(
                                "[agenda] Failed to create view for '{}': {:?}",
                                source.display_name, e
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "[agenda] Failed to open calendar '{}': {:?}",
                        source.display_name, e
                    );
                }
            }
        }

        self.store.emit(AgendaOp::SetLoading(false));
        Ok(())
    }
}

#[async_trait(?Send)]
impl OverviewPlugin for AgendaPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("waft::eds-agenda")
    }

    fn configure(&mut self, settings: &toml::Table) -> Result<()> {
        self.config = settings.clone().try_into()?;
        self.period = parse_period(&self.config.period)?;
        self.lookahead = if self.config.lookahead.is_empty() {
            None
        } else {
            match parse_iso8601_duration(&self.config.lookahead) {
                Ok(dur) => Some(dur),
                Err(e) => {
                    warn!(
                        "[agenda] Invalid lookahead '{}': {:?}",
                        self.config.lookahead, e
                    );
                    None
                }
            }
        };
        debug!(
            "[agenda] Configured: {:?}, lookahead: {:?}",
            self.config, self.lookahead
        );
        Ok(())
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        debug!("[agenda] init() called");

        let tokio_handle = resources
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
        let _guard = tokio_handle.enter();
        self.tokio_handle = Some(tokio_handle.clone());

        // Use the session dbus connection provided by the host
        let dbus = resources
            .session_dbus
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session_dbus not provided"))?
            .clone();
        debug!("[agenda] Received dbus connection from host");

        self.dbus = Some(dbus.clone());
        self.store.emit(AgendaOp::SetLoading(true));

        // Start listening for view signals before setting up views
        listen_view_signals(
            &dbus,
            self.signal_channel.0.clone(),
            self.view_paths.clone(),
            tokio_handle,
        )
        .await?;

        // Set up views (discover sources, open calendars, etc.)
        self.setup_views().await?;

        debug!("[agenda] init() completed successfully");
        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let _guard = self.tokio_handle.as_ref().map(|h| h.enter());
        let agenda_widget = AgendaWidget::new(menu_store, self.store.clone());

        // Initial render
        {
            let state = self.store.get_state();
            agenda_widget.update(&state);
        }

        // Register the widget
        registrar.register_widget(Rc::new(Widget {
            id: "agenda:main".to_string(),
            slot: Slot::Info,
            el: agenda_widget.root.clone().upcast::<gtk::Widget>(),
            weight: 30,
        }));

        *self.widget.borrow_mut() = Some(agenda_widget);

        // Subscribe to store changes
        let widget_ref = self.widget.clone();
        let store = self.store.clone();
        self.store.subscribe(move || {
            let state = store.get_state();
            if let Some(ref widget) = *widget_ref.borrow() {
                widget.update(&state);
            }
        });

        // Consume view signals from D-Bus on the main thread
        let store_for_signals = self.store.clone();
        let signal_rx = self.signal_channel.1.clone();
        glib::spawn_future_local(async move {
            while let Ok(signal) = signal_rx.recv_async().await {
                match signal {
                    ViewSignal::Added(events) => {
                        store_for_signals.emit(AgendaOp::UpsertEvents(events));
                    }
                    ViewSignal::Modified(events) => {
                        let uids: Vec<String> =
                            events.iter().map(|e| e.uid.clone()).collect();
                        store_for_signals.emit(AgendaOp::RemoveEvents(uids));
                        store_for_signals.emit(AgendaOp::UpsertEvents(events));
                    }
                    ViewSignal::Removed(uids) => {
                        store_for_signals.emit(AgendaOp::RemoveEvents(uids));
                    }
                }
            }
            warn!("[agenda] signal receiver loop exited — widget is now unresponsive");
        });

        // Periodic refresh: recreate views with updated time range
        let dbus = self.dbus.clone().expect("dbus not initialized");
        let tokio_handle = self.tokio_handle.clone().expect("tokio_handle not initialized");
        let store_for_refresh = self.store.clone();
        let active_views = self.active_views.clone();
        let view_paths_for_refresh = self.view_paths.clone();
        let period = self.period.clone();
        let lookahead = self.lookahead;
        let refresh_interval = self.config.refresh_interval;

        glib::timeout_add_local(Duration::from_secs(refresh_interval), move || {
            let dbus = dbus.clone();
            let handle = tokio_handle.clone();
            let store = store_for_refresh.clone();
            let active_views = active_views.clone();
            let view_paths = view_paths_for_refresh.clone();
            let period = period.clone();

            glib::spawn_future_local(async move {
                debug!("[agenda] Periodic refresh");
                store.emit(AgendaOp::SetLoading(true));

                let sources = match discover_calendar_sources(&dbus, &handle).await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("[agenda] Refresh: failed to discover sources: {:?}", e);
                        store.emit(AgendaOp::SetLoading(false));
                        return;
                    }
                };

                store.emit(AgendaOp::SetSources(sources.clone()));

                // Clean up old views
                let views_to_stop: Vec<(String, String)> = active_views.borrow()
                    .iter()
                    .map(|view| (view.bus_name.clone(), view.view_path.clone()))
                    .collect();

                for (bus_name, view_path) in views_to_stop {
                    if let Err(e) = stop_and_dispose_view(&dbus, &handle, &bus_name, &view_path).await {
                        debug!("[agenda] refresh: failed to stop/dispose view: {e}");
                    }
                }
                active_views.borrow_mut().clear();
                match view_paths.lock() {
                    Ok(mut paths) => paths.clear(),
                    Err(e) => {
                        warn!("[agenda] view_paths mutex poisoned during refresh, recovering: {e}");
                        e.into_inner().clear();
                    }
                }

                let (since, until, next_period_start) =
                    compute_time_range(&period, lookahead.as_ref());
                let query = format_time_range_query(since, until);
                store.emit(AgendaOp::SetNextPeriodStart(next_period_start));
                store.emit(AgendaOp::SetQuerySince(since));

                for source in &sources {
                    match open_calendar(&dbus, &handle, &source.uid).await {
                        Ok((calendar_path, bus_name)) => {
                            match create_view(&dbus, &handle, &bus_name, &calendar_path, &query).await {
                                Ok(view_path) => {
                                    if let Err(e) = start_view(&dbus, &handle, &bus_name, &view_path).await {
                                        warn!(
                                            "[agenda] Refresh: failed to start view for '{}': {:?}",
                                            source.display_name, e
                                        );
                                        continue;
                                    }
                                    match view_paths.lock() {
                                        Ok(mut paths) => {
                                            paths.insert(view_path.clone());
                                        }
                                        Err(e) => {
                                            warn!(
                                                "[agenda] view_paths mutex poisoned during refresh, recovering: {e}"
                                            );
                                            e.into_inner().insert(view_path.clone());
                                        }
                                    }
                                    active_views.borrow_mut().push(ActiveView {
                                        bus_name,
                                        view_path,
                                    });
                                }
                                Err(e) => {
                                    warn!(
                                        "[agenda] Refresh: failed to create view for '{}': {:?}",
                                        source.display_name, e
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                "[agenda] Refresh: failed to open calendar '{}': {:?}",
                                source.display_name, e
                            );
                        }
                    }
                }

                store.emit(AgendaOp::SetLoading(false));
            });

            glib::ControlFlow::Continue
        });

        Ok(())
    }
}
