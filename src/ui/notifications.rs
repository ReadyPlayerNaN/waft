/*!
Notifications UI (model + view)

This module provides:
- A testable `NotificationsModel` that stores notifications, grouped by app name.
- A GTK `NotificationsView` that renders the model as grouped, collapsible notification cards.
- Icon support: notification icons can be themed icon names or file paths (scaled to 32px).
- Actions and default action callbacks.
- Grouping rules:
  - Group strictly by a normalized app name key (normalization also used for app icon lookup).
  - Groups are ordered by most recent notification timestamp (descending).
  - Within a group, notifications are ordered by most recent timestamp (descending).
  - If a group has more than one notification, it is collapsible; collapsed shows only latest plus an expand button.
  - Only one group can be expanded at a time.
  - Closing latest reveals the next-latest; empty group disappears.
  - Clear clears all notifications (even if expanded).

Note: This module currently keeps rendering simple (rebuild on each change).
It is intentionally structured so re-render can be debounced later.
*/

use adw::prelude::*;
use gtk::gdk;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

/// Notification icon representation.
///
/// The builder is responsible for choosing the final icon (explicit/app/default),
/// so `Notification.icon` is mandatory and always set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationIcon {
    /// A themed icon name, e.g. "dialog-information-symbolic".
    Themed(String),
    /// A file path to an image (png/svg/etc). Will be loaded and scaled to fit.
    FilePath(PathBuf),
}

/// A notification action (button).
#[derive(Clone)]
pub struct NotificationAction {
    pub label: String,
    pub on_invoke: Rc<dyn Fn() + 'static>,
}

impl std::fmt::Debug for NotificationAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Avoid printing closure details.
        f.debug_struct("NotificationAction")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

/// Represents a single notification with its data.
///
/// Notes:
/// - `created_at` is required to support correct "latest" grouping and ordering.
/// - `icon` is required and assumed to be already resolved by the builder.
#[derive(Clone)]
pub struct Notification {
    pub id: u64,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub created_at: SystemTime,
    pub icon: NotificationIcon,
    pub actions: Vec<NotificationAction>,
    pub on_default_action: Option<Rc<dyn Fn() + 'static>>,
}

impl std::fmt::Debug for Notification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Avoid printing closure details.
        f.debug_struct("Notification")
            .field("id", &self.id)
            .field("app_name", &self.app_name)
            .field("summary", &self.summary)
            .field("body", &self.body)
            .field("created_at", &self.created_at)
            .field("icon", &self.icon)
            .field("actions", &self.actions)
            .finish_non_exhaustive()
    }
}

impl Notification {
    pub fn new(
        id: u64,
        app_name: String,
        summary: String,
        body: String,
        created_at: SystemTime,
        icon: NotificationIcon,
    ) -> Self {
        Self {
            id,
            app_name,
            summary,
            body,
            created_at,
            icon,
            actions: vec![],
            on_default_action: None,
        }
    }

    pub fn with_default_action<F: Fn() + 'static>(mut self, action: F) -> Self {
        self.on_default_action = Some(Rc::new(action));
        self
    }

    pub fn with_action<F: Fn() + 'static>(
        mut self,
        label: impl Into<String>,
        on_invoke: F,
    ) -> Self {
        self.actions.push(NotificationAction {
            label: label.into(),
            on_invoke: Rc::new(on_invoke),
        });
        self
    }
}

/// Normalized key used for grouping and for app-name icon lookup.
fn normalize_app_key(app_name: &str) -> String {
    let mut out = String::with_capacity(app_name.len());
    let mut prev_dash = false;

    for ch in app_name.chars() {
        let c = ch.to_ascii_lowercase();

        // Keep common desktop/app-id characters.
        let is_ok = c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';
        let mapped = if is_ok {
            Some(c)
        } else if c.is_whitespace() || c == '/' || c == ':' {
            Some('-')
        } else {
            // Drop other punctuation/symbols.
            None
        };

        if let Some(mc) = mapped {
            if mc == '-' {
                if prev_dash {
                    continue;
                }
                prev_dash = true;
                out.push('-');
            } else {
                prev_dash = false;
                out.push(mc);
            }
        }
    }

    // Trim leading/trailing dashes
    while out.starts_with('-') {
        out.remove(0);
    }
    while out.ends_with('-') {
        out.pop();
    }

    out
}

