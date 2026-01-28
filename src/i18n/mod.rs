//! Internationalization (i18n) support using Fluent.
//!
//! This module provides translation functions for the UI.

use fluent_bundle::concurrent::FluentBundle as ConcurrentFluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use std::sync::OnceLock;
use unic_langid::LanguageIdentifier;

/// Global bundle for translations (thread-safe concurrent version).
static BUNDLE: OnceLock<ConcurrentFluentBundle<FluentResource>> = OnceLock::new();

/// Initialize the i18n system.
///
/// This should be called early in application startup, after config loading.
/// It detects the system locale and loads the appropriate translations.
pub fn init() {
    BUNDLE.get_or_init(|| {
        let locale = detect_locale();
        load_bundle(&locale)
    });
}

/// Translate a message by ID.
///
/// Returns the message ID if translation is not found.
pub fn t(id: &str) -> String {
    let bundle = BUNDLE.get_or_init(|| {
        let locale = detect_locale();
        load_bundle(&locale)
    });

    bundle
        .get_message(id)
        .and_then(|msg| msg.value())
        .map(|pattern| {
            let mut errors = vec![];
            bundle.format_pattern(pattern, None, &mut errors).to_string()
        })
        .unwrap_or_else(|| id.to_string())
}

/// Translate a message with arguments.
///
/// Returns the message ID if translation is not found.
pub fn t_args(id: &str, args: &[(&str, &str)]) -> String {
    let bundle = BUNDLE.get_or_init(|| {
        let locale = detect_locale();
        load_bundle(&locale)
    });

    bundle
        .get_message(id)
        .and_then(|msg| msg.value())
        .map(|pattern| {
            let mut fluent_args = FluentArgs::new();
            for (key, value) in args {
                fluent_args.set(*key, FluentValue::from(*value));
            }
            let mut errors = vec![];
            bundle
                .format_pattern(pattern, Some(&fluent_args), &mut errors)
                .to_string()
        })
        .unwrap_or_else(|| id.to_string())
}

/// Detect the system locale.
fn detect_locale() -> LanguageIdentifier {
    sys_locale::get_locale()
        .and_then(|locale_str| locale_str.parse().ok())
        .unwrap_or_else(|| "en-US".parse().unwrap())
}

/// Load a translation bundle for the given locale.
fn load_bundle(locale: &LanguageIdentifier) -> ConcurrentFluentBundle<FluentResource> {
    let mut bundle = ConcurrentFluentBundle::new_concurrent(vec![locale.clone()]);

    // Try to load the requested locale, fall back to en-US
    let ftl_content = load_ftl_for_locale(locale)
        .or_else(|| load_ftl_for_locale(&"en-US".parse().unwrap()))
        .unwrap_or_default();

    if let Ok(resource) = FluentResource::try_new(ftl_content) {
        let _ = bundle.add_resource(resource);
    }

    bundle
}

/// Load FTL content for a specific locale.
fn load_ftl_for_locale(locale: &LanguageIdentifier) -> Option<String> {
    let locale_str = locale.to_string();

    // Try exact match first (e.g., "cs-CZ")
    if let Some(content) = get_embedded_ftl(&locale_str) {
        return Some(content.to_string());
    }

    // Try language-only match (e.g., "cs" from "cs-CZ")
    let lang = locale.language.as_str();

    // Map language codes to full locale codes we support
    match lang {
        "cs" => get_embedded_ftl("cs-CZ").map(|s| s.to_string()),
        "en" => get_embedded_ftl("en-US").map(|s| s.to_string()),
        _ => None,
    }
}

/// Get embedded FTL content for a locale.
fn get_embedded_ftl(locale: &str) -> Option<&'static str> {
    match locale {
        "en-US" => Some(include_str!("../../locales/en-US/main.ftl")),
        "cs-CZ" => Some(include_str!("../../locales/cs-CZ/main.ftl")),
        _ => None,
    }
}
