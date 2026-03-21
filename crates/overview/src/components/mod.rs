pub mod agenda;
pub mod audio_sliders;
pub mod battery;
pub mod brightness_sliders;
pub mod claude;
pub mod entity_keyed_base;
pub mod calendar;
pub mod clock;
pub mod events;
pub mod keyboard_layout;
pub mod notification_group;
pub mod notification_list;
pub mod right_column_stack;
pub mod session_actions;
pub mod settings_button;
pub mod system_actions;
pub mod throttled_sender;
pub mod toggles;
pub mod weather;

/// Single GTK test entry point for component tests that create widgets.
///
/// GTK can only be initialized once per process on the main thread.
/// All GTK widget tests must run from this single `#[test]` function
/// to avoid thread contention on the GLib main context.
#[cfg(test)]
mod gtk_component_tests {
    use std::sync::Once;

    fn init_gtk() {
        static GTK_INIT: Once = Once::new();
        GTK_INIT.call_once(|| {
            gtk::init().expect("Failed to initialize GTK for tests");
        });
    }

    #[test]
    fn all_gtk_component_tests() {
        init_gtk();

        super::brightness_sliders::tests::run_all();
        super::audio_sliders::tests::run_all_gtk();
        super::entity_keyed_base::tests::run_all();
    }
}
