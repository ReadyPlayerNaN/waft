//! Tokio-only EDS calendar plugin.
use anyhow::Result;
use waft_plugin::PluginRunner;
use waft_plugin_eds::{EdsConfig, EdsPlugin};
use waft_protocol::entity::calendar::{
    CALENDAR_SOURCE_STATUS_ENTITY_TYPE, CALENDAR_SYNC_ENTITY_TYPE, ENTITY_TYPE,
};

static I18N: std::sync::LazyLock<waft_i18n::I18n> = std::sync::LazyLock::new(|| {
    waft_i18n::I18n::new(&[
        ("en-US", include_str!("../locales/en-US/eds.ftl")),
        ("cs-CZ", include_str!("../locales/cs-CZ/eds.ftl")),
    ])
});
fn main() -> Result<()> {
    PluginRunner::new(
        "eds",
        &[
            ENTITY_TYPE,
            CALENDAR_SYNC_ENTITY_TYPE,
            CALENDAR_SOURCE_STATUS_ENTITY_TYPE,
        ],
    )
    .i18n(&I18N, "plugin-name", "plugin-description")
    .run(|notifier| async move {
        let config = match waft_plugin::config::load_plugin_config::<EdsConfig>("eds") {
            Ok(config) => config,
            Err(_) => {
                log::warn!("[eds] unable to load configuration; using defaults");
                EdsConfig::default()
            }
        };
        let plugin = EdsPlugin::new(notifier, config);
        plugin.start();
        Ok(plugin)
    })
}
