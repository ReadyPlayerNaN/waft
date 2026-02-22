//! Launcher application setup.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use adw::prelude::*;
use waft_client::{ClientEvent, EntityStore, WaftClient, daemon_connection_task};
use waft_config::Config;
use waft_protocol::entity;
use waft_protocol::entity::app::App;
use waft_protocol::urn::Urn;
use waft_ui_gtk::widgets::search_pane::SearchPaneOutput;

use crate::ranking::rank_apps;
use crate::usage::{load_usage, record_launch_in, save_usage_to, usage_file_path};
use crate::window::LauncherWindow;

const ENTITY_TYPES: &[&str] = &[entity::app::ENTITY_TYPE];

pub fn run() -> anyhow::Result<()> {
    let config = Config::load();
    let rank_by_usage = config.launcher.rank_by_usage;
    let max_results = config.launcher.max_results;

    // Channels for daemon communication
    let (event_tx, event_rx) = flume::unbounded::<ClientEvent>();
    let client_handle: Arc<Mutex<Option<WaftClient>>> = Arc::new(Mutex::new(None));

    // Spawn tokio runtime for daemon connection
    let rt = tokio::runtime::Runtime::new()?;
    let client_for_task = client_handle.clone();
    rt.spawn(async move {
        daemon_connection_task(event_tx, client_for_task, ENTITY_TYPES).await;
        log::debug!("[launcher] daemon connection task exited");
    });

    // Action writer thread (GTK -> daemon, bypasses tokio)
    let (action_tx, action_rx) =
        std::sync::mpsc::channel::<(Urn, String, serde_json::Value)>();
    let client_for_writer = client_handle.clone();
    std::thread::spawn(move || {
        while let Ok((urn, action, params)) = action_rx.recv() {
            let guard = match client_for_writer.lock() {
                Ok(g) => g,
                Err(e) => {
                    log::warn!("[launcher] client handle poisoned: {e}");
                    e.into_inner()
                }
            };
            if let Some(ref client) = *guard {
                client.trigger_action(urn, &action, params);
            }
        }
        log::debug!("[launcher] action writer thread exiting");
    });

    let app = adw::Application::builder()
        .application_id("com.waft.launcher")
        .build();

    // Wrap one-shot values in slots so they can be taken inside connect_startup.
    // connect_startup requires Fn (not FnOnce) but fires exactly once.
    let event_rx_slot: Rc<RefCell<Option<flume::Receiver<ClientEvent>>>> =
        Rc::new(RefCell::new(Some(event_rx)));
    let action_tx_slot: Rc<RefCell<Option<std::sync::mpsc::Sender<(Urn, String, serde_json::Value)>>>> =
        Rc::new(RefCell::new(Some(action_tx)));

    app.connect_startup(move |app| {
        let Some(event_rx) = event_rx_slot.borrow_mut().take() else {
            return;
        };
        let Some(action_tx) = action_tx_slot.borrow_mut().take() else {
            return;
        };

        // Load launcher stylesheet
        let provider = gtk::CssProvider::new();
        provider.load_from_data(include_str!("../style.css"));
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        } else {
            log::warn!("[launcher] no display available; stylesheet not loaded");
        }

        let entity_store = Rc::new(EntityStore::new());
        let launcher_win = LauncherWindow::new(app);
        // Show loading spinner immediately; cleared once the first entities arrive.
        launcher_win.search_pane().set_loading(true);

        let win = Rc::new(launcher_win);
        let current_query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

        // connect_activate fires on first launch (after startup) and on every
        // subsequent invocation of `waft-launcher` while this process is running.
        // It shows the window and resets search state, acting as the "open" trigger.
        {
            let win_for_activate = win.clone();
            let query_for_activate = current_query.clone();
            let store_for_activate = entity_store.clone();
            app.connect_activate(move |_| {
                // Reset query and search entry text
                *query_for_activate.borrow_mut() = String::new();
                win_for_activate.reset();
                // Populate results immediately if entities are already in store;
                // this also clears the loading spinner when data is present.
                update_results(&win_for_activate, &store_for_activate, "", rank_by_usage, max_results);
                win_for_activate.window.present();
                win_for_activate.grab_focus();
            });
        }

        // Connect search pane output
        {
            let win_ref = win.clone();
            let store_ref = Rc::clone(&entity_store);
            let query_ref = current_query.clone();
            let action_tx = action_tx.clone();
            win.search_pane().connect_output(move |event| match event {
                SearchPaneOutput::QueryChanged(query) => {
                    *query_ref.borrow_mut() = query.clone();
                    update_results(&win_ref, &store_ref, &query, rank_by_usage, max_results);
                }
                SearchPaneOutput::QueryActivated => {
                    let idx = win_ref.search_pane().selected_index().unwrap_or(0);
                    if let Some((urn, _)) = win_ref.result_at(idx) {
                        launch_app(&action_tx, &urn);
                        win_ref.window.hide();
                    }
                }
                SearchPaneOutput::ResultActivated(index) => {
                    if let Some((urn, _)) = win_ref.result_at(index) {
                        launch_app(&action_tx, &urn);
                        win_ref.window.hide();
                    }
                }
                SearchPaneOutput::ResultSelected(_) => {} // selection tracked internally
                SearchPaneOutput::Stopped => {
                    win_ref.window.hide();
                }
            });
        }

        // Entity store subscription -- rebuild results on any app entity change
        {
            let win_ref = win.clone();
            let store_ref = Rc::clone(&entity_store);
            let query_ref = current_query.clone();
            entity_store.subscribe_type(entity::app::ENTITY_TYPE, move || {
                let query = query_ref.borrow().clone();
                update_results(&win_ref, &store_ref, &query, rank_by_usage, max_results);
            });
        }

        // Initial reconciliation
        {
            let win_ref = win.clone();
            let store_ref = Rc::clone(&entity_store);
            gtk::glib::idle_add_local_once(move || {
                update_results(&win_ref, &store_ref, "", rank_by_usage, max_results);
            });
        }

        // Receive daemon events on glib main loop
        let store_for_events = Rc::clone(&entity_store);
        gtk::glib::spawn_future_local(async move {
            while let Ok(event) = event_rx.recv_async().await {
                match event {
                    ClientEvent::Notification(notification) => {
                        store_for_events.handle_notification(notification);
                    }
                    ClientEvent::Connected => {
                        log::info!("[launcher] connected to daemon");
                    }
                    ClientEvent::Disconnected => {
                        log::info!("[launcher] disconnected from daemon");
                    }
                }
            }
            log::warn!("[launcher] event receiver loop exited -- launcher is now unresponsive");
        });
    });

    let exit_code = app.run();
    std::process::exit(exit_code.into());
}

fn update_results(
    win: &LauncherWindow,
    store: &EntityStore,
    query: &str,
    rank_by_usage: bool,
    max_results: usize,
) {
    let all_apps: Vec<(Urn, App)> = store.get_entities_typed(entity::app::ENTITY_TYPE);
    let usage = load_usage();
    let mut ranked = rank_apps(&all_apps, query, &usage, rank_by_usage);
    ranked.truncate(max_results);
    // Clear the loading spinner only once real entity data has arrived from the
    // daemon. An empty store with an empty query is still "loading", not "ready".
    if !all_apps.is_empty() {
        win.search_pane().set_loading(false);
    }
    win.set_results(ranked, query);
}

fn launch_app(tx: &std::sync::mpsc::Sender<(Urn, String, serde_json::Value)>, urn: &Urn) {
    // Record usage
    let mut usage = load_usage();
    record_launch_in(&mut usage, &urn.to_string());
    if let Err(e) = save_usage_to(&usage_file_path(), &usage) {
        log::warn!("[launcher] failed to save usage data: {e}");
    }

    // Dispatch open action through daemon
    if let Err(e) = tx.send((urn.clone(), "open".to_string(), serde_json::Value::Null)) {
        log::warn!("[launcher] failed to send open action: {e}");
    }
}
