use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

use gtk::prelude::*;
use gtk4_layer_shell::LayerShell;

use crate::ui::overlay_animation::{self, FadeConfig};

use super::{
    card::NotificationCard,
    toast_policy::ToastRenderItem,
    types::{Notification, NotificationUrgency},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InsertPlacement {
    PrependTop,
    AppendBottom,
}

/// Pure helper: decide whether a newly-created toast row should be inserted at the top or bottom.
///
/// This is intentionally GTK-free so it can be unit-tested.
///
/// Semantics:
/// - Normally, new toasts appear at the top (most-recent-first).
/// - While an exit animation is in progress (i.e. a row is exiting and we suppress reordering),
///   "fill-in" toasts (older items becoming newly visible because a slot was freed) must appear
///   at the bottom to avoid the confusing `5,10,9,7,6` jump described in the bug report.
/// - Truly new incoming toasts must still appear at the top even during an exit animation. The
///   toast view is explicitly told which id was pushed via `note_pushed(id)`.
///
/// Inputs:
/// - `is_newly_visible`: true if this id was not visible in the previous frame.
/// - `exit_in_progress`: true if any existing row is exiting (or will exit this frame).
/// - `pushed_id`: the id recorded by `note_pushed`, if any.
/// - `id`: the id being inserted.
pub(crate) fn decide_insert_placement(
    is_newly_visible: bool,
    exit_in_progress: bool,
    pushed_id: Option<u64>,
    id: u64,
) -> InsertPlacement {
    if exit_in_progress && is_newly_visible {
        if pushed_id.is_some_and(|p| p == id) {
            // Brand-new incoming toast during exit: still goes to the top immediately.
            InsertPlacement::PrependTop
        } else {
            // Fill-in during exit: must appear from the bottom.
            InsertPlacement::AppendBottom
        }
    } else {
        // Normal path: most-recent-first.
        InsertPlacement::PrependTop
    }
}

/// Hardcoded toast window configuration (per requirements).
const TOAST_WIDTH_PX: i32 = 480;
const TOAST_SPACING_PX: i32 = 12;
const TOAST_MAX_VISIBLE: usize = 5;

/// A toast window that renders a stacked list of recent notifications (most recent on top),
/// using the same notification card look as the main notifications list.
///
/// Design goals:
/// - GTK-friendly: all UI runs on the main thread, no `Send + Sync` requirements.
/// - Single source of truth: the controller/plugin decides what is "active"; this view renders
///   a provided list of `Notification`s as toasts (subset of the main model/history).
/// - Behavior (hardcoded):
///   - Fixed width: 480px, top-centered on Wayland (best effort via layer-shell).
///   - Max visible toasts: 5.
///   - Auto-dismiss after a TTL (8s, or 16s if actions exist); `critical` never auto-dismisses.
///   - Hovering any toast pauses *all* dismiss timers; resume on leave.
///   - Close button dismisses globally via callback.
///   - Clicking the toast body attempts to activate the sender app (gio), then dismisses globally.
///   - Window fade-in/fade-out uses the same animation helpers as the main overlay.
pub struct ToastView {
    window: gtk::Window,
    container: gtk::Box,

    // State (shared so hover closures can safely capture)
    /// Currently visible toasts in render order (most recent first).
    visible_ids: Rc<RefCell<Vec<u64>>>,

    /// Effective pause state (derived from overlay + hover).
    paused: Rc<RefCell<bool>>,

    /// Whether hover pause is currently active (any toast hovered).
    hover_paused: Rc<RefCell<bool>>,

    /// Whether the overlay/suppression pause is currently active.
    overlay_paused: Rc<RefCell<bool>>,

    /// Reference count of hover-enter events across all toast cards.
    hover_count: Rc<RefCell<u32>>,

    /// Callback invoked when the pointer enters/leaves any toast card region.
    ///
    /// The plugin should use this to pause/resume pure toast-state timers (expiry only).
    on_hover_pause_changed: Rc<RefCell<Option<Rc<dyn Fn(bool)>>>>,

    /// Last toast id that was pushed into the toast stack due to a new incoming notification.
    ///
    /// This is used to distinguish:
    /// - brand-new incoming toasts (should appear at TOP immediately), from
    /// - fill-ins (older toasts becoming visible after a slot is freed; should appear at BOTTOM
    ///   while an exit animation is running and reordering is suppressed).
    last_pushed_id: Rc<RefCell<Option<u64>>>,

    /// Stable row wrappers keyed by notification id.
    ///
    /// This is the core of "incremental reconciliation":
    /// - we reuse existing widgets (no full rebuild) so progress bars stay smooth,
    /// - we can animate enter/exit per row without fighting GTK reallocation.
    rows: Rc<RefCell<HashMap<u64, ToastRow>>>,

    /// Whether we've installed the hover pause controller for the toast list container.
    hover_pause_installed: Cell<bool>,

    /// Currently hovered toast row id (if any).
    ///
    /// Semantics (per requirements):
    /// - If the pointer is over a toast card (or any of its children) -> pause ALL toast timers.
    /// - If the pointer is over spacing gap between toasts -> resume (act as if left the toast).
    /// - If the pointer is over toast window background outside container -> resume.
    /// - If a toast is animating, we still correctly track hovered row by mapping the picked widget
    ///   back to the owning `ToastRow` (using the row wrapper widget identity).
    hovered_row_id: Rc<RefCell<Option<u64>>>,

    /// Map of currently visible toast id -> card instance, so we can push timeout indicator
    /// updates without re-building widgets every render.
    cards: Rc<RefCell<HashMap<u64, NotificationCard>>>,

    /// Whether the toast window is suppressed (e.g. main overlay is visible/active).
    ///
    /// When suppressed:
    /// - we never show the window from `render()` / `sync_window_visibility()`,
    /// - we still update internal state + widgets so it can re-appear instantly,
    /// - timers should be paused by the caller (typically via `hide_with_pause()`).
    suppressed: Rc<RefCell<bool>>,

    /// Debounce/guard for fade-in so it doesn't get restarted on frequent `render()` calls.
    ///
    /// We only want to animate on an actual transition from "not visible" -> "visible".
    /// Re-triggering the fade-in while already visible looks like flicker.
    fade_in_armed: Rc<RefCell<bool>>,
}

impl ToastView {
    /// Create a toast window view.
    ///
    /// This MUST be called after GTK is initialized (and from the GTK main thread).
    pub fn new(app: &adw::Application) -> Self {
        // Use a plain GTK toplevel window. We keep it minimal and compositor-friendly.
        let window = gtk::Window::builder()
            .application(app)
            .decorated(false)
            .resizable(false)
            .title("sacrebleui-toasts")
            .build();

        // Hard constrain the toast window width so children (labels) cannot expand it horizontally.
        // This prevents long unbroken lines from stretching the window "to infinity".
        //
        // Also ensure the window never computes a zero height while rows are animating in/out.
        // Wayland/GTK may warn if a toplevel is asked to compute a size with height <= 0.
        const TOAST_MIN_HEIGHT_PX: i32 = 1;
        window.set_default_size(TOAST_WIDTH_PX, TOAST_MIN_HEIGHT_PX);
        window.set_size_request(TOAST_WIDTH_PX, TOAST_MIN_HEIGHT_PX);

        // Best-effort "don't steal focus".
        window.set_focusable(false);
        window.set_modal(false);

        // Layer-shell positioning (Wayland only by project design).
        window.init_layer_shell();
        window.set_layer(gtk4_layer_shell::Layer::Overlay);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);

        // Anchor to top, and let layer-shell place it. We'll set 0 top margin per requirement.
        window.set_anchor(gtk4_layer_shell::Edge::Top, true);
        window.set_anchor(gtk4_layer_shell::Edge::Left, false);
        window.set_anchor(gtk4_layer_shell::Edge::Right, false);
        window.set_margin(gtk4_layer_shell::Edge::Top, 0);

        // Root content widget that we fade in/out (matches overlay_animation’s usage).
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["toast-window-content"])
            .build();

        // Container for stacked cards (hardcoded config).
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(TOAST_SPACING_PX)
            .margin_top(0)
            .margin_bottom(0)
            .margin_start(0)
            .margin_end(0)
            .width_request(TOAST_WIDTH_PX)
            .build();

        // Hard constrain toast width so long text *wraps* instead of expanding the toast window.
        //
        // `width_request` is only a hint; `set_size_request` makes it a hard minimum and helps
        // ensure the labels are measured with a finite width during layout.
        root.set_size_request(TOAST_WIDTH_PX, -1);
        container.set_size_request(TOAST_WIDTH_PX, -1);

        root.append(&container);
        window.set_child(Some(&root));

        // Start hidden.
        window.set_visible(false);

        Self {
            window,
            container,

            visible_ids: Rc::new(RefCell::new(Vec::new())),
            paused: Rc::new(RefCell::new(false)),
            hover_paused: Rc::new(RefCell::new(false)),
            overlay_paused: Rc::new(RefCell::new(false)),
            hover_count: Rc::new(RefCell::new(0)),
            on_hover_pause_changed: Rc::new(RefCell::new(None)),

            last_pushed_id: Rc::new(RefCell::new(None)),

            rows: Rc::new(RefCell::new(HashMap::new())),
            hover_pause_installed: Cell::new(false),
            hovered_row_id: Rc::new(RefCell::new(None)),

            cards: Rc::new(RefCell::new(HashMap::new())),

            suppressed: Rc::new(RefCell::new(false)),
            fade_in_armed: Rc::new(RefCell::new(true)),
        }
    }

    /// Update toast rendering from a list of notifications.
    ///
    /// You are expected to pass **most-recent-first** ordering for toast selection.
    /// This view enforces `max_visible` and will only show the first N.
    ///
    /// `on_dismiss_global` MUST remove the notification from the underlying model and emit DBus
    /// `NotificationClosed` as appropriate (reason depends on the caller).
    /// Record that a brand-new toast (incoming notification) was pushed into the toast stack.
    ///
    /// Callers should invoke this right before rendering toasts after ingesting a new notification.
    /// This lets the view decide whether a newly created row should be placed at the top (new) or
    /// at the bottom (fill-in) while an exit animation is running and reordering is suppressed.
    pub fn note_pushed(&self, id: u64) {
        *self.last_pushed_id.borrow_mut() = Some(id);
    }

    /// Install a callback invoked when hover pause toggles on/off.
    ///
    /// The plugin should use this to pause/resume pure toast-state timers (expiry only).
    pub fn set_on_hover_pause_changed(&self, cb: Rc<dyn Fn(bool)>) {
        *self.on_hover_pause_changed.borrow_mut() = Some(cb);
    }

    /// Update timeout indicator progress for currently visible toast cards without re-rendering.
    ///
    /// This is intended to be called from a periodic tick in the plugin, while `render()` is only
    /// called on structural changes (push/remove/reorder) to avoid churn that breaks hover states.
    pub fn update_progress(&self, items_most_recent_first: &[ToastRenderItem<Notification>]) {
        let cards = self.cards.borrow();
        for item in items_most_recent_first {
            if let Some(card) = cards.get(&item.id) {
                card.set_timeout_progress(item.elapsed, item.ttl);
            }
        }
    }

    pub fn render<FActivateFallback, FDismissExpired, FDismissUser>(
        &self,
        toasts_most_recent_first: Vec<ToastRenderItem<Notification>>,
        on_activate_fallback: FActivateFallback,
        on_dismiss_expired: FDismissExpired,
        on_dismiss_user: FDismissUser,
    ) where
        FActivateFallback: Fn() + Clone + 'static,
        FDismissExpired: Fn(u64) + Clone + 'static,
        FDismissUser: Fn(u64) + Clone + 'static,
    {
        // Hardcoded view config.
        self.container.set_spacing(TOAST_SPACING_PX);
        self.container.set_width_request(TOAST_WIDTH_PX);

        // Keep the toast list constrained to the toast width so children (labels) wrap.
        self.container.set_size_request(TOAST_WIDTH_PX, -1);

        // Install hover pause once at the container level.
        //
        // We avoid per-row motion controllers (which can flicker during row reveal/collapse
        // animations), but we still want the correct semantics:
        // - pause only when the pointer is actually over a toast card
        // - resume when the pointer is over the spacing gap between toasts
        // - resume when the pointer is outside the toast window background (handled by leave)
        //
        // Implementation: single motion controller + hit-test pick -> map to `ToastRow` id by
        // walking up the parent chain and matching against known row wrapper widgets.
        if !self.hover_pause_installed.get() {
            self.hover_pause_installed.set(true);
            self.install_hover_pause_with_pick();
        }

        let desired: Vec<ToastRenderItem<Notification>> = toasts_most_recent_first
            .into_iter()
            .take(TOAST_MAX_VISIBLE)
            .collect();

        // Caller already provides most-recent-first.
        let desired_ids: Vec<u64> = desired.iter().map(|i| i.id).collect();

        // Track ids that are in the desired (visible) toast list.
        // Anything not in this set should be animated out (but kept in the UI during animation).
        let desired_set: HashSet<u64> = desired_ids.iter().copied().collect();

        // We need to distinguish between:
        // - brand-new incoming notifications (should appear at TOP immediately), and
        // - "fill-in" notifications that become visible only because a slot was freed (older ones,
        //   should appear from the BOTTOM when a removal/exit animation is in progress).
        //
        // IMPORTANT:
        // When the toast list is capped (max 5), older notifications are *not* visible even though
        // they exist. When a visible toast is removed, the next older notification becomes visible.
        // That "fill-in" must appear at the bottom, not jump to the top.
        //
        // The view cannot infer "new vs fill-in" from `desired_ids` alone. Instead, the plugin
        // calls `ToastView::note_pushed(id)` for truly new incoming notifications, and we consume
        // that id once when inserting the corresponding row.
        let prev_visible: HashSet<u64> = self.visible_ids.borrow().iter().copied().collect();
        let last_pushed_id = self.last_pushed_id.clone();

        // Create or reuse rows for desired ids; do not rebuild the container wholesale.
        for item in &desired {
            let id = item.id;
            let n = &item.payload;

            // If the row already exists (possibly in "removing" state), reuse it and cancel removal.
            if let Some(existing) = self.rows.borrow().get(&id) {
                existing.removing.set(false);
                continue;
            }

            let card = NotificationCard::new(
                n,
                on_dismiss_user.clone(),
                /* enable_timeout_indicator */ true,
                /* enable_right_click_dismiss */ true,
            );

            let card_widget = card.widget();

            // Toast-specific styling hooks.
            card_widget.add_css_class("toast-card");
            if n.urgency == NotificationUrgency::Critical {
                card_widget.add_css_class("toast-critical");
            }

            // Hover pause (global timers pause) + per-row hover styling.
            //
            // IMPORTANT:
            // - Hover pause must be installed exactly once per toast row; otherwise overlapping
            //   controllers can produce quick enter/leave flicker (pause for a fraction of a second).
            // - Attach hover pause to the *row wrapper* (Revealer), not the inner card widget.
            //   With Revealers/animations, enter/leave on the child can be unreliable.
            //
            // We still apply the visual hover class to the card widget.
            install_row_hover_style(&card_widget);

            // Default action click: activate sender app (gio), otherwise fallback; then dismiss globally.
            let desktop_entry = n.desktop_entry.clone();
            let app_name = n.app_name.clone();
            let on_activate_fallback2 = on_activate_fallback.clone();
            let on_dismiss_user2 = on_dismiss_user.clone();
            {
                let gesture = gtk::GestureClick::new();
                card_widget.add_controller(gesture.clone());
                gesture.connect_pressed(move |_, _, _, _| {
                    let activated = try_activate_desktop_entry(desktop_entry.as_deref())
                        .or_else(|| try_activate_guess_from_app_name(&app_name))
                        .unwrap_or(false);

                    if !activated {
                        (on_activate_fallback2)();
                    }

                    (on_dismiss_user2)(id);
                });
            }

            // Row wrapper to support enter/exit height animations.
            let row = ToastRow::new(&card_widget);

            // Hover pause is installed once at the container level (see start of `render()`).
            // Per-row hover controllers caused transient enter/leave flicker during animations.

            // Store: card for ticking updates, row for stable widget + animations.
            self.cards.borrow_mut().insert(id, card);
            self.rows.borrow_mut().insert(id, row);

            // Add to container (revealer starts collapsed), then animate in.
            //
            // IMPORTANT ORDERING (must not break enter/exit animations):
            //
            // - Truly new notifications should appear at the TOP immediately (most-recent-first).
            // - "Fill-in" notifications (older ones that become newly visible because a slot was freed)
            //   should appear at the BOTTOM if an exit animation is currently running (because we skip
            //   reordering during exits to avoid confusing shifts).
            //
            // Heuristic:
            // - If `id` was previously visible, this is not a new insertion (but we wouldn't be here).
            // - If `id` was NOT previously visible:
            //     - if there is any removing row, treat it as a fill-in and append to bottom,
            //     - otherwise treat it as truly new and prepend to top.
            let w = self
                .rows
                .borrow()
                .get(&id)
                .expect("row just inserted")
                .root
                .clone();

            // Detect whether an exit animation is currently running.
            //
            // IMPORTANT:
            // Checking `row.removing` here is too early for the "fill-in placement" decision:
            // - We haven't marked any rows as removing yet in this render pass.
            // - Therefore `any_exit_in_progress` would often be false, causing fill-ins (e.g. id=5)
            //   to be inserted at the top, which is exactly the bug.
            //
            // Instead, treat "exit in progress for this frame" as:
            // - any existing row that is currently removing, OR
            // - any existing row that will be removed (present in rows, absent from desired_set).
            let any_exit_in_progress = self
                .rows
                .borrow()
                .iter()
                .any(|(rid, row)| row.removing.get() || !desired_set.contains(rid));

            // If the id wasn't previously visible, it's either:
            // - a brand-new incoming notification, or
            // - a fill-in (older) notification becoming visible after a removal.
            let is_newly_visible = !prev_visible.contains(&id);

            let pushed_id = *last_pushed_id.borrow();
            let placement =
                decide_insert_placement(is_newly_visible, any_exit_in_progress, pushed_id, id);

            // Consume the pushed marker once we used it, so it doesn't affect later fill-ins.
            if pushed_id.is_some_and(|p| p == id) {
                *last_pushed_id.borrow_mut() = None;
            }

            match placement {
                InsertPlacement::PrependTop => self.container.prepend(&w),
                InsertPlacement::AppendBottom => self.container.append(&w),
            }

            // Ensure the timeout bar becomes visible immediately once layout is available.
            //
            // The timeout fill width computation depends on allocated width, which may be 0 on the
            // very first frame. Force one post-layout update so the bar doesn't remain invisible
            // until the first interaction (e.g. hover) triggers a new allocation.
            let cards_for_init = self.cards.clone();
            let desired_for_init = desired.clone();
            gtk::glib::idle_add_local_once(move || {
                for item in &desired_for_init {
                    if let Some(card) = cards_for_init.borrow().get(&item.id) {
                        card.set_timeout_progress(item.elapsed, item.ttl);
                    }
                }
            });

            // Enter animation: reveal (grow) within 200ms.
            if let Some(r) = self.rows.borrow().get(&id) {
                r.animate_in();
            }
        }

        // Mark rows not in desired as removing (animate out), but keep them in-place for ~200ms.
        //
        // IMPORTANT:
        // - Do NOT drop their state here; the removal callback will do that after animation.
        // - Do NOT remove/reinsert them during reordering; keep them stable in the container.
        //
        // Additionally, if ANY row is currently removing, we skip reordering entirely for this
        // render pass. Reordering while an exit animation runs can look like "data shifted by one",
        // because widgets move to fill gaps while the animation is still collapsing a different row.
        let mut any_removing = false;
        {
            let ids_now: Vec<u64> = self.rows.borrow().keys().copied().collect();
            for id in ids_now {
                if let Some(r) = self.rows.borrow().get(&id) {
                    if r.removing.get() {
                        any_removing = true;
                    }
                }

                if desired_set.contains(&id) {
                    continue;
                }

                // If already removing, don't restart animation.
                let already = self
                    .rows
                    .borrow()
                    .get(&id)
                    .is_some_and(|r| r.removing.get());
                if already {
                    continue;
                }

                if let Some(r) = self.rows.borrow().get(&id) {
                    r.removing.set(true);
                    any_removing = true;

                    // Ensure the removing row stays in the container where it is, then collapse it.
                    // (If it isn't currently parented for some reason, append it so the user still sees
                    // the collapse animation.)
                    let w = r.root.clone();
                    if w.parent().is_none() {
                        self.container.append(&w);
                    }

                    r.animate_out_then_remove(
                        self.container.clone(),
                        self.rows.clone(),
                        self.cards.clone(),
                        id,
                    );
                }
            }
        }

        // Reorder children to match desired_ids (most recent first).
        //
        // IMPORTANT:
        // If ANY exit animation is in progress, do not reorder at all. This prevents the container
        // from reshuffling rows underneath the collapsing revealer, which looks like the card data
        // "shifts" and the last row collapses instead of the intended one.
        if !any_removing {
            for id in desired_ids.iter().rev() {
                if let Some(r) = self.rows.borrow().get(id) {
                    let w = r.root.clone();
                    if w.parent().is_some() {
                        self.container.remove(&w);
                    }
                    self.container.prepend(&w);
                }
            }
        }

        // Update visible ids (only desired; removing rows are not "visible").
        *self.visible_ids.borrow_mut() = desired_ids;

        // Ensure window visibility matches whether we have any toasts and whether it should be shown.
        self.sync_window_visibility();

        // Timeout ticking/expiry is handled by pure state in the plugin layer. The view only renders.
        let _ = on_dismiss_expired;
    }

    /// Hide toasts (e.g. when main window becomes visible/focused). Keeps stack and pauses timers.
    ///
    /// IMPORTANT: this also enables "suppressed" mode, which prevents `render()` from immediately
    /// re-showing the toast window and causing fade loops.
    pub fn hide_with_pause(&self) {
        self.set_suppressed(true);
        self.set_overlay_paused(true);

        // Re-arm fade-in so the next real show transition animates once.
        *self.fade_in_armed.borrow_mut() = true;

        let cfg = FadeConfig::default();
        let w = self.window.clone();
        if let Some(child) = w.child() {
            overlay_animation::fade_out(&child, cfg, move || {
                w.set_visible(false);
            });
        } else {
            self.window.set_visible(false);
        }
    }

    /// Show toasts again (e.g. when main window hides). Respects whether there are any toasts.
    ///
    /// This disables suppression and re-shows (if there is content).
    pub fn show_if_any(&self) {
        self.set_suppressed(false);
        self.set_overlay_paused(false);

        if self.visible_ids.borrow().is_empty() {
            return;
        }

        // Only animate on a real "not visible -> visible" transition, and only once per arm.
        let was_visible = self.window.is_visible();

        self.window.present();
        self.window.set_visible(true);

        let should_fade_in = !was_visible && *self.fade_in_armed.borrow();
        if should_fade_in {
            *self.fade_in_armed.borrow_mut() = false;

            let cfg = FadeConfig::default();
            if let Some(child) = self.window.child() {
                overlay_animation::fade_in_after_present(&child, cfg);
            }
        }
    }

    fn sync_window_visibility(&self) {
        // When suppressed (e.g. main overlay visible), never show the toast window from inside
        // `render()`. This prevents animation loops where the plugin hides the window but the
        // next `render()` immediately re-shows it.
        if *self.suppressed.borrow() {
            return;
        }

        if self.visible_ids.borrow().is_empty() {
            // No content; hide.
            //
            // Also re-arm fade-in so the next show animates once.
            *self.fade_in_armed.borrow_mut() = true;

            let cfg = FadeConfig::default();
            let w = self.window.clone();
            if w.is_visible() {
                if let Some(child) = w.child() {
                    overlay_animation::fade_out(&child, cfg, move || {
                        w.set_visible(false);
                    });
                } else {
                    w.set_visible(false);
                }
            }
            return;
        }

        // Has content; show if not visible.
        if !self.window.is_visible() {
            self.window.present();
            self.window.set_visible(true);

            // Only animate if armed (prevents restart loops if something keeps toggling visibility).
            if *self.fade_in_armed.borrow() {
                *self.fade_in_armed.borrow_mut() = false;

                let cfg = FadeConfig::default();
                if let Some(child) = self.window.child() {
                    overlay_animation::fade_in_after_present(&child, cfg);
                }
            }
        }
    }

    fn install_hover_pause_with_pick(&self) {
        // Single motion controller on the container, with hit-testing to decide if we're
        // actually over a toast card, and mapping the pick result to a *specific* toast row id.
        //
        // Required semantics:
        // - Over toast card (or any child) => pause ALL toast timers
        // - Over spacing gap => resume
        // - Over toast window background outside container => resume (handled by leave)
        // - While animating, still map to correct hovered row (we use the row wrapper identity)
        let controller = gtk::EventControllerMotion::new();

        // Clone stable handles for 'static closures (we must not capture `&self`).
        let container_widget: gtk::Widget = self.container.clone().upcast::<gtk::Widget>();
        let rows = self.rows.clone();
        let view = self.clone_for_hover();
        let hovered_row_id = self.hovered_row_id.clone();

        // Map the picked widget at (x, y) to an owning toast row id (if any).
        //
        // Strategy (N is tiny: <= 5 visible toasts + at most a few exiting):
        // 1) Pick widget at pointer.
        // 2) Walk up parents to find a `gtk::Revealer` (toast row wrapper).
        // 3) Find the matching row id by comparing widget identity against `ToastRow.root`.
        let update_from_xy = move |x: f64, y: f64| {
            let picked = container_widget.pick(x, y, gtk::PickFlags::DEFAULT);

            // Find ancestor that is a `gtk::Revealer` (our row wrapper).
            let mut ancestor_revealer: Option<gtk::Revealer> = None;
            if let Some(mut w) = picked {
                loop {
                    if let Ok(r) = w.clone().downcast::<gtk::Revealer>() {
                        ancestor_revealer = Some(r);
                        break;
                    }
                    if let Some(p) = w.parent() {
                        w = p;
                    } else {
                        break;
                    }
                }
            }

            // Resolve revealer -> row id (by widget identity).
            let mut new_hovered_id: Option<u64> = None;
            if let Some(rev) = ancestor_revealer {
                for (id, row) in rows.borrow().iter() {
                    if row.root == rev {
                        new_hovered_id = Some(*id);
                        break;
                    }
                }
            }

            // Apply semantics:
            // - If over a row => pause + record hovered row id
            // - If not over any row (gap) => resume + clear hovered row id
            let prev = *hovered_row_id.borrow();
            if new_hovered_id.is_some() {
                *hovered_row_id.borrow_mut() = new_hovered_id;

                let mut c = view.hover_count.borrow_mut();
                if *c == 0 {
                    *c = 1;
                    view.set_hover_paused(true);
                }
            } else {
                if prev.is_some() {
                    *hovered_row_id.borrow_mut() = None;
                }

                let mut c = view.hover_count.borrow_mut();
                if *c != 0 {
                    *c = 0;
                    view.set_hover_paused(false);
                }
            }
        };

        // Enter: engage immediately (otherwise you only pause after the first motion event).
        let update_from_xy_enter = update_from_xy.clone();
        controller.connect_enter(move |_, x, y| {
            update_from_xy_enter(x, y);
        });

        // Motion: continuously update based on hit-test (handles gaps correctly).
        controller.connect_motion(move |_, x, y| {
            update_from_xy(x, y);
        });

        // Leave container => resume.
        let view_leave = self.clone_for_hover();
        let hovered_row_id_leave = self.hovered_row_id.clone();
        controller.connect_leave(move |_| {
            *hovered_row_id_leave.borrow_mut() = None;
            *view_leave.hover_count.borrow_mut() = 0;
            view_leave.set_hover_paused(false);
        });

        self.container.add_controller(controller);
    }

    fn set_paused(&self, paused: bool) {
        let mut p = self.paused.borrow_mut();
        if *p == paused {
            return;
        }
        *p = paused;
    }

    fn recompute_paused(&self) {
        let hover = *self.hover_paused.borrow();
        let overlay = *self.overlay_paused.borrow();
        self.set_paused(hover || overlay);
    }

    fn set_overlay_paused(&self, overlay_paused: bool) {
        let mut op = self.overlay_paused.borrow_mut();
        if *op == overlay_paused {
            return;
        }
        *op = overlay_paused;
        drop(op);
        self.recompute_paused();
    }

    fn set_suppressed(&self, suppressed: bool) {
        let mut s = self.suppressed.borrow_mut();
        if *s == suppressed {
            return;
        }
        *s = suppressed;
    }

    fn clone_for_hover(&self) -> ToastViewHoverLite {
        ToastViewHoverLite {
            hover_count: self.hover_count.clone(),
            paused: self.paused.clone(),
            hover_paused: self.hover_paused.clone(),
            overlay_paused: self.overlay_paused.clone(),
            on_hover_pause_changed: self.on_hover_pause_changed.clone(),
        }
    }
}

