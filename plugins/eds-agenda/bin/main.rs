//! Agenda daemon — displays upcoming calendar events from Evolution Data Server.

use anyhow::{Context, Result};
use async_trait::async_trait;
use log::{debug, error, warn};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex};

use waft_core::dbus::DbusHandle;
use waft_ipc::{Action, NamedWidget, Widget};
use waft_plugin_sdk::builder::*;
use waft_plugin_sdk::{PluginDaemon, PluginServer};

use waft_plugin_agenda::values::{
    AgendaEvent, AgendaPeriod, CalendarSource, compute_time_range, extract_meeting_links,
    format_time_range_query, parse_period, remove_events_by_uids, AgendaConfig, MeetingProvider,
};

mod dbus;
use dbus::{
    ViewSignal, create_view, discover_calendar_sources, listen_view_signals, open_calendar,
    start_view, stop_and_dispose_view,
};

/// Lock a mutex, recovering from poison.
fn lock<T>(mutex: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(e) => {
            warn!("[agenda] mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    }
}

struct ActiveView {
    bus_name: String,
    view_path: String,
}

struct AgendaState {
    events: BTreeMap<String, AgendaEvent>,
    sources: Vec<CalendarSource>,
    available: bool,
    loading: bool,
    error: Option<String>,
    show_past: bool,
    query_since: Option<i64>,
}

impl Default for AgendaState {
    fn default() -> Self {
        Self {
            events: BTreeMap::new(),
            sources: Vec::new(),
            available: false,
            loading: false,
            error: None,
            show_past: true,
            query_since: None,
        }
    }
}

pub struct AgendaDaemon {
    dbus: Arc<DbusHandle>,
    state: Arc<StdMutex<AgendaState>>,
    period: AgendaPeriod,
    lookahead: Option<chrono::Duration>,
    active_views: Arc<StdMutex<Vec<ActiveView>>>,
    view_paths: Arc<StdMutex<HashSet<String>>>,
}

impl AgendaDaemon {
    async fn setup_views(&self) -> Result<()> {
        let sources = match discover_calendar_sources(&self.dbus).await {
            Ok(s) => s,
            Err(e) => {
                warn!("[agenda] failed to discover calendar sources: {e:?}");
                let mut state = lock(&self.state);
                state.error = Some("Calendar not available".into());
                state.loading = false;
                return Ok(());
            }
        };

        {
            let mut state = lock(&self.state);
            state.sources = sources.clone();
            state.available = true;
        }

        debug!("[agenda] found {} calendar source(s)", sources.len());
        for source in &sources {
            debug!("[agenda]   - '{}' (uid: {})", source.display_name, source.uid);
        }

        if sources.is_empty() {
            lock(&self.state).loading = false;
            return Ok(());
        }

        let (since, until, _next_period_start) =
            compute_time_range(&self.period, self.lookahead.as_ref());
        let query = format_time_range_query(since, until);
        debug!("[agenda] query: {query}");

        lock(&self.state).query_since = Some(since);

        // Stop previous views
        let views_to_stop: Vec<(String, String)> = lock(&self.active_views)
            .iter()
            .map(|v| (v.bus_name.clone(), v.view_path.clone()))
            .collect();

        for (bus_name, view_path) in views_to_stop {
            if let Err(e) = stop_and_dispose_view(&self.dbus, &bus_name, &view_path).await {
                debug!("[agenda] failed to stop/dispose view: {e}");
            }
        }
        lock(&self.active_views).clear();
        lock(&self.view_paths).clear();

        // Open calendars and create views (but don't start yet — that triggers
        // ObjectsAdded signals, and the consumer pipeline must be ready first).
        for source in &sources {
            match open_calendar(&self.dbus, &source.uid).await {
                Ok((calendar_path, bus_name)) => {
                    match create_view(&self.dbus, &bus_name, &calendar_path, &query).await {
                        Ok(view_path) => {
                            debug!(
                                "[agenda] view created for '{}' at {view_path}",
                                source.display_name
                            );
                            lock(&self.view_paths).insert(view_path.clone());
                            lock(&self.active_views).push(ActiveView {
                                bus_name,
                                view_path,
                            });
                        }
                        Err(e) => warn!(
                            "[agenda] failed to create view for '{}': {e:?}",
                            source.display_name
                        ),
                    }
                }
                Err(e) => warn!(
                    "[agenda] failed to open calendar '{}': {e:?}",
                    source.display_name
                ),
            }
        }

        Ok(())
    }

    fn build_widgets(&self, state: &AgendaState) -> Vec<NamedWidget> {
        build_widgets(state)
    }
}

fn build_widgets(state: &AgendaState) -> Vec<NamedWidget> {
    let mut widgets = Vec::new();

    // Header: title + show-past toggle
    let header = RowBuilder::new()
        .spacing(8)
        .child(LabelBuilder::new("Agenda").css_class("title-3").build())
        .child(
            ToggleButtonBuilder::new("task-past-due-symbolic")
                .active(state.show_past)
                .on_toggle("toggle_past")
                .build(),
        )
        .build();

    widgets.push(NamedWidget {
        id: "eds-agenda:header".into(),
        weight: 30,
        widget: header,
    });

    if state.loading {
        widgets.push(NamedWidget {
            id: "eds-agenda:loading".into(),
            weight: 31,
            widget: Widget::Spinner { spinning: true },
        });
        return widgets;
    }

    if let Some(ref error) = state.error {
        widgets.push(NamedWidget {
            id: "eds-agenda:error".into(),
            weight: 31,
            widget: LabelBuilder::new(error).css_class("error").build(),
        });
        return widgets;
    }

    if state.events.is_empty() {
        widgets.push(NamedWidget {
            id: "eds-agenda:empty".into(),
            weight: 31,
            widget: LabelBuilder::new("No upcoming events")
                .css_class("dim-label")
                .build(),
        });
        return widgets;
    }

    let now = chrono::Local::now().timestamp();
    let mut past_events: Vec<&AgendaEvent> = Vec::new();
    let mut future_events: Vec<&AgendaEvent> = Vec::new();

    for event in state.events.values() {
        if event.end_time < now {
            past_events.push(event);
        } else {
            future_events.push(event);
        }
    }

    // Past events (if toggled on)
    if state.show_past && !past_events.is_empty() {
        for (idx, event) in past_events.iter().enumerate() {
            widgets.push(build_event_widget(event, idx, true));
        }
        widgets.push(NamedWidget {
            id: "eds-agenda:separator".into(),
            weight: 32,
            widget: SeparatorBuilder::new().build(),
        });
    }

    // Future events
    for (idx, event) in future_events.iter().enumerate() {
        widgets.push(build_event_widget(event, idx + 1000, false));
    }

    widgets
}

fn build_event_widget(event: &AgendaEvent, idx: usize, past: bool) -> NamedWidget {
    use chrono::TimeZone;

    let time_str = if event.all_day {
        "All day".to_string()
    } else {
        match chrono::Local.timestamp_opt(event.start_time, 0) {
            chrono::LocalResult::Single(start) => start.format("%H:%M").to_string(),
            _ => "??:??".to_string(),
        }
    };

    let mut css_classes = Vec::new();
    if past {
        css_classes.push("past-event".into());
    }

    // Summary row: time + title + meeting link buttons
    let mut summary = RowBuilder::new()
        .spacing(8)
        .child(
            LabelBuilder::new(&time_str)
                .css_class("dim-label")
                .css_class("caption")
                .build(),
        )
        .child(LabelBuilder::new(&event.summary).build());

    for link in extract_meeting_links(event) {
        let provider_label = match link.provider {
            MeetingProvider::GoogleMeet => "Meet",
            MeetingProvider::Zoom => "Zoom",
            MeetingProvider::Teams => "Teams",
        };
        summary = summary.child(
            ListButtonBuilder::new(provider_label)
                .on_click(format!("meeting_link:{}", link.url))
                .build(),
        );
    }

    // Content: details section (location, attendees, description)
    if !event.has_details() {
        // No details — render as plain row, no Details wrapper
        return NamedWidget {
            id: format!("eds-agenda:event:{idx}"),
            weight: (33 + idx) as u32,
            widget: summary.build(),
        };
    }

    let mut content_children = Vec::new();

    if let Some(ref location) = event.location {
        content_children.push(
            IconListBuilder::new("mark-location-symbolic")
                .child(LabelBuilder::new(location).build())
                .build(),
        );
    }

    if !event.attendees.is_empty() {
        let mut attendee_list = IconListBuilder::new("system-users-symbolic");
        for attendee in &event.attendees {
            let rsvp_icon = match attendee.status {
                waft_plugin_agenda::values::PartStat::Accepted => "emblem-ok-symbolic",
                waft_plugin_agenda::values::PartStat::Declined => "window-close-symbolic",
                waft_plugin_agenda::values::PartStat::Tentative => "dialog-question-symbolic",
                waft_plugin_agenda::values::PartStat::NeedsAction => "mail-unread-symbolic",
            };
            let name = attendee
                .name
                .as_deref()
                .unwrap_or(&attendee.email);
            attendee_list = attendee_list.child(
                IconListBuilder::new(rsvp_icon)
                    .icon_size(12)
                    .child(LabelBuilder::new(name).build())
                    .build(),
            );
        }
        content_children.push(attendee_list.build());
    }

    if let Some(ref desc) = event.description {
        let truncated = if desc.len() > 200 {
            format!("{}…", &desc[..200])
        } else {
            desc.clone()
        };
        content_children.push(
            IconListBuilder::new("text-x-generic-symbolic")
                .child(LabelBuilder::new(&truncated).build())
                .build(),
        );
    }

    let content = ColBuilder::new().spacing(4).children(content_children).build();

    NamedWidget {
        id: format!("eds-agenda:event:{idx}"),
        weight: (33 + idx) as u32,
        widget: DetailsBuilder::new()
            .summary(summary.build())
            .content(content)
            .css_classes(css_classes)
            .on_toggle(format!("toggle_detail:{}", event.uid))
            .build(),
    }
}

#[async_trait]
impl PluginDaemon for AgendaDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let state = lock(&self.state);
        self.build_widgets(&state)
    }

    async fn handle_action(
        &self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("[agenda] action: {}", action.id);

        match action.id.as_str() {
            "toggle_past" => {
                let mut state = lock(&self.state);
                state.show_past = !state.show_past;
            }
            id if id.starts_with("toggle_detail:") => {
                // Expand/collapse handled by overview MenuStore
            }
            id if id.starts_with("meeting_link:") => {
                let url = id.strip_prefix("meeting_link:").unwrap_or("");
                debug!("[agenda] opening meeting link: {url}");
                match std::process::Command::new("xdg-open").arg(url).spawn() {
                    Ok(child) => {
                        std::thread::spawn(move || {
                            let mut child = child;
                            let _ = child.wait();
                        });
                    }
                    Err(e) => error!("[agenda] failed to launch xdg-open: {e}"),
                }
            }
            _ => warn!("[agenda] unknown action: {}", action.id),
        }

        Ok(())
    }
}

