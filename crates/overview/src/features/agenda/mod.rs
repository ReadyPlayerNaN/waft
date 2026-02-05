//! Agenda plugin — displays upcoming calendar events from Evolution Data Server.
use crate::menu_state::MenuStore;

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, warn};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gtk::prelude::*;

use crate::dbus::DbusHandle;
use crate::plugin::{Plugin, PluginId, Slot, Widget, WidgetRegistrar};

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

/// Holds an active calendar view session for cleanup.
struct ActiveView {
    bus_name: String,
    view_path: String,
}

pub struct AgendaPlugin {
    dbus: Arc<DbusHandle>,
    store: Rc<AgendaStore>,
    widget: Rc<RefCell<Option<AgendaWidget>>>,
    config: AgendaConfig,
    period: AgendaPeriod,
    lookahead: Option<chrono::Duration>,
    signal_channel: (flume::Sender<ViewSignal>, flume::Receiver<ViewSignal>),
    active_views: Rc<RefCell<Vec<ActiveView>>>,
    /// Thread-safe set of our active view object paths, shared with the D-Bus signal listener
    /// so it can ignore signals from views created by other applications.
    view_paths: Arc<Mutex<HashSet<String>>>,
}

impl AgendaPlugin {
    pub fn new(dbus: Arc<DbusHandle>) -> Self {
        Self {
            dbus,
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

    /// Set up calendar views for all discovered sources.
    async fn setup_views(&self) -> Result<()> {
        let sources = match discover_calendar_sources(&self.dbus).await {
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
        {
            let views = self.active_views.borrow();
            for view in views.iter() {
                if let Err(e) =
                    stop_and_dispose_view(&self.dbus, &view.bus_name, &view.view_path).await
                {
                    debug!("[agenda] failed to stop/dispose view: {e}");
                }
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
        // Don't clear events here - let them remain visible while new views are being created.
        // New events will arrive via D-Bus signals and incrementally replace old ones.
        // self.store.emit(AgendaOp::ClearEvents);

        // Open calendars and create views
        for source in &sources {
            match open_calendar(&self.dbus, &source.uid).await {
                Ok((calendar_path, bus_name)) => {
                    match create_view(&self.dbus, &bus_name, &calendar_path, &query).await {
                        Ok(view_path) => {
                            if let Err(e) = start_view(&self.dbus, &bus_name, &view_path).await {
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
impl Plugin for AgendaPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::agenda")
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

    async fn init(&mut self) -> Result<()> {
        self.store.emit(AgendaOp::SetLoading(true));

        // Start listening for view signals before setting up views
        listen_view_signals(
            &self.dbus,
            self.signal_channel.0.clone(),
            self.view_paths.clone(),
        )
        .await?;

        // Set up views (discover sources, open calendars, etc.)
        self.setup_views().await?;

        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        menu_store: Arc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let agenda_widget = AgendaWidget::new(menu_store);

        // Initial render
        {
            let state = self.store.get_state();
            agenda_widget.update(&state);
        }

        // Register the widget
        registrar.register_widget(Arc::new(Widget {
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
                    ViewSignal::EventsAdded(events) => {
                        store_for_signals.emit(AgendaOp::UpsertEvents(events));
                    }
                    ViewSignal::EventsModified(events) => {
                        // When an event is modified (e.g. time changed), we need to remove
                        // old occurrences first. Otherwise, if the start_time changed, the
                        // occurrence_key changes and we end up with duplicate entries.
                        let uids: Vec<String> =
                            events.iter().map(|e| e.uid.clone()).collect();
                        store_for_signals.emit(AgendaOp::RemoveEvents(uids));
                        store_for_signals.emit(AgendaOp::UpsertEvents(events));
                    }
                    ViewSignal::EventsRemoved(uids) => {
                        store_for_signals.emit(AgendaOp::RemoveEvents(uids));
                    }
                }
            }
            warn!("[agenda] signal receiver loop exited — widget is now unresponsive");
        });

        // Periodic refresh: recreate views with updated time range
        let dbus = self.dbus.clone();
        let store_for_refresh = self.store.clone();
        let active_views = self.active_views.clone();
        let view_paths_for_refresh = self.view_paths.clone();
        let period = self.period.clone();
        let lookahead = self.lookahead;
        let refresh_interval = self.config.refresh_interval;

        glib::timeout_add_local(Duration::from_secs(refresh_interval), move || {
            let dbus = dbus.clone();
            let store = store_for_refresh.clone();
            let active_views = active_views.clone();
            let view_paths = view_paths_for_refresh.clone();
            let period = period.clone();

            glib::spawn_future_local(async move {
                debug!("[agenda] Periodic refresh");
                store.emit(AgendaOp::SetLoading(true));

                let sources = match discover_calendar_sources(&dbus).await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("[agenda] Refresh: failed to discover sources: {:?}", e);
                        store.emit(AgendaOp::SetLoading(false));
                        return;
                    }
                };

                store.emit(AgendaOp::SetSources(sources.clone()));

                // Clean up old views
                {
                    let views = active_views.borrow();
                    for view in views.iter() {
                        if let Err(e) =
                            stop_and_dispose_view(&dbus, &view.bus_name, &view.view_path).await
                        {
                            debug!("[agenda] refresh: failed to stop/dispose view: {e}");
                        }
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
                // Don't clear events here - let them remain visible while new views are being created.
                // New events will arrive via D-Bus signals and incrementally replace old ones.
                // store.emit(AgendaOp::ClearEvents);

                let (since, until, next_period_start) =
                    compute_time_range(&period, lookahead.as_ref());
                let query = format_time_range_query(since, until);
                store.emit(AgendaOp::SetNextPeriodStart(next_period_start));
                store.emit(AgendaOp::SetQuerySince(since));

                for source in &sources {
                    match open_calendar(&dbus, &source.uid).await {
                        Ok((calendar_path, bus_name)) => {
                            match create_view(&dbus, &bus_name, &calendar_path, &query).await {
                                Ok(view_path) => {
                                    if let Err(e) = start_view(&dbus, &bus_name, &view_path).await {
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
