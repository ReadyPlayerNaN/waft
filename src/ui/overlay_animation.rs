//! Minimal fade animations for overlay content widgets (GTK4 + libadwaita).
//!
//! We intentionally animate the *content widget* (child), not the layer-shell surface.
//! This keeps things compositor-agnostic and avoids window-manager quirks.
//!
//! Typical usage:
//! - After `window.present()` + `window.set_visible(true)`:
//!   `fade_in_after_present(&content_widget, FadeConfig::default());`
//! - When dismissing:
//!   `fade_out(&content_widget, FadeConfig::default(), move || window.set_visible(false));`

use adw::prelude::*;

/// Private key used to store the currently running animation on a widget.
const DATA_KEY_ANIM: &str = "sacrebleui.overlay_animation.current";

/// Configuration for fade animations.
#[derive(Debug, Clone, Copy)]
pub struct FadeConfig {
    /// Duration in milliseconds.
    pub duration_ms: u32,
    /// Easing curve.
    pub easing: adw::Easing,
}

impl Default for FadeConfig {
    fn default() -> Self {
        Self {
            duration_ms: 200,
            easing: adw::Easing::EaseOutCubic,
        }
    }
}

/// Fade the widget in: opacity 0 → 1.
///
/// If another animation started by this module is running on the widget, it is replaced.
pub fn fade_in(widget: &gtk::Widget, cfg: FadeConfig) {
    stop_running_animation(widget);

    widget.set_opacity(0.0);

    let w = widget.clone();
    let target = adw::CallbackAnimationTarget::new(move |value: f64| {
        w.set_opacity(value.clamp(0.0, 1.0));
    });

    let anim = adw::TimedAnimation::new(widget, 0.0, 1.0, cfg.duration_ms, target);
    anim.set_easing(cfg.easing);

    {
        let w = widget.clone();
        anim.connect_done(move |_| {
            w.set_opacity(1.0);
        });
    }

    store_running_animation(widget, anim.clone());
    anim.play();
}

/// Convenience: schedule a fade-in on the next main-loop iteration.
///
/// This is useful right after `present()` for layer-shell overlays, so the widget is
/// actually mapped before the first frames of the animation.
pub fn fade_in_after_present(widget: &gtk::Widget, cfg: FadeConfig) {
    let widget = widget.clone();
    gtk::glib::idle_add_local_once(move || {
        fade_in(&widget, cfg);
    });
}

/// Fade the widget out: opacity 1 → 0, then call `on_done`.
///
/// If another animation started by this module is running on the widget, it is replaced.
pub fn fade_out<F>(widget: &gtk::Widget, cfg: FadeConfig, on_done: F)
where
    F: FnOnce() + 'static,
{
    stop_running_animation(widget);

    widget.set_opacity(1.0);

    let w = widget.clone();
    let target = adw::CallbackAnimationTarget::new(move |value: f64| {
        let v = value.clamp(0.0, 1.0);
        w.set_opacity(1.0 - v);
    });

    let anim = adw::TimedAnimation::new(widget, 0.0, 1.0, cfg.duration_ms, target);
    anim.set_easing(cfg.easing);

    {
        let w = widget.clone();
        let on_done: std::rc::Rc<std::cell::RefCell<Option<F>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Some(on_done)));

        anim.connect_done({
            let on_done = on_done.clone();
            move |_| {
                w.set_opacity(0.0);
                if let Some(cb) = on_done.borrow_mut().take() {
                    cb();
                }
            }
        });
    }

    store_running_animation(widget, anim.clone());
    anim.play();
}

/// Drops any currently running animation reference started by this module.
///
/// We don't attempt to pause/stop the underlying animation object; dropping our strong
/// reference is enough for our usage because we immediately replace the widget state.
pub fn stop_running_animation(widget: &gtk::Widget) {
    unsafe {
        if widget.data::<adw::TimedAnimation>(DATA_KEY_ANIM).is_some() {
            let _ = widget.steal_data::<adw::TimedAnimation>(DATA_KEY_ANIM);
        }
    }
}

fn store_running_animation(widget: &gtk::Widget, anim: adw::TimedAnimation) {
    unsafe {
        widget.set_data(DATA_KEY_ANIM, anim);
    }
}
