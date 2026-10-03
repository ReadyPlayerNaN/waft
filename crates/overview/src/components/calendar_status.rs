//! Cached view delivery and Refresh replies do not establish remote freshness.
use gtk::prelude::*;
use std::{cell::Cell, rc::Rc};
use waft_protocol::entity::{
    accounts::Availability,
    calendar::{CalendarSourceState, CalendarSourceStatus, CalendarSync, RefreshOutcome},
};
pub fn health_key(
    sync: Option<&CalendarSync>,
    sources: &[CalendarSourceStatus],
) -> Option<&'static str> {
    let Some(sync) = sync else {
        return Some("calendar-status-unknown");
    };
    match sync.availability {
        Availability::Starting => return Some("calendar-status-loading"),
        Availability::Recovering | Availability::Unavailable => {
            return Some("calendar-status-recovering");
        }
        Availability::Unsupported => return Some("calendar-status-unsupported"),
        Availability::Unknown => return Some("calendar-status-unknown"),
        Availability::Ready => {}
    }
    if sources.iter().any(|s| {
        s.error.as_ref().is_some_and(|e| {
            matches!(
                e.code.as_str(),
                "eds.credentials-required" | "eds.certificate-required"
            )
        })
    }) {
        return Some("calendar-status-attention");
    }
    if sources.iter().any(|s| {
        s.online == Some(false) || s.error.as_ref().is_some_and(|e| e.code == "eds.offline")
    }) {
        return Some("calendar-status-offline");
    }
    if sources
        .iter()
        .any(|s| s.state == CalendarSourceState::Unsupported)
        || sync.refresh_outcome == RefreshOutcome::Unsupported
    {
        return Some("calendar-status-unsupported");
    }
    if sources.iter().any(|s| {
        s.error.is_some()
            || matches!(
                s.state,
                CalendarSourceState::RetryWait | CalendarSourceState::Unavailable
            )
    }) {
        return Some("calendar-status-recovering");
    }
    if sources.iter().any(|s| {
        matches!(
            s.state,
            CalendarSourceState::Pending
                | CalendarSourceState::Opening
                | CalendarSourceState::Loading
        )
    }) {
        return Some("calendar-status-loading");
    }
    if matches!(
        sync.refresh_outcome,
        RefreshOutcome::Failed | RefreshOutcome::PartialFailure
    ) {
        return Some("calendar-refresh-failed");
    }
    if sources
        .iter()
        .any(|s| s.state == CalendarSourceState::Disabled)
    {
        return Some("calendar-status-disabled");
    }
    if sources
        .iter()
        .any(|s| s.state == CalendarSourceState::Unknown)
    {
        return Some("calendar-status-unknown");
    }
    None
}
pub fn source_key(source: &CalendarSourceStatus) -> &'static str {
    health_key(
        Some(&CalendarSync {
            availability: Availability::Ready,
            ..Default::default()
        }),
        std::slice::from_ref(source),
    )
    .unwrap_or("calendar-status-view-delivered")
}
/// Lives above the calendar revealer: agenda-only mode must expose the same health.
pub(crate) fn widget(store: &Rc<waft_client::EntityStore>, parent: &gtk::Box) -> gtk::Label {
    let label = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label", "caption"])
        .visible(false)
        .build();
    let fresh = Rc::new(Cell::new(true));
    let update: Rc<dyn Fn()> = {
        let store = store.clone();
        let label = label.clone();
        let fresh = fresh.clone();
        Rc::new(move || {
            let sync = store
                .get_entities_typed::<CalendarSync>(
                    waft_protocol::entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
                )
                .into_iter()
                .next()
                .map(|(_, s)| s);
            let mut sources: Vec<_> = store
                .get_entities_typed::<CalendarSourceStatus>(
                    waft_protocol::entity::calendar::CALENDAR_SOURCE_STATUS_ENTITY_TYPE,
                )
                .into_iter()
                .map(|(_, s)| s)
                .collect();
            sources.sort_by(|a, b| a.source_uid.cmp(&b.source_uid));
            let key = if fresh.get() {
                health_key(sync.as_ref(), &sources)
            } else {
                Some("calendar-status-recovering")
            };
            let request = sync
                .and_then(|s| s.last_refresh)
                .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                .map(|t| {
                    format!(
                        "{}: {}",
                        crate::i18n::t("calendar-last-refresh-request"),
                        t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")
                    )
                });
            label.set_visible(key.is_some() || request.is_some());
            label.set_label(
                &key.map(crate::i18n::t)
                    .or_else(|| request.clone())
                    .unwrap_or_default(),
            );
            let mut lines: Vec<_> = sources
                .iter()
                .map(|s| format!("{}: {}", s.display_name, crate::i18n::t(source_key(s))))
                .collect();
            if let Some(request) = request {
                lines.push(request);
            }
            label.set_tooltip_text((!lines.is_empty()).then(|| lines.join("\n")).as_deref());
            crate::ui::main_window::trigger_window_resize();
        })
    };
    // Coalesce signal bursts into one idle update; hidden overlays reconcile on map.
    let schedule: Rc<dyn Fn()> = {
        let parent = parent.downgrade();
        let queued = Rc::new(Cell::new(false));
        let update = update.clone();
        Rc::new(move || {
            if !parent.upgrade().is_some_and(|p| p.is_mapped()) || queued.replace(true) {
                return;
            }
            let queued = queued.clone();
            let update = update.clone();
            let parent = parent.clone();
            gtk::glib::idle_add_local_once(move || {
                queued.set(false);
                if parent.upgrade().is_some_and(|p| p.is_mapped()) {
                    update();
                }
            });
        })
    };
    let s = schedule.clone();
    let f = fresh.clone();
    let cache = store.clone();
    store.subscribe_type(
        waft_protocol::entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
        move || {
            f.set(
                !cache
                    .get_entities_raw(waft_protocol::entity::calendar::CALENDAR_SYNC_ENTITY_TYPE)
                    .is_empty(),
            );
            s();
        },
    );
    let s = schedule.clone();
    store.subscribe_type(
        waft_protocol::entity::calendar::CALENDAR_SOURCE_STATUS_ENTITY_TYPE,
        move || s(),
    );
    let s = schedule.clone();
    store.on_disconnect(move || {
        fresh.set(false);
        s();
    });
    let u = update.clone();
    parent.connect_map(move |_| u());
    gtk::glib::idle_add_local_once(move || update());
    label
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires an isolated GTK display; run explicitly under Broadway or a disposable Wayland session"]
    fn gtk_calendar_health_remains_observable_and_recovers_after_equal_status() {
        gtk::init().expect("GTK display");
        let store = Rc::new(waft_client::EntityStore::new());
        let parent = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let label = widget(&store, &parent);
        parent.append(&label);
        parent.append(&gtk::Label::new(Some("Fixture agenda")));
        let window = gtk::Window::new();
        window.set_child(Some(&parent));
        window.present();
        let drain = || {
            let context = gtk::glib::MainContext::default();
            for _ in 0..100 {
                if !context.pending() {
                    break;
                }
                context.iteration(false);
            }
        };
        drain();
        let sync = CalendarSync {
            availability: Availability::Ready,
            last_refresh: Some(1700000000),
            refresh_outcome: RefreshOutcome::Accepted,
            ..Default::default()
        };
        let notification = waft_protocol::AppNotification::EntityUpdated {
            urn: waft_protocol::Urn::new("eds", "calendar-sync", "singleton"),
            entity_type: None,
            data: serde_json::to_value(&sync).expect("sync"),
        };
        store.handle_notification(notification.clone());
        drain();
        assert!(label.is_visible());
        assert!(
            label
                .label()
                .starts_with(&crate::i18n::t("calendar-last-refresh-request"))
        );
        store.handle_disconnect();
        drain();
        assert_eq!(label.label(), crate::i18n::t("calendar-status-recovering"));
        store.handle_notification(notification);
        drain();
        assert!(
            label
                .label()
                .starts_with(&crate::i18n::t("calendar-last-refresh-request"))
        );
        let source = CalendarSourceStatus {
            source_uid: "fixture".into(),
            display_name: "Fixture".into(),
            state: CalendarSourceState::Watching,
            online: Some(false),
            ..Default::default()
        };
        store.handle_notification(waft_protocol::AppNotification::EntityUpdated {
            urn: waft_protocol::Urn::new("eds", "calendar-source-status", "fixture"),
            entity_type: None,
            data: serde_json::to_value(source).expect("source"),
        });
        drain();
        assert_eq!(label.label(), crate::i18n::t("calendar-status-offline"));
        window.close();
    }
    #[test]
    fn accepted_request_does_not_hide_offline_or_incomplete_source() {
        let sync = CalendarSync {
            availability: Availability::Ready,
            refresh_outcome: RefreshOutcome::Accepted,
            ..Default::default()
        };
        let mut source = CalendarSourceStatus {
            state: CalendarSourceState::Watching,
            online: Some(false),
            ..Default::default()
        };
        assert_eq!(
            health_key(Some(&sync), &[source.clone()]),
            Some("calendar-status-offline")
        );
        source.online = Some(true);
        source.state = CalendarSourceState::Loading;
        assert_eq!(
            health_key(Some(&sync), &[source.clone()]),
            Some("calendar-status-loading")
        );
        source.state = CalendarSourceState::Watching;
        assert_eq!(health_key(Some(&sync), &[source]), None);
        assert_eq!(
            health_key(Some(&CalendarSync::default()), &[]),
            Some("calendar-status-unknown")
        );
        assert_eq!(health_key(None, &[]), Some("calendar-status-unknown"));
    }
}