fn systemtime_cmp_desc(a: &SystemTime, b: &SystemTime) -> std::cmp::Ordering {
    // SystemTime doesn't implement Ord; use duration since UNIX_EPOCH if possible.
    // If times are before UNIX_EPOCH or errors occur, fall back to equality-ish ordering.
    use std::time::UNIX_EPOCH;

    match (a.duration_since(UNIX_EPOCH), b.duration_since(UNIX_EPOCH)) {
        (Ok(da), Ok(db)) => db.cmp(&da),
        _ => std::cmp::Ordering::Equal,
    }
}

/// A group of notifications (by normalized app key).
#[derive(Clone, Debug)]
pub struct NotificationGroup {
    pub app_key: String,
    pub display_app_name: String,
    pub notifications: Vec<Notification>, // sorted newest-first
}

impl NotificationGroup {
    pub fn latest(&self) -> Option<&Notification> {
        self.notifications.first()
    }

    pub fn latest_ts(&self) -> Option<SystemTime> {
        self.latest().map(|n| n.created_at)
    }

    pub fn len(&self) -> usize {
        self.notifications.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notifications.is_empty()
    }
}

/// A snapshot suitable for rendering and testing.
#[derive(Clone, Debug)]
pub struct NotificationsSnapshot {
    pub groups: Vec<NotificationGroup>, // sorted newest-first by group latest_ts
    pub open_group: Option<String>,     // app_key
    pub total_count: usize,
}

/// Testable model. UI should observe it by calling `snapshot()` and re-rendering.
///
/// This is structured so it can be debounced later: the view can schedule
/// `render_from_snapshot(model.snapshot())` on idle/timer instead of doing it immediately.
#[derive(Debug, Default)]
pub struct NotificationsModel {
    // Store notifications grouped by app_key.
    groups: HashMap<String, NotificationGroup>,
    open_group: Option<String>,
}