/// Lightweight handle used by the timer callback.
///
/// This avoids capturing the full `ToastView` (and its `gtk::Window`) in a periodic closure.
#[derive(Clone)]
struct ToastViewHoverLite {
    hover_count: Rc<RefCell<u32>>,
    paused: Rc<RefCell<bool>>,
    hover_paused: Rc<RefCell<bool>>,
    overlay_paused: Rc<RefCell<bool>>,
    on_hover_pause_changed: Rc<RefCell<Option<Rc<dyn Fn(bool)>>>>,
}

impl ToastViewHoverLite {
    fn recompute_paused(&self) {
        let hover = *self.hover_paused.borrow();
        let overlay = *self.overlay_paused.borrow();
        let paused = hover || overlay;

        let mut p = self.paused.borrow_mut();
        if *p == paused {
            return;
        }
        *p = paused;
    }

    fn set_hover_paused(&self, hover_paused: bool) {
        let mut hp = self.hover_paused.borrow_mut();
        if *hp == hover_paused {
            return;
        }
        *hp = hover_paused;
        drop(hp);

        // Inform the plugin so it can pause/resume pure toast-state expiry timers.
        if let Some(cb) = self.on_hover_pause_changed.borrow().as_ref().cloned() {
            (cb)(hover_paused);
        }

        self.recompute_paused();
    }
}

