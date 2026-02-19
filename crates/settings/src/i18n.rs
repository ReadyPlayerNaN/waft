//! Internationalization support for waft-settings.
//!
//! Uses the `waft-i18n` crate with a `OnceLock<I18n>` singleton.
//! All widgets access translations via `crate::i18n::t("key")`.

use std::sync::OnceLock;
use waft_i18n::I18n;

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/settings.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/settings.ftl")),
        ])
    })
}

pub fn t(id: &str) -> String {
    i18n().t(id)
}

pub fn t_args(id: &str, args: &[(&str, &str)]) -> String {
    i18n().t_args(id, args)
}
