//! Internationalization support using Fluent.
//!
//! Provides locale detection and translation lookup. Each plugin creates its
//! own [`I18n`] instance with its embedded FTL content.
//!
//! # Example
//!
//! ```rust
//! use std::sync::OnceLock;
//! use waft_i18n::I18n;
//!
//! static I18N: OnceLock<I18n> = OnceLock::new();
//!
//! fn i18n() -> &'static I18n {
//!     I18N.get_or_init(|| I18n::new(&[
//!         ("en-US", "hello = Hello"),
//!         ("cs-CZ", "hello = Ahoj"),
//!     ]))
//! }
//!
//! // i18n().t("hello") returns "Hello" or "Ahoj" based on system locale
//! ```

/// Returns the detected system locale as a BCP47 string (e.g. "cs-CZ").
/// Falls back to "en-US" if detection fails.
pub fn system_locale() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string())
}

use fluent_bundle::concurrent::FluentBundle as ConcurrentFluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

/// A translation bundle initialized from per-locale FTL content.
pub struct I18n {
    bundle: ConcurrentFluentBundle<FluentResource>,
}

impl I18n {
    /// Create a new translation bundle.
    ///
    /// `translations` is a slice of `(locale, ftl_content)` pairs.
    /// The system locale is detected automatically; the best matching
    /// translation is loaded, falling back to `"en-US"`.
    pub fn new(translations: &[(&str, &str)]) -> Self {
        let locale = detect_locale();
        let bundle = load_bundle(&locale, translations);
        Self { bundle }
    }

    /// Translate a message by ID.
    ///
    /// Returns the message ID if translation is not found.
    pub fn t(&self, id: &str) -> String {
        self.bundle
            .get_message(id)
            .and_then(|msg| msg.value())
            .map(|pattern| {
                let mut errors = vec![];
                self.bundle
                    .format_pattern(pattern, None, &mut errors)
                    .to_string()
            })
            .unwrap_or_else(|| id.to_string())
    }

    /// Translate a message with arguments.
    ///
    /// Returns the message ID if translation is not found.
    pub fn t_args(&self, id: &str, args: &[(&str, &str)]) -> String {
        self.bundle
            .get_message(id)
            .and_then(|msg| msg.value())
            .map(|pattern| {
                let mut fluent_args = FluentArgs::new();
                for (key, value) in args {
                    fluent_args.set(*key, FluentValue::from(*value));
                }
                let mut errors = vec![];
                self.bundle
                    .format_pattern(pattern, Some(&fluent_args), &mut errors)
                    .to_string()
            })
            .unwrap_or_else(|| id.to_string())
    }
}

/// Detect the system locale.
fn detect_locale() -> LanguageIdentifier {
    sys_locale::get_locale()
        .and_then(|locale_str| locale_str.parse().ok())
        .unwrap_or_else(|| "en-US".parse().expect("en-US is a valid language identifier"))
}

/// Load a translation bundle, picking the best locale match.
fn load_bundle(
    locale: &LanguageIdentifier,
    translations: &[(&str, &str)],
) -> ConcurrentFluentBundle<FluentResource> {
    let mut bundle = ConcurrentFluentBundle::new_concurrent(vec![locale.clone()]);

    let ftl_content = find_ftl(locale, translations)
        .or_else(|| find_ftl(&"en-US".parse().expect("en-US is a valid language identifier"), translations))
        .unwrap_or_default();

    if let Ok(resource) = FluentResource::try_new(ftl_content) {
        let _ = bundle.add_resource(resource);
    }

    bundle
}

/// Find FTL content matching a locale (exact match, then language-only).
fn find_ftl(locale: &LanguageIdentifier, translations: &[(&str, &str)]) -> Option<String> {
    let locale_str = locale.to_string();

    // Exact match (e.g. "cs-CZ")
    for (loc, content) in translations {
        if *loc == locale_str {
            return Some(content.to_string());
        }
    }

    // Language-only match (e.g. "cs" matches "cs-CZ")
    let lang = locale.language.as_str();
    for (loc, content) in translations {
        if let Ok(candidate) = loc.parse::<LanguageIdentifier>()
            && candidate.language.as_str() == lang
        {
            return Some(content.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_t_returns_translation() {
        let i18n = I18n::new(&[("en-US", "greeting = Hello")]);
        assert_eq!(i18n.t("greeting"), "Hello");
    }

    #[test]
    fn test_t_returns_id_for_missing_key() {
        let i18n = I18n::new(&[("en-US", "greeting = Hello")]);
        assert_eq!(i18n.t("missing-key"), "missing-key");
    }

    #[test]
    fn test_t_args() {
        let i18n = I18n::new(&[("en-US", "hello = Hello { $name }")]);
        // Fluent wraps interpolated values in Unicode isolation marks (U+2068, U+2069)
        assert_eq!(
            i18n.t_args("hello", &[("name", "World")]),
            "Hello \u{2068}World\u{2069}"
        );
    }

    #[test]
    fn test_empty_translations_returns_id() {
        let i18n = I18n::new(&[]);
        assert_eq!(i18n.t("anything"), "anything");
    }
}