fn install_row_hover_style(card_root: &gtk::Widget) {
    // Hover styling for a single toast row (visual feedback), independent of global pause logic.
    let motion = gtk::EventControllerMotion::new();

    let w_enter = card_root.clone();
    motion.connect_enter(move |_, _, _| {
        w_enter.add_css_class("toast-hover");
    });

    let w_leave = card_root.clone();
    motion.connect_leave(move |_| {
        w_leave.remove_css_class("toast-hover");
    });

    card_root.add_controller(motion);
}

#[derive(Clone)]
struct ToastRow {
    /// Wrapper widget that is inserted into `ToastView.container`.
    /// We use a Revealer for reliable "height" enter/exit animations.
    root: gtk::Revealer,
    removing: Rc<Cell<bool>>,
}

impl ToastRow {
    fn new(child: &gtk::Widget) -> Self {
        // Child box exists so we can keep a stable CSS hook if needed.
        let child_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(false)
            .css_classes(["toast-row"])
            .build();
        child_box.append(child);

        let root = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();
        root.set_child(Some(&child_box));

        Self {
            root,
            removing: Rc::new(Cell::new(false)),
        }
    }

    fn animate_in(&self) {
        // Height-based enter animation (grow from 0 -> full height).
        //
        // We defer the reveal to the next main-loop iteration so the revealer has a chance to be
        // realized/allocated first. Without this, GTK can apply the final state immediately
        // (no visible transition) when the widget is newly inserted.
        self.root.set_transition_duration(200);
        self.root
            .set_transition_type(gtk::RevealerTransitionType::SlideDown);

        // Ensure we start collapsed.
        self.root.set_reveal_child(false);

        let revealer = self.root.clone();
        gtk::glib::idle_add_local_once(move || {
            revealer.set_reveal_child(true);
        });
    }

