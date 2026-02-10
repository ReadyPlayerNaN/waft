//! Internationalization (i18n) support using Fluent.
//!
//! This module provides translation functions for the UI.
//! It delegates to [`waft_i18n::I18n`] with the plugin-api's embedded FTL files.
//!
//! It is shared across all plugins (each cdylib gets its own copy with
//! its own `OnceLock`, which is fine — the overhead is minimal).

use std::sync::OnceLock;
use waft_i18n::I18n;

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/main.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/main.ftl")),
        ])
    })
}

/// Initialize the i18n system.
///
/// This should be called early in application startup, after config loading.
/// It detects the system locale and loads the appropriate translations.
pub fn init() {
    let _ = i18n();
}

/// Translate a message by ID.
///
/// Returns the message ID if translation is not found.
pub fn t(id: &str) -> String {
    i18n().t(id)
}

/// Translate a message with arguments.
///
/// Returns the message ID if translation is not found.
pub fn t_args(id: &str, args: &[(&str, &str)]) -> String {
    i18n().t_args(id, args)
}