/// Apply a ViewSignal to the state, updating events in place.
fn apply_signal(state: &StdMutex<AgendaState>, signal: ViewSignal) {
    let mut state = lock(state);
    match signal {
        ViewSignal::Added(events) => {
            for event in events {
                state.events.insert(event.occurrence_key(), event);
            }
        }
        ViewSignal::Modified(events) => {
            let uids: Vec<String> = events.iter().map(|e| e.uid.clone()).collect();
            remove_events_by_uids(&mut state.events, &uids);
            for event in events {
                state.events.insert(event.occurrence_key(), event);
            }
        }
        ViewSignal::Removed(uids) => {
            remove_events_by_uids(&mut state.events, &uids);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::init();

    let dbus = Arc::new(
        DbusHandle::connect()
            .await
            .context("failed to connect to session bus")?,
    );

    let state = Arc::new(StdMutex::new(AgendaState::default()));
    let view_paths = Arc::new(StdMutex::new(HashSet::new()));
    let active_views = Arc::new(StdMutex::new(Vec::new()));
    let config = AgendaConfig::default();
    let period = parse_period(&config.period)?;

    // Create signal channel
    let (signal_tx, signal_rx) = flume::unbounded::<ViewSignal>();

    // Start D-Bus signal listener BEFORE setting up views
    // so we don't miss the initial ObjectsAdded events.
    {
        let dbus = dbus.clone();
        let view_paths = view_paths.clone();
        tokio::spawn(async move {
            if let Err(e) = listen_view_signals(&dbus, signal_tx, view_paths).await {
                error!("[agenda] signal listener error: {e}");
            }
            warn!("[agenda] signal listener exited");
        });
    }

    // Small delay to let D-Bus match rule register
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let daemon = AgendaDaemon {
        dbus: dbus.clone(),
        state: state.clone(),
        period,
        lookahead: None,
        active_views: active_views.clone(),
        view_paths: view_paths.clone(),
    };

    // Phase 1: Discover sources and create views (no Start() yet —
    // that triggers the ObjectsAdded burst, so the consumer must be ready first).
    lock(&state).loading = true;
    daemon.setup_views().await?;

    // Create plugin server (moves daemon into server)
    let (server, server_notifier) = PluginServer::new("eds-agenda", daemon);

    // Spawn signal consumer — completes the pipeline:
    // D-Bus signal → broadcast → listener → flume → consumer → notifier → server → clients
    let signal_state = state.clone();
    tokio::spawn(async move {
        while let Ok(signal) = signal_rx.recv_async().await {
            apply_signal(&signal_state, signal);
            server_notifier.notify();
        }
        warn!("[agenda] signal consumer exited — daemon is now unresponsive");
    });

    // Phase 2: Start views now that the full pipeline is ready.
    // This triggers ObjectsAdded with all matching events.
    {
        let views: Vec<(String, String)> = lock(&active_views)
            .iter()
            .map(|v| (v.bus_name.clone(), v.view_path.clone()))
            .collect();

        for (bus_name, view_path) in &views {
            if let Err(e) = start_view(&dbus, bus_name, view_path).await {
                warn!("[agenda] failed to start view {view_path}: {e:?}");
            } else {
                debug!("[agenda] view started: {view_path}");
            }
        }

        lock(&state).loading = false;
    }

    server.run().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(uid: &str, start: i64, end: i64) -> AgendaEvent {
        AgendaEvent {
            uid: uid.to_string(),
            summary: format!("Event {uid}"),
            start_time: start,
            end_time: end,
            all_day: false,
            description: None,
            alt_description: None,
            location: None,
            attendees: Vec::new(),
        }
    }

    fn make_event_with_details(
        uid: &str,
        start: i64,
        end: i64,
        desc: Option<&str>,
        loc: Option<&str>,
    ) -> AgendaEvent {
        AgendaEvent {
            uid: uid.to_string(),
            summary: format!("Event {uid}"),
            start_time: start,
            end_time: end,
            all_day: false,
            description: desc.map(|s| s.to_string()),
            alt_description: None,
            location: loc.map(|s| s.to_string()),
            attendees: Vec::new(),
        }
    }

    fn make_all_day_event(uid: &str, start: i64, end: i64) -> AgendaEvent {
        AgendaEvent {
            uid: uid.to_string(),
            summary: format!("Event {uid}"),
            start_time: start,
            end_time: end,
            all_day: true,
            description: None,
            alt_description: None,
            location: None,
            attendees: Vec::new(),
        }
    }

    /// Find a widget by ID prefix in a list of NamedWidgets.
    fn find_widget<'a>(widgets: &'a [NamedWidget], id: &str) -> Option<&'a NamedWidget> {
        widgets.iter().find(|w| w.id == id)
    }

    /// Check if any widget has the given ID prefix.
    fn has_widget(widgets: &[NamedWidget], id: &str) -> bool {
        widgets.iter().any(|w| w.id == id)
    }

    // ── apply_signal ────────────────────────────────────────────

    #[test]
    fn apply_signal_added_inserts_events() {
        let state = StdMutex::new(AgendaState::default());
        apply_signal(
            &state,
            ViewSignal::Added(vec![
                make_event("a", 1000, 2000),
                make_event("b", 3000, 4000),
            ]),
        );
        let state = lock(&state);
        assert_eq!(state.events.len(), 2);
        assert!(state.events.contains_key("a@1000"));
        assert!(state.events.contains_key("b@3000"));
    }

    #[test]
    fn apply_signal_modified_replaces_events() {
        let state = StdMutex::new(AgendaState::default());
        // Initial event
        apply_signal(
            &state,
            ViewSignal::Added(vec![make_event("a", 1000, 2000)]),
        );
        // Modified: time changed from 1000 to 5000
        apply_signal(
            &state,
            ViewSignal::Modified(vec![make_event("a", 5000, 6000)]),
        );
        let state = lock(&state);
        // Old occurrence removed, new inserted
        assert_eq!(state.events.len(), 1);
        assert!(!state.events.contains_key("a@1000"));
        assert!(state.events.contains_key("a@5000"));
    }

    #[test]
    fn apply_signal_modified_removes_old_recurring_occurrences() {
        let state = StdMutex::new(AgendaState::default());
        // Two occurrences of same event
        apply_signal(
            &state,
            ViewSignal::Added(vec![
                make_event("a", 1000, 2000),
                make_event("a", 5000, 6000),
            ]),
        );
        assert_eq!(lock(&state).events.len(), 2);
        // Modified signal for UID "a" — should remove both old occurrences
        apply_signal(
            &state,
            ViewSignal::Modified(vec![make_event("a", 9000, 10000)]),
        );
        let state = lock(&state);
        assert_eq!(state.events.len(), 1);
        assert!(state.events.contains_key("a@9000"));
    }

    #[test]
    fn apply_signal_removed_deletes_events() {
        let state = StdMutex::new(AgendaState::default());
        apply_signal(
            &state,
            ViewSignal::Added(vec![
                make_event("a", 1000, 2000),
                make_event("b", 3000, 4000),
            ]),
        );
        apply_signal(
            &state,
            ViewSignal::Removed(vec!["a".to_string()]),
        );
        let state = lock(&state);
        assert_eq!(state.events.len(), 1);
        assert!(state.events.contains_key("b@3000"));
    }

    #[test]
    fn apply_signal_removed_nonexistent_noop() {
        let state = StdMutex::new(AgendaState::default());
        apply_signal(
            &state,
            ViewSignal::Added(vec![make_event("a", 1000, 2000)]),
        );
        apply_signal(
            &state,
            ViewSignal::Removed(vec!["nonexistent".to_string()]),
        );
        assert_eq!(lock(&state).events.len(), 1);
    }

    // ── build_widgets: state variations ─────────────────────────

    #[test]
    fn build_widgets_always_has_header() {
        let state = AgendaState::default();
        let widgets = build_widgets(&state);
        assert!(has_widget(&widgets, "eds-agenda:header"));
    }

    #[test]
    fn build_widgets_loading_shows_spinner() {
        let state = AgendaState {
            loading: true,
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        assert!(has_widget(&widgets, "eds-agenda:loading"));
        // Should not show empty or error
        assert!(!has_widget(&widgets, "eds-agenda:empty"));
        assert!(!has_widget(&widgets, "eds-agenda:error"));
    }

    #[test]
    fn build_widgets_loading_spinner_is_spinning() {
        let state = AgendaState {
            loading: true,
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        let loading = find_widget(&widgets, "eds-agenda:loading").unwrap();
        assert!(matches!(loading.widget, Widget::Spinner { spinning: true }));
    }

    #[test]
    fn build_widgets_error_shows_error_label() {
        let state = AgendaState {
            error: Some("Calendar not available".to_string()),
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        assert!(has_widget(&widgets, "eds-agenda:error"));
        let error = find_widget(&widgets, "eds-agenda:error").unwrap();
        if let Widget::Label { ref text, ref css_classes } = error.widget {
            assert_eq!(text, "Calendar not available");
            assert!(css_classes.contains(&"error".to_string()));
        } else {
            panic!("Expected Label widget for error, got {:?}", error.widget);
        }
    }

    #[test]
    fn build_widgets_empty_events_shows_message() {
        let state = AgendaState {
            available: true,
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        assert!(has_widget(&widgets, "eds-agenda:empty"));
        let empty = find_widget(&widgets, "eds-agenda:empty").unwrap();
        if let Widget::Label { ref text, ref css_classes } = empty.widget {
            assert_eq!(text, "No upcoming events");
            assert!(css_classes.contains(&"dim-label".to_string()));
        } else {
            panic!("Expected Label widget for empty, got {:?}", empty.widget);
        }
    }

    #[test]
    fn build_widgets_loading_takes_priority_over_error() {
        let state = AgendaState {
            loading: true,
            error: Some("Error".to_string()),
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        assert!(has_widget(&widgets, "eds-agenda:loading"));
        assert!(!has_widget(&widgets, "eds-agenda:error"));
    }

    #[test]
    fn build_widgets_error_takes_priority_over_empty() {
        let state = AgendaState {
            error: Some("Error".to_string()),
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        assert!(has_widget(&widgets, "eds-agenda:error"));
        assert!(!has_widget(&widgets, "eds-agenda:empty"));
    }

    #[test]
    fn build_widgets_future_events_shown() {
        let far_future = chrono::Local::now().timestamp() + 86400;
        let mut events = BTreeMap::new();
        let evt = make_event("evt-1", far_future, far_future + 3600);
        events.insert(evt.occurrence_key(), evt);

        let state = AgendaState {
            events,
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        assert!(has_widget(&widgets, "eds-agenda:event:1000"));
        assert!(!has_widget(&widgets, "eds-agenda:empty"));
    }

    #[test]
    fn build_widgets_past_events_shown_when_show_past_true() {
        let past = 1000; // way in the past
        let mut events = BTreeMap::new();
        let evt = make_event("past-evt", past, past + 3600);
        events.insert(evt.occurrence_key(), evt);

        let state = AgendaState {
            events,
            show_past: true,
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        // Past event at index 0
        assert!(has_widget(&widgets, "eds-agenda:event:0"));
        // Separator between past and future
        assert!(has_widget(&widgets, "eds-agenda:separator"));
    }

    #[test]
    fn build_widgets_past_events_hidden_when_show_past_false() {
        let past = 1000;
        let mut events = BTreeMap::new();
        let evt = make_event("past-evt", past, past + 3600);
        events.insert(evt.occurrence_key(), evt);

        let state = AgendaState {
            events,
            show_past: false,
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        // Should not show any event widgets (only past events, hidden)
        assert!(!has_widget(&widgets, "eds-agenda:event:0"));
        assert!(!has_widget(&widgets, "eds-agenda:separator"));
    }

    #[test]
    fn build_widgets_mixed_past_and_future() {
        let now = chrono::Local::now().timestamp();
        let past = now - 7200;
        let future = now + 7200;

        let mut events = BTreeMap::new();
        let past_evt = make_event("past", past, past + 3600);
        let future_evt = make_event("future", future, future + 3600);
        events.insert(past_evt.occurrence_key(), past_evt);
        events.insert(future_evt.occurrence_key(), future_evt);

        let state = AgendaState {
            events,
            show_past: true,
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        // Past event at index 0, separator, future event at index 1000
        assert!(has_widget(&widgets, "eds-agenda:event:0"));
        assert!(has_widget(&widgets, "eds-agenda:separator"));
        assert!(has_widget(&widgets, "eds-agenda:event:1000"));
    }

    #[test]
    fn build_widgets_widget_ids_are_prefixed() {
        let far_future = chrono::Local::now().timestamp() + 86400;
        let mut events = BTreeMap::new();
        let evt = make_event("a", far_future, far_future + 3600);
        events.insert(evt.occurrence_key(), evt);

        let state = AgendaState {
            events,
            ..Default::default()
        };
        let widgets = build_widgets(&state);
        for widget in &widgets {
            assert!(
                widget.id.starts_with("eds-agenda:"),
                "Widget ID '{}' must start with 'eds-agenda:'",
                widget.id
            );
        }
    }

    // ── build_event_widget ──────────────────────────────────────

    #[test]
    fn build_event_widget_simple_no_details() {
        let event = make_event("evt-1", 1737885600, 1737889200);
        let widget = build_event_widget(&event, 5, false);
        assert_eq!(widget.id, "eds-agenda:event:5");
        assert_eq!(widget.weight, 38); // 33 + 5
        // No details → should be a Row, not Details
        assert!(matches!(widget.widget, Widget::Row { .. }));
    }

    #[test]
    fn build_event_widget_with_details_uses_details_wrapper() {
        let event = make_event_with_details("evt-2", 1737885600, 1737889200, Some("Notes"), Some("Room 1"));
        let widget = build_event_widget(&event, 0, false);
        assert!(
            matches!(widget.widget, Widget::Details { .. }),
            "Event with details should use Details widget"
        );
    }

    #[test]
    fn build_event_widget_past_has_css_class() {
        let event = make_event_with_details("evt-3", 1737885600, 1737889200, Some("Notes"), None);
        let widget = build_event_widget(&event, 0, true);
        if let Widget::Details { ref css_classes, .. } = widget.widget {
            assert!(
                css_classes.contains(&"past-event".to_string()),
                "Past event should have 'past-event' CSS class"
            );
        } else {
            panic!("Expected Details widget for past event with details");
        }
    }

    #[test]
    fn build_event_widget_not_past_no_css_class() {
        let event = make_event_with_details("evt-4", 1737885600, 1737889200, Some("Notes"), None);
        let widget = build_event_widget(&event, 0, false);
        if let Widget::Details { ref css_classes, .. } = widget.widget {
            assert!(
                !css_classes.contains(&"past-event".to_string()),
                "Non-past event should not have 'past-event' CSS class"
            );
        }
    }

    #[test]
    fn build_event_widget_all_day_shows_all_day_label() {
        let event = make_all_day_event("evt-5", 1737849600, 1737936000);
        let widget = build_event_widget(&event, 0, false);
        // The widget should be a Row (no details) containing "All day" label
        if let Widget::Row { ref children, .. } = widget.widget {
            // First child is the time label
            let time_label = &children[0].widget;
            if let Widget::Label { text, .. } = time_label {
                assert_eq!(text, "All day");
            } else {
                panic!("Expected Label as first child of Row, got {:?}", time_label);
            }
        } else {
            panic!("Expected Row widget, got {:?}", widget.widget);
        }
    }

    #[test]
    fn build_event_widget_weight_includes_index() {
        let event = make_event("evt-w", 1737885600, 1737889200);
        assert_eq!(build_event_widget(&event, 0, false).weight, 33);
        assert_eq!(build_event_widget(&event, 10, false).weight, 43);
        assert_eq!(build_event_widget(&event, 1000, false).weight, 1033);
    }

    #[test]
    fn build_event_widget_with_meeting_link_adds_button() {
        let event = AgendaEvent {
            uid: "evt-meet".to_string(),
            summary: "Video Call".to_string(),
            start_time: 1737885600,
            end_time: 1737889200,
            all_day: false,
            description: Some("Join https://meet.google.com/abc-def-ghi".to_string()),
            alt_description: None,
            location: None,
            attendees: Vec::new(),
        };
        let widget = build_event_widget(&event, 0, false);
        // Has description → Details widget
        if let Widget::Details { ref summary, .. } = widget.widget {
            // Summary is a Row with children
            if let Widget::Row { ref children, .. } = **summary {
                // Should have time label + summary label + Meet button
                assert!(
                    children.len() >= 3,
                    "Expected at least 3 children (time, title, meet button), got {}",
                    children.len()
                );
                // Last child should be a ListButton with "Meet" label
                let last = &children[children.len() - 1].widget;
                if let Widget::ListButton { label, .. } = last {
                    assert_eq!(label, "Meet");
                } else {
                    panic!("Expected ListButton for meeting link, got {:?}", last);
                }
            } else {
                panic!("Expected Row as summary");
            }
        } else {
            panic!("Expected Details widget");
        }
    }

    #[test]
    fn build_event_widget_with_attendees_shows_icon_list() {
        use waft_plugin_agenda::values::{Attendee, PartStat};
        let event = AgendaEvent {
            uid: "evt-att".to_string(),
            summary: "Team Meeting".to_string(),
            start_time: 1737885600,
            end_time: 1737889200,
            all_day: false,
            description: None,
            alt_description: None,
            location: None,
            attendees: vec![
                Attendee {
                    name: Some("Alice".to_string()),
                    email: "alice@example.com".to_string(),
                    status: PartStat::Accepted,
                },
                Attendee {
                    name: Some("Bob".to_string()),
                    email: "bob@example.com".to_string(),
                    status: PartStat::Declined,
                },
            ],
        };
        let widget = build_event_widget(&event, 0, false);
        // Has attendees → Details widget
        assert!(
            matches!(widget.widget, Widget::Details { .. }),
            "Event with attendees should use Details widget"
        );
    }

    #[test]
    fn build_event_widget_details_on_toggle_contains_uid() {
        let event = make_event_with_details("my-uid-123", 1737885600, 1737889200, Some("Notes"), None);
        let widget = build_event_widget(&event, 0, false);
        if let Widget::Details { ref on_toggle, .. } = widget.widget {
            assert_eq!(on_toggle.id, "toggle_detail:my-uid-123");
        } else {
            panic!("Expected Details widget");
        }
    }

    #[test]
    fn build_event_widget_long_description_truncated() {
        let long_desc = "A".repeat(300);
        let event = make_event_with_details("evt-long", 1737885600, 1737889200, Some(&long_desc), None);
        let widget = build_event_widget(&event, 0, false);
        if let Widget::Details { ref content, .. } = widget.widget {
            // Content is a Col with children
            if let Widget::Col { ref children, .. } = **content {
                // Should have one child (description IconList)
                assert!(!children.is_empty());
                // The description text should be truncated
                let desc_icon_list = &children[0].widget;
                if let Widget::IconList { children, .. } = desc_icon_list {
                    let label = &children[0].widget;
                    if let Widget::Label { text, .. } = label {
                        assert!(text.len() <= 204); // 200 chars + "…" (multi-byte)
                        assert!(text.ends_with('…'));
                    }
                }
            }
        }
    }

    // ── AgendaState default ─────────────────────────────────────

    #[test]
    fn default_state_is_correct() {
        let state = AgendaState::default();
        assert!(state.events.is_empty());
        assert!(state.sources.is_empty());
        assert!(!state.available);
        assert!(!state.loading);
        assert!(state.error.is_none());
        assert!(state.show_past);
        assert!(state.query_since.is_none());
    }
}