    fn animate_out_then_remove(
        &self,
        container: gtk::Box,
        rows: Rc<RefCell<HashMap<u64, ToastRow>>>,
        cards: Rc<RefCell<HashMap<u64, NotificationCard>>>,
        id: u64,
    ) {
        // Height-based exit animation (shrink to 0 height within 200ms), then remove widgets and state.
        let revealer = self.root.clone();
        revealer.set_transition_duration(200);
        revealer.set_transition_type(gtk::RevealerTransitionType::SlideUp);
        revealer.set_reveal_child(false);

        // After the transition, remove the row and its state.
        gtk::glib::timeout_add_local(Duration::from_millis(210), move || {
            if revealer.parent().is_some() {
                container.remove(&revealer);
            }
            rows.borrow_mut().remove(&id);
            cards.borrow_mut().remove(&id);
            gtk::glib::ControlFlow::Break
        });
    }
}

/// Best-effort activate app from a `.desktop` id using gio.
///
/// Returns `Some(true)` if activation was attempted and reported success,
/// `Some(false)` if activation was attempted but failed,
/// `None` if no desktop entry provided.
fn try_activate_desktop_entry(desktop_entry: Option<&str>) -> Option<bool> {
    let desktop_entry = desktop_entry?;
    let id = desktop_entry.trim();
    if id.is_empty() {
        return Some(false);
    }

    // Use DesktopAppInfo if available. This requires the `.desktop` id, with or without ".desktop".
    let mut candidates = vec![id.to_string()];
    if !id.ends_with(".desktop") {
        candidates.push(format!("{id}.desktop"));
    }

    for c in candidates {
        if let Some(app) = gio::DesktopAppInfo::new(&c) {
            // Best-effort activation: launch/activate via AppInfo.
            // On Wayland this is compositor-mediated; `launch` is the portable option.
            let launched = app.launch(&[], None::<&gio::AppLaunchContext>).is_ok();
            return Some(launched);
        }
    }

    Some(false)
}

/// Best-effort guess desktop entry from a human app name.
///
/// This is heuristic and may fail; it's intentionally conservative.
fn try_activate_guess_from_app_name(app_name: &str) -> Option<bool> {
    let name = app_name.trim();
    if name.is_empty() {
        return None;
    }

    // Simple normalization: lowercase, spaces/underscores => '-', strip some punctuation.
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;

    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        let is_sep = c.is_ascii_whitespace() || c == '_' || c == '-';
        if is_sep {
            if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
            continue;
        }
        if c.is_ascii_alphanumeric() || c == '.' {
            out.push(c);
            prev_dash = false;
        }
    }

    if out.is_empty() {
        return None;
    }

    // Common pattern: app-id like "org.gnome.Nautilus" won't be derivable from "Files",
    // but for names like "Slack" this at least tries "slack.desktop".
    try_activate_desktop_entry(Some(&out)).or(Some(false))
}