impl NotificationsModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a notification (inserts into its group) and ensures ordering.
    pub fn add(&mut self, n: Notification) {
        let key = normalize_app_key(&n.app_name);
        let group = self
            .groups
            .entry(key.clone())
            .or_insert_with(|| NotificationGroup {
                app_key: key.clone(),
                display_app_name: n.app_name.clone(),
                notifications: vec![],
            });

        // If we already have a display name, keep the original; otherwise set.
        // (This keeps first-seen app name as label.)
        if group.display_app_name.is_empty() {
            group.display_app_name = n.app_name.clone();
        }

        group.notifications.push(n);

        // Sort newest-first, stable-ish by created_at then id.
        group.notifications.sort_by(|a, b| {
            let c = systemtime_cmp_desc(&a.created_at, &b.created_at);
            if c == std::cmp::Ordering::Equal {
                // Desc by id as tie-breaker (higher id considered newer).
                b.id.cmp(&a.id)
            } else {
                c
            }
        });
    }

    /// Remove a notification by id. Returns true if removed.
    pub fn remove(&mut self, id: u64) -> bool {
        let mut empty_keys: Vec<String> = vec![];
        let mut removed = false;

        for (k, g) in self.groups.iter_mut() {
            let before = g.notifications.len();
            g.notifications.retain(|n| n.id != id);
            if g.notifications.len() != before {
                removed = true;
            }
            if g.notifications.is_empty() {
                empty_keys.push(k.clone());
            }
        }

        for k in empty_keys {
            self.groups.remove(&k);
            if self.open_group.as_deref() == Some(k.as_str()) {
                self.open_group = None;
            }
        }

        removed
    }

    /// Clear all notifications and close any open group.
    pub fn clear(&mut self) {
        self.groups.clear();
        self.open_group = None;
    }

    /// Set which group is open. Only one may be open at a time.
    ///
    /// If `app_key` is `None`, closes all.
    /// If a key is provided but no such group exists (any more), open is cleared.
    pub fn set_open_group(&mut self, app_key: Option<String>) {
        if let Some(k) = app_key {
            if self.groups.contains_key(&k) {
                self.open_group = Some(k);
            } else {
                self.open_group = None;
            }
        } else {
            self.open_group = None;
        }
    }

    pub fn toggle_open_group(&mut self, app_key: &str) {
        let k = app_key.to_string();
        if self.open_group.as_deref() == Some(app_key) {
            self.open_group = None;
        } else if self.groups.contains_key(app_key) {
            self.open_group = Some(k);
        } else {
            self.open_group = None;
        }
    }

    pub fn open_group(&self) -> Option<&str> {
        self.open_group.as_deref()
    }

    /// Returns a sorted snapshot for rendering.
    pub fn snapshot(&self) -> NotificationsSnapshot {
        let mut groups: Vec<NotificationGroup> = self.groups.values().cloned().collect();

        groups.sort_by(|a, b| match (a.latest_ts(), b.latest_ts()) {
            (Some(ta), Some(tb)) => systemtime_cmp_desc(&ta, &tb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        let total_count = groups.iter().map(|g| g.notifications.len()).sum();

        NotificationsSnapshot {
            groups,
            open_group: self.open_group.clone(),
            total_count,
        }
    }
}

/// GTK view for notifications. Renders a `NotificationsModel` snapshot.
pub struct NotificationsView {
    root: gtk::Widget,

    title_label: gtk::Label,
    clear_btn: gtk::Button,
    scrolled: gtk::ScrolledWindow,
    groups_list: gtk::Box,

    // Rendering settings
    icon_size: i32,
    // Default icon name for themed icon fallback in the view (if a themed name is missing).
    default_themed_icon: String,
}

impl NotificationsView {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .vexpand(true)
            .build();

        let header_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_bottom(8)
            .build();

        let title_label = gtk::Label::builder()
            .css_classes(["heading"])
            .xalign(0.0)
            .hexpand(true)
            .label("Notifications")
            .build();

        let clear_btn = gtk::Button::builder()
            .label("Clear")
            .css_classes(["destructive-action"])
            .build();

        header_box.append(&title_label);
        header_box.append(&clear_btn);

        let scrolled = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .css_classes(["notification-scrollable"])
            .build();
        scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

        let groups_list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(8)
            .margin_end(16)
            .build();

        scrolled.set_child(Some(&groups_list));

        root.append(&header_box);
        root.append(&scrolled);

        Self {
            root: root.upcast::<gtk::Widget>(),
            title_label,
            clear_btn,
            scrolled,
            groups_list,
            icon_size: 32,
            // No preference was specified; pick a reasonable symbolic default.
            default_themed_icon: "dialog-information-symbolic".to_string(),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }

    /// Connect Clear button to the provided handler (typically model.clear() + rerender).
    pub fn connect_clear<F: Fn() + 'static>(&self, f: F) {
        self.clear_btn.connect_clicked(move |_| f());
    }

    /// Render from a model snapshot.
    ///
    /// This rebuilds the list. It is structured so it can be debounced later by
    /// calling it less frequently from the controller.
    pub fn render_from_snapshot<F1, F2, F3>(
        &self,
        snapshot: NotificationsSnapshot,
        on_close_notification: F1,
        on_toggle_group: F2,
        on_close_all_groups: F3,
    ) where
        F1: Fn(u64) + Clone + 'static,
        F2: Fn(String) + Clone + 'static,
        F3: Fn() + Clone + 'static,
    {
        // Title
        if snapshot.total_count == 0 {
            self.title_label.set_label("Notifications");
        } else {
            self.title_label
                .set_label(&format!("Notifications ({})", snapshot.total_count));
        }

        // Clear existing children
        while let Some(child) = self.groups_list.first_child() {
            self.groups_list.remove(&child);
        }

        let open_group = snapshot.open_group.clone();

        for group in snapshot.groups {
            let is_open = open_group.as_deref() == Some(group.app_key.as_str());
            let group_widget = self.build_group_widget(
                &group,
                is_open,
                on_close_notification.clone(),
                on_toggle_group.clone(),
                on_close_all_groups.clone(),
            );
            self.groups_list.append(&group_widget);
        }

        // If list is empty, optionally show an empty-state label.
        if snapshot.total_count == 0 {
            let empty = gtk::Label::builder()
                .label("No notifications")
                .css_classes(["dim-label"])
                .xalign(0.0)
                .margin_top(12)
                .build();
            self.groups_list.append(&empty);
        }
    }

    fn build_group_widget<F1, F2, F3>(
        &self,
        group: &NotificationGroup,
        is_open: bool,
        on_close_notification: F1,
        on_toggle_group: F2,
        _on_close_all_groups: F3,
    ) -> gtk::Widget
    where
        F1: Fn(u64) + Clone + 'static,
        F2: Fn(String) + Clone + 'static,
        F3: Fn() + Clone + 'static,
    {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();

        // Group header (app name)
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let app_label = gtk::Label::builder()
            .label(&group.display_app_name)
            .css_classes(["heading"])
            .xalign(0.0)
            .hexpand(true)
            .build();

        header.append(&app_label);
        container.append(&header);

        let count = group.notifications.len();
        if count == 0 {
            return container.upcast::<gtk::Widget>();
        }

        // Latest card always shown
        let latest = &group.notifications[0];
        let latest_card = self.build_notification_card(latest, on_close_notification.clone());
        container.append(&latest_card);

        // Expand button (only if more than one)
        let expand_btn = gtk::Button::builder()
            .css_classes(["flat"])
            .margin_bottom(0)
            .label(if is_open {
                "Show less".to_string()
            } else {
                format!("Show {} more", count.saturating_sub(1))
            })
            .halign(gtk::Align::Start)
            .build();

        if count <= 1 {
            expand_btn.set_visible(false);
        } else {
            let app_key = group.app_key.clone();
            expand_btn.connect_clicked(move |_| {
                on_toggle_group(app_key.clone());
            });
        }

        container.append(&expand_btn);

        // Remaining notifications in revealer
        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(is_open && count > 1)
            .build();

        let rest_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();

        for n in group.notifications.iter().skip(1) {
            let card = self.build_notification_card(n, on_close_notification.clone());
            rest_box.append(&card);
        }

        revealer.set_child(Some(&rest_box));
        container.append(&revealer);

        container.upcast::<gtk::Widget>()
    }

    fn build_notification_card<F>(&self, notification: &Notification, on_close: F) -> gtk::Widget
    where
        F: Fn(u64) + Clone + 'static,
    {
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["card", "notification-card"])
            .build();

        // Header with icon + app name + close
        let layout = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .spacing(12)
            .margin_start(16)
            .margin_end(16)
            .margin_top(16)
            .margin_bottom(16)
            .build();

        let icon = self.build_icon_image(&notification.icon);
        let spacer = gtk::Box::builder().hexpand(true).build();

        let close_btn = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat", "circular", "notification-close"])
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .build();

        // Main content (clickable)
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["notification-content"])
            .build();

        let title = gtk::Label::builder()
            .label(&notification.summary)
            .xalign(0.0)
            .wrap(true)
            .css_classes(["heading"])
            .build();

        let text = gtk::Label::builder()
            .label(&notification.body)
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();

        layout.append(&icon);
        layout.append(&content);
        layout.append(&spacer);
        layout.append(&close_btn);

        content.append(&title);
        content.append(&text);

        card.append(&layout);

        // Actions
        if !notification.actions.is_empty() {
            let actions_container = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .css_classes(["notification-actions-container"])
                .margin_top(8)
                .build();

            let separator = gtk::Separator::builder()
                .orientation(gtk::Orientation::Horizontal)
                .css_classes(["notification-separator"])
                .build();

            let actions_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(6)
                .margin_top(6)
                .margin_start(12)
                .margin_end(12)
                .margin_bottom(8)
                .build();

            for a in &notification.actions {
                let b = gtk::Button::builder()
                    .label(&a.label)
                    .css_classes(["pill", "notif-action"])
                    .build();
                let on_invoke = a.on_invoke.clone();
                b.connect_clicked(move |_| (on_invoke)());
                actions_box.append(&b);
            }

            actions_container.append(&separator);
            actions_container.append(&actions_box);
            card.append(&actions_container);
        }

        // Default action click
        if let Some(action) = &notification.on_default_action {
            let gesture = gtk::GestureClick::new();
            content.add_controller(gesture.clone());
            let action = action.clone();
            gesture.connect_pressed(move |_, _, _, _| (action)());
        }

        // Close button
        let id = notification.id;
        close_btn.connect_clicked(move |_| on_close(id));

        card.upcast::<gtk::Widget>()
    }

    fn build_icon_image(&self, icon: &NotificationIcon) -> gtk::Image {
        let img = gtk::Image::builder()
            .pixel_size(self.icon_size)
            .valign(gtk::Align::Start)
            .build();

        match icon {
            NotificationIcon::Themed(name) => {
                // Prefer symbolic if available by trying "-symbolic" variant first when not already.
                let display = match gdk::Display::default() {
                    Some(d) => d,
                    None => {
                        img.set_icon_name(Some(&self.default_themed_icon));
                        return img;
                    }
                };

                let theme = gtk::IconTheme::for_display(&display);
                let mut candidates: Vec<String> = vec![];

                if name.ends_with("-symbolic") {
                    candidates.push(name.clone());
                } else {
                    candidates.push(format!("{name}-symbolic"));
                    candidates.push(name.clone());
                }

                let mut chosen = None;
                for c in candidates {
                    if theme.has_icon(&c) {
                        chosen = Some(c);
                        break;
                    }
                }

                if let Some(chosen) = chosen {
                    img.set_icon_name(Some(&chosen));
                } else {
                    img.set_icon_name(Some(&self.default_themed_icon));
                }
            }
            NotificationIcon::FilePath(path) => {
                // Load and scale-to-fit (distortion allowed per requirement).
                //
                // Note: this is synchronous file IO/decoding. For production, you likely want
                // async loading + caching; keeping it simple for now.
                if let Ok(tex) = gdk::Texture::from_filename(path) {
                    img.set_paintable(Some(&tex));
                } else {
                    img.set_icon_name(Some(&self.default_themed_icon));
                }
            }
        }

        img
    }
}

