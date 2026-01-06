use std::time::Duration;

use adw::prelude::*;
use gtk;

use super::types::{Notification, NotificationIcon, NotificationUrgency};

/// View-only notification card that can be embedded in both:
/// - the main notifications history list, and
/// - the toast window list.
///
/// This widget intentionally does *not* own any timer logic.
///
/// Toast countdown/TTL state is expected to be computed by the toast view/controller and then
/// pushed into the card via `set_timeout_progress_*`.
///
/// ## Timeout indicator
///
/// - Implemented as a thin (2px) bar on the bottom edge.
/// - If actions exist, it is placed between the main content and the actions container.
/// - CSS-driven `gtk::Box` (not `gtk::ProgressBar`).
/// - Width is updated by setting a pixel width request computed from the card's allocated width.
/// - No special handling for rounded corners (by request).
///
/// ## Interactions
///
/// - Close button calls `on_close(id)`.
/// - Optional default action click on the content area:
///   - If `notification.on_default_action` is Some, clicking the content triggers it.
/// - Optional right-click dismissal (toast policy): if enabled, right-click triggers `on_close(id)`.
pub struct NotificationCard {
    root: gtk::Box,

    /// The timeout bar container (present in the widget tree only when enabled).
    timeout_bar: gtk::Box,

    /// Fixed-position container used to smoothly "shrink" the fill by setting an x-aligned width
    /// without forcing the whole card to relayout.
    timeout_fixed: gtk::Fixed,

    /// The shrinking bar itself (child of `timeout_fixed`).
    timeout_fill: gtk::Box,

    /// Whether the timeout indicator is enabled for this card instance.
    timeout_enabled: bool,

    /// Cached last-known normalized progress to avoid redundant updates.
    last_progress: std::cell::Cell<Option<f32>>,
}

impl NotificationCard {
    /// Build a notification card.
    ///
    /// - `enable_timeout_indicator`: whether this card should show/update the toast timeout bar.
    ///   In the main history list you typically pass `false`.
    ///
    /// - `enable_right_click_dismiss`: if true, right click dismisses the notification by calling
    ///   `on_close(id)` (toast behavior).
    pub fn new<FOnClose>(
        notification: &Notification,
        on_close: FOnClose,
        enable_timeout_indicator: bool,
        enable_right_click_dismiss: bool,
    ) -> Self
    where
        FOnClose: Fn(u64) + Clone + 'static,
    {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["card", "notification-card"])
            .build();

        // Timeout indicator (thin 2px bar).
        //
        // Placement policy:
        // - If actions exist: between main content and the actions container.
        // - Otherwise: bottom edge of the card.
        let timeout_bar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .height_request(2)
            .css_classes(["notification-timeout-bar"])
            .build();

        // We render the fill inside a `gtk::Fixed` so we can update its width smoothly without
        // relying on transforms (not available in this gtk-rs version) and without forcing the
        // rest of the card to be rebuilt.
        let timeout_fixed = gtk::Fixed::new();
        timeout_fixed.set_hexpand(true);
        timeout_fixed.set_vexpand(false);

        let timeout_fill = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .height_request(2)
            .css_classes(["notification-timeout-fill"])
            .build();

        // Pin the fill to the left edge; width will be adjusted in `set_timeout_progress_fraction`.
        timeout_fixed.put(&timeout_fill, 0.0, 0.0);

        timeout_bar.append(&timeout_fixed);

        // Header with icon + content + close
        //
        // IMPORTANT for toasts:
        // Make sure the horizontal layout can shrink; otherwise long labels may request
        // more width and cause the toast window to expand.
        let layout = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .spacing(12)
            .margin_start(16)
            .margin_end(16)
            .margin_top(16)
            .margin_bottom(16)
            .build();

        let icon = build_icon_image(&notification.icon);
        let spacer = gtk::Box::builder().hexpand(true).build();

        let close_btn = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat", "circular", "notification-close"])
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .build();

