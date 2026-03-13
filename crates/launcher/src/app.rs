//! Launcher application setup.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use adw::prelude::*;
use waft_client::{ClientEvent, EntityStore, WaftClient, daemon_connection_task};
use waft_config::Config;
use waft_protocol::entity;
use waft_protocol::urn::Urn;
use waft_ui_gtk::widgets::search_pane::SearchPaneOutput;

use crate::command_index::CommandIndex;
use waft_protocol::commands::command_entity_types;
use crate::ranking::{RankedResult, rank_commands, rank_results};
use crate::search_index::SearchIndex;
use crate::usage::{UsageMap, load_usage, record_launch_in, save_usage_to, usage_file_path};
use crate::window::LauncherWindow;

#[derive(Clone, Copy, PartialEq)]
enum LauncherMode {
    Normal,
    CommandPalette,
}

impl LauncherMode {
    fn prefix(self) -> &'static str {
        match self {
            LauncherMode::Normal => "",
            LauncherMode::CommandPalette => "> ",
        }
    }
}

type ActionSender = Rc<RefCell<Option<std::sync::mpsc::Sender<(Urn, String, serde_json::Value)>>>>;

const ENTITY_TYPES: &[&str] = &[
    entity::app::ENTITY_TYPE,
    entity::window::ENTITY_TYPE,
    // Command palette entity types
    entity::session::SESSION_ENTITY_TYPE,
    entity::display::DARK_MODE_ENTITY_TYPE,
    entity::display::NIGHT_LIGHT_ENTITY_TYPE,
    entity::session::SLEEP_INHIBITOR_ENTITY_TYPE,
    entity::notification::DND_ENTITY_TYPE,
    entity::notification::RECORDING_ENTITY_TYPE,
    entity::bluetooth::BluetoothDevice::ENTITY_TYPE,
    entity::network::VPN_ENTITY_TYPE,
    entity::storage::BACKUP_METHOD_ENTITY_TYPE,
];

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
        .flags(gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    app.add_main_option(
        "command",
        'c'.try_into().unwrap(),
        gtk::glib::OptionFlags::NONE,
        gtk::glib::OptionArg::None,
        "Open in command palette mode",
        None,
    );

    let requested_mode: Rc<Cell<LauncherMode>> = Rc::new(Cell::new(LauncherMode::Normal));

    {
        let mode_for_cmdline = requested_mode.clone();
        app.connect_command_line(move |app, cmdline| {
            let is_command = cmdline.options_dict().contains("command");
            mode_for_cmdline.set(if is_command {
                LauncherMode::CommandPalette
            } else {
                LauncherMode::Normal
            });
            app.activate();
            0.into()
        });
    }

    // Wrap one-shot values in slots so they can be taken inside connect_startup.
    // connect_startup requires Fn (not FnOnce) but fires exactly once.
    let event_rx_slot: Rc<RefCell<Option<flume::Receiver<ClientEvent>>>> =
        Rc::new(RefCell::new(Some(event_rx)));
    let action_tx_slot: ActionSender =
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
        let usage_cache: Rc<RefCell<UsageMap>> = Rc::new(RefCell::new(load_usage()));
        let search_index: Rc<RefCell<SearchIndex>> = Rc::new(RefCell::new(SearchIndex::new()));
        let command_index: Rc<RefCell<CommandIndex>> = Rc::new(RefCell::new(CommandIndex::new()));

        // connect_activate fires on every invocation of `waft-launcher` (first and
        // subsequent). It handles show/hide/mode-switch based on requested_mode.
        {
            let win_for_activate = win.clone();
            let query_for_activate = current_query.clone();
            let index_for_activate = search_index.clone();
            let cmd_index_for_activate = command_index.clone();
            let usage_for_activate = usage_cache.clone();
            let mode_for_activate = requested_mode.clone();
            app.connect_activate(move |_| {
                let mode = mode_for_activate.get();
                let prefix = mode.prefix();

                if win_for_activate.window.is_visible() {
                    if win_for_activate.is_animating_hide() {
                        // Mid hide-animation: reverse back to visible in requested mode.
                        win_for_activate.show();
                        apply_launcher_mode(&win_for_activate, prefix, &query_for_activate, &index_for_activate, &cmd_index_for_activate, &usage_for_activate, rank_by_usage, max_results);
                        win_for_activate.grab_focus();
                    } else {
                        // Determine current mode from entry text prefix.
                        let current_is_command = win_for_activate
                            .search_pane()
                            .search_bar
                            .text()
                            .starts_with('>');
                        let same_mode =
                            current_is_command == (mode == LauncherMode::CommandPalette);
                        if same_mode {
                            // Same mode: toggle off.
                            win_for_activate.hide();
                        } else {
                            // Different mode: switch without hiding.
                            apply_launcher_mode(&win_for_activate, prefix, &query_for_activate, &index_for_activate, &cmd_index_for_activate, &usage_for_activate, rank_by_usage, max_results);
                            win_for_activate.grab_focus();
                        }
                    }
                    return;
                }

                // Fully hidden: reset and open fresh in requested mode.
                *query_for_activate.borrow_mut() = String::new();
                win_for_activate.reset();
                apply_launcher_mode(&win_for_activate, prefix, &query_for_activate, &index_for_activate, &cmd_index_for_activate, &usage_for_activate, rank_by_usage, max_results);
                win_for_activate.show();
                win_for_activate.grab_focus();
            });
        }

        // Connect search pane output
        {
            let win_ref = win.clone();
            let index_ref = search_index.clone();
            let cmd_index_ref = command_index.clone();
            let query_ref = current_query.clone();
            let action_tx = action_tx.clone();
            let usage_for_output = usage_cache.clone();
            win.search_pane().connect_output(move |event| match event {
                SearchPaneOutput::QueryChanged(query) => {
                    *query_ref.borrow_mut() = query.clone();
                    update_results(
                        &win_ref,
                        &index_ref.borrow(),
                        &cmd_index_ref.borrow(),
                        &query,
                        &usage_for_output.borrow(),
                        rank_by_usage,
                        max_results,
                    );
                }
                SearchPaneOutput::QueryActivated => {
                    let idx = win_ref.search_pane().selected_index().unwrap_or(0);
                    activate_result(&win_ref, idx, &usage_for_output, &action_tx);
                }
                SearchPaneOutput::ResultActivated(index) => {
                    activate_result(&win_ref, index, &usage_for_output, &action_tx);
                }
                SearchPaneOutput::ResultSelected(_) => {} // selection tracked internally
                SearchPaneOutput::Stopped => {
                    win_ref.hide();
                }
            });
        }

        // Entity store subscriptions -- rebuild search index, then re-rank
        {
            let win_ref = win.clone();
            let store_ref = Rc::clone(&entity_store);
            let index_ref = search_index.clone();
            let cmd_index_ref = command_index.clone();
            let query_ref = current_query.clone();
            let usage_for_subscribe = usage_cache.clone();
            entity_store.subscribe_type(entity::app::ENTITY_TYPE, move || {
                index_ref.borrow_mut().rebuild_apps(&store_ref);
                let query = query_ref.borrow().clone();
                update_results(&win_ref, &index_ref.borrow(), &cmd_index_ref.borrow(), &query, &usage_for_subscribe.borrow(), rank_by_usage, max_results);
            });
        }
        {
            let win_ref = win.clone();
            let store_ref = Rc::clone(&entity_store);
            let index_ref = search_index.clone();
            let cmd_index_ref = command_index.clone();
            let query_ref = current_query.clone();
            let usage_for_subscribe = usage_cache.clone();
            entity_store.subscribe_type(entity::window::ENTITY_TYPE, move || {
                index_ref.borrow_mut().rebuild_windows(&store_ref);
                let query = query_ref.borrow().clone();
                update_results(&win_ref, &index_ref.borrow(), &cmd_index_ref.borrow(), &query, &usage_for_subscribe.borrow(), rank_by_usage, max_results);
            });
        }

        // Command entity type subscriptions -- rebuild command index, then re-rank
        for &entity_type in command_entity_types() {
            let win_ref = win.clone();
            let store_ref = Rc::clone(&entity_store);
            let index_ref = search_index.clone();
            let cmd_index_ref = command_index.clone();
            let query_ref = current_query.clone();
            let usage_for_subscribe = usage_cache.clone();
            entity_store.subscribe_type(entity_type, move || {
                cmd_index_ref.borrow_mut().rebuild(&store_ref);
                let query = query_ref.borrow().clone();
                update_results(&win_ref, &index_ref.borrow(), &cmd_index_ref.borrow(), &query, &usage_for_subscribe.borrow(), rank_by_usage, max_results);
            });
        }

        // Initial reconciliation
        {
            let win_ref = win.clone();
            let store_ref = Rc::clone(&entity_store);
            let index_ref = search_index.clone();
            let cmd_index_ref = command_index.clone();
            let usage_for_init = usage_cache.clone();
            let query_for_init = current_query.clone();
            gtk::glib::idle_add_local_once(move || {
                {
                    let mut idx = index_ref.borrow_mut();
                    idx.rebuild_apps(&store_ref);
                    idx.rebuild_windows(&store_ref);
                }
                cmd_index_ref.borrow_mut().rebuild(&store_ref);
                let query = query_for_init.borrow().clone();
                update_results(&win_ref, &index_ref.borrow(), &cmd_index_ref.borrow(), &query, &usage_for_init.borrow(), rank_by_usage, max_results);
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
    index: &SearchIndex,
    command_idx: &CommandIndex,
    query: &str,
    usage: &UsageMap,
    rank_by_usage: bool,
    max_results: usize,
) {
    let ranked = if let Some(cmd_query) = query.strip_prefix('>') {
        let cmd_query = cmd_query.trim_start();
        rank_commands(command_idx, cmd_query, max_results)
    } else {
        rank_results(index, query, usage, rank_by_usage, max_results)
    };
    // Clear the loading spinner only once real entity data has arrived from the
    // daemon. An empty store with an empty query is still "loading", not "ready".
    if !index.is_empty() {
        win.search_pane().set_loading(false);
    }
    win.set_results(ranked, query);
}

fn apply_launcher_mode(
    win: &LauncherWindow,
    prefix: &str,
    query: &Rc<RefCell<String>>,
    index: &Rc<RefCell<SearchIndex>>,
    cmd_index: &Rc<RefCell<CommandIndex>>,
    usage: &Rc<RefCell<UsageMap>>,
    rank_by_usage: bool,
    max_results: usize,
) {
    *query.borrow_mut() = prefix.to_string();
    win.search_pane().search_bar.set_text(prefix);
    update_results(win, &index.borrow(), &cmd_index.borrow(), prefix, &usage.borrow(), rank_by_usage, max_results);
}

fn activate_result(
    win: &LauncherWindow,
    index: usize,
    usage: &Rc<RefCell<UsageMap>>,
    tx: &std::sync::mpsc::Sender<(Urn, String, serde_json::Value)>,
) {
    let Some(result) = win.result_at(index) else {
        return;
    };

    match &result {
        RankedResult::App { urn, .. } => {
            launch_app(usage, tx, urn);
        }
        RankedResult::Window { urn, .. } => {
            focus_window(tx, urn);
        }
        RankedResult::Command { urn, action, .. } => {
            if let Err(e) = tx.send((urn.clone(), action.clone(), serde_json::Value::Null)) {
                log::warn!("[launcher] failed to send command action: {e}");
            }
        }
    }
    win.hide();
}

fn launch_app(
    usage: &Rc<RefCell<UsageMap>>,
    tx: &std::sync::mpsc::Sender<(Urn, String, serde_json::Value)>,
    urn: &Urn,
) {
    // Record usage in cache and persist to disk
    {
        let mut u = usage.borrow_mut();
        record_launch_in(&mut u, &urn.to_string());
        if let Err(e) = save_usage_to(&usage_file_path(), &u) {
            log::warn!("[launcher] failed to save usage data: {e}");
        }
    }

    // Dispatch open action through daemon
    if let Err(e) = tx.send((urn.clone(), "open".to_string(), serde_json::Value::Null)) {
        log::warn!("[launcher] failed to send open action: {e}");
    }
}

fn focus_window(
    tx: &std::sync::mpsc::Sender<(Urn, String, serde_json::Value)>,
    urn: &Urn,
) {
    if let Err(e) = tx.send((urn.clone(), "focus".to_string(), serde_json::Value::Null)) {
        log::warn!("[launcher] failed to send focus action: {e}");
    }
}