/// A small controller that safely wires a `NotificationsModel` to a `NotificationsView`.
///
/// This replaces the previous unsafe self-referential callback wiring. The controller owns:
/// - the model
/// - the view
/// - the render function (as an `Rc<dyn Fn()>`) that callbacks can call to request a refresh
///
/// This structure also makes it straightforward to introduce debouncing later (by turning
/// `render_now()` into "schedule render" and coalescing multiple requests).
struct NotificationsController {
    model: Rc<RefCell<NotificationsModel>>,
    view: Rc<NotificationsView>,
    render_now: Rc<dyn Fn()>,
}

impl NotificationsController {
    fn new(initial: Vec<Notification>) -> Self {
        let model = Rc::new(RefCell::new(NotificationsModel::new()));
        for n in initial {
            model.borrow_mut().add(n);
        }

        let view = Rc::new(NotificationsView::new());

        // We create `render_now` in a two-step manner without unsafe code by capturing
        // an `Rc<RefCell<Option<Rc<dyn Fn()>>>>` indirection.
        let render_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

        let model_for_render = model.clone();
        let view_for_render = view.clone();
        let render_slot_for_render = render_slot.clone();

        let render_impl: Rc<dyn Fn()> = Rc::new(move || {
            let snapshot = model_for_render.borrow().snapshot();

            let render_now = render_slot_for_render
                .borrow()
                .as_ref()
                .expect("render_now must be initialized")
                .clone();

            let on_close_notification = {
                let model = model_for_render.clone();
                let render_now = render_now.clone();
                move |id: u64| {
                    model.borrow_mut().remove(id);
                    (render_now)();
                }
            };

            let on_toggle_group = {
                let model = model_for_render.clone();
                let render_now = render_now.clone();
                move |app_key: String| {
                    model.borrow_mut().toggle_open_group(&app_key);
                    (render_now)();
                }
            };

            let on_close_all_groups = {
                let model = model_for_render.clone();
                let render_now = render_now.clone();
                move || {
                    model.borrow_mut().set_open_group(None);
                    (render_now)();
                }
            };

            view_for_render.render_from_snapshot(
                snapshot,
                on_close_notification,
                on_toggle_group,
                on_close_all_groups,
            );
        });

        // Publish render_impl into the slot so render_impl can fetch it for callbacks.
        *render_slot.borrow_mut() = Some(render_impl.clone());

        let controller = Self {
            model: model.clone(),
            view: view.clone(),
            render_now: render_impl.clone(),
        };

        // Wire Clear to model.clear() + rerender.
        {
            let model = controller.model.clone();
            let render_now = controller.render_now.clone();
            controller.view.connect_clear(move || {
                model.borrow_mut().clear();
                (render_now)();
            });
        }

        controller
    }