        // Main content (clickable)
        //
        // IMPORTANT for toasts:
        // Ensure the content area is willing to shrink horizontally so long text wraps
        // instead of forcing the toast/card to grow wider than the toast window.
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["notification-content"])
            .hexpand(true)
            .build();

        // IMPORTANT:
        // Prevent long unbroken strings from forcing the toast/card to request infinite width.
        // We prefer wrapping; ellipsize is a last-resort fallback.
        content.set_halign(gtk::Align::Fill);

        let title = gtk::Label::builder()
            .label(&notification.summary)
            .xalign(0.0)
            .wrap(true)
            .css_classes(["heading"])
            .build();

        // Long titles must wrap (word/char) rather than stretching the toast.
        title.set_wrap(true);
        title.set_wrap_mode(gtk::pango::WrapMode::WordChar);

        let text = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();

        // Long bodies must wrap (word/char) rather than stretching the toast.
        text.set_wrap(true);
        text.set_wrap_mode(gtk::pango::WrapMode::WordChar);

        // Render notification body as markup (`body-markup` capability).
        text.set_use_markup(true);
        text.set_markup(&notification.body);

        layout.append(&icon);
        layout.append(&content);
        layout.append(&spacer);
        layout.append(&close_btn);

        content.append(&title);
        content.append(&text);

        root.append(&layout);

        // Actions
        if !notification.actions.is_empty() {
            // If enabled, place the timeout bar between content and actions container.
            if enable_timeout_indicator {
                root.append(&timeout_bar);
            }

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
            root.append(&actions_container);
        } else if enable_timeout_indicator {
            // No actions: place the timeout bar on the bottom edge.
            root.append(&timeout_bar);
        }

        // Default action click (main list behavior).
        if let Some(action) = &notification.on_default_action {
            let gesture = gtk::GestureClick::new();
            content.add_controller(gesture.clone());
            let action = action.clone();
            gesture.connect_pressed(move |_, _, _, _| (action)());
        }

        // Right-click dismiss (toast behavior).
        if enable_right_click_dismiss {
            let id = notification.id;
            let on_close2 = on_close.clone();

            let gesture = gtk::GestureClick::new();
            // Button 3 = right click.
            gesture.set_button(3);
            root.add_controller(gesture.clone());
            gesture.connect_pressed(move |_, _, _, _| {
                (on_close2)(id);
            });
        }

        // Close button.
        let id = notification.id;
        let on_close3 = on_close.clone();
        close_btn.connect_clicked(move |_| on_close3(id));

        // CSS class hook for critical urgency (toast styling can also use this).
        if notification.urgency == NotificationUrgency::Critical {
            root.add_css_class("notification-critical");
        }

        // NOTE:
        // We intentionally do not hook into size-allocate signals here.
        //
        // The timeout fill width is updated by `set_timeout_progress_*` (called periodically by the
        // toast view tick), and it uses the bar's current allocated width at that time.
        let card = Self {
            root,
            timeout_bar,
            timeout_fixed,
            timeout_fill,
            timeout_enabled: enable_timeout_indicator,
            last_progress: std::cell::Cell::new(None),
        };

        // Ensure a sensible initial state.
        if enable_timeout_indicator {
            card.set_timeout_progress_fraction(1.0);
        }

        card
    }

    /// Get the root widget to pack into containers.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }

    /// Show or hide the timeout indicator widgets (only meaningful when timeout was enabled).
    pub fn set_timeout_indicator_visible(&self, visible: bool) {
        if !self.timeout_enabled {
            return;
        }
        self.timeout_bar.set_visible(visible);
    }

    /// Update timeout progress based on elapsed and total durations.
    ///
    /// If `total` is `None`, the notification is "never expires" and the indicator is hidden.
    pub fn set_timeout_progress(&self, elapsed: Duration, total: Option<Duration>) {
        if !self.timeout_enabled {
            return;
        }

        let Some(total) = total else {
            self.set_timeout_indicator_visible(false);
            return;
        };

        let total_ms = total.as_millis() as f64;
        let elapsed_ms = elapsed.as_millis() as f64;
        let remaining = (total_ms - elapsed_ms).max(0.0);

        let frac = if total_ms <= 0.0 {
            0.0
        } else {
            (remaining / total_ms).clamp(0.0, 1.0)
        };

        self.set_timeout_indicator_visible(true);
        self.set_timeout_progress_fraction(frac as f32);
    }

    /// Update timeout progress by directly providing remaining fraction in [0..=1].
    ///
    /// `1.0` means full bar (just shown); `0.0` means expired.
    ///
    /// Implementation detail:
    /// We control the fill width inside a `gtk::Fixed` so updates are smooth and don't rely on
    /// widget transform APIs.
    pub fn set_timeout_progress_fraction(&self, remaining_fraction: f32) {
        if !self.timeout_enabled {
            return;
        }

        let frac = remaining_fraction.clamp(0.0, 1.0);

        // Avoid redundant work.
        if self
            .last_progress
            .get()
            .is_some_and(|prev| (prev - frac).abs() < 0.0001)
        {
            return;
        }
        self.last_progress.set(Some(frac));

        // Compute target width from the bar's allocated width.
        //
        // Note: if not allocated yet, keep it "full" and let the next tick update precisely.
        let bar_w = self.timeout_bar.allocated_width().max(0);
        if bar_w == 0 {
            self.timeout_fill.set_size_request(-1, 2);
            self.timeout_fixed.move_(&self.timeout_fill, 0.0, 0.0);
            return;
        }

        let target_w = ((bar_w as f32) * frac).round().max(0.0) as i32;

        // Pin to left edge and only change the fill width (height stays 2px).
        self.timeout_fixed.move_(&self.timeout_fill, 0.0, 0.0);
        self.timeout_fill.set_size_request(target_w, 2);
    }
}

fn build_icon_image(icon: &NotificationIcon) -> gtk::Image {
    let img = gtk::Image::builder()
        .pixel_size(32)
        .valign(gtk::Align::Start)
        .build();

    match icon {
        NotificationIcon::Themed(name) => {
            img.set_icon_name(Some(name));
        }
        NotificationIcon::FilePath(path) => {
            if let Ok(tex) = gtk::gdk::Texture::from_filename(path) {
                img.set_paintable(Some(&tex));
            } else {
                img.set_icon_name(Some("dialog-information-symbolic"));
            }
        }
    }

    img
}
