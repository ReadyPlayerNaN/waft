use std::sync::OnceLock;
use waft_i18n::I18n;

static I18N: OnceLock<I18n> = OnceLock::new();

pub fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/weather.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/weather.ftl")),
        ])
    })
}