    fn widget(&self) -> gtk::Widget {
        self.view.widget()
    }

    fn render_now(&self) {
        (self.render_now)();
    }
}

/// Convenience: create model+view controller-like assembly as a widget, from an initial list.
///
/// This is a transitional helper so callers can still do a one-shot build, but it is backed
/// by a real model and view.
pub fn build_notifications_section(notifications: Vec<Notification>) -> gtk::Widget {
    let controller = NotificationsController::new(notifications);
    controller.render_now();
    controller.widget()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: u64) -> SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs)
    }

    fn n(id: u64, app: &str, ts: u64) -> Notification {
        Notification::new(
            id,
            app.to_string(),
            format!("s{id}"),
            format!("b{id}"),
            t(ts),
            NotificationIcon::Themed("dialog-information-symbolic".to_string()),
        )
    }

    #[test]
    fn groups_by_normalized_app_name() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "Slack", 10));
        m.add(n(2, "slack", 11));
        m.add(n(3, "SLACK ", 12));
        m.add(n(4, "org.example.App", 13));

        let snap = m.snapshot();
        // "Slack"/"slack"/"SLACK " all normalize to "slack"
        assert_eq!(snap.groups.len(), 2);

        let keys: Vec<String> = snap.groups.iter().map(|g| g.app_key.clone()).collect();
        assert!(keys.contains(&"slack".to_string()));
        assert!(keys.contains(&"org.example.app".to_string()));
    }

    #[test]
    fn notifications_sorted_newest_first_within_group() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "Slack", 10));
        m.add(n(2, "Slack", 12));
        m.add(n(3, "Slack", 11));

        let snap = m.snapshot();
        let g = snap.groups.iter().find(|g| g.app_key == "slack").unwrap();

        let ids: Vec<u64> = g.notifications.iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn groups_sorted_by_latest_notification_newest_first() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "AppA", 10));
        m.add(n(2, "AppB", 20));
        m.add(n(3, "AppA", 30)); // AppA becomes newest group

        let snap = m.snapshot();
        assert_eq!(snap.groups.len(), 2);
        assert_eq!(snap.groups[0].app_key, "appa");
        assert_eq!(snap.groups[1].app_key, "appb");
    }

    #[test]
    fn only_one_group_open_at_a_time() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "AppA", 10));
        m.add(n(2, "AppB", 20));

        m.set_open_group(Some("appa".to_string()));
        assert_eq!(m.open_group(), Some("appa"));

        m.set_open_group(Some("appb".to_string()));
        assert_eq!(m.open_group(), Some("appb"));
    }

    #[test]
    fn removing_latest_reveals_next_latest_and_group_disappears_when_empty() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "Slack", 10));
        m.add(n(2, "Slack", 20)); // latest
        m.add(n(3, "Slack", 15));

        // Remove latest (id=2), next latest should be id=3
        assert!(m.remove(2));
        let snap = m.snapshot();
        let g = snap.groups.iter().find(|g| g.app_key == "slack").unwrap();
        assert_eq!(g.notifications[0].id, 3);

        // Remove remaining
        assert!(m.remove(3));
        assert!(m.remove(1));
        let snap = m.snapshot();
        assert!(snap.groups.is_empty());
        assert_eq!(snap.total_count, 0);
    }

    #[test]
    fn clear_removes_everything_and_closes_open_group() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "Slack", 10));
        m.add(n(2, "AppB", 20));
        m.set_open_group(Some("slack".to_string()));

        m.clear();
        let snap = m.snapshot();
        assert!(snap.groups.is_empty());
        assert_eq!(snap.total_count, 0);
        assert_eq!(snap.open_group, None);
    }
}
