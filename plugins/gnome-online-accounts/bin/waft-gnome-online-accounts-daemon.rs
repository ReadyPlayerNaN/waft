//! GNOME Online Accounts client with supervised discovery and current-generation actions.
use anyhow::Result;
use std::sync::LazyLock;
use waft_plugin::*;
use waft_plugin_gnome_online_accounts::{actions::GoaPlugin, lifecycle::GoaLifecycle};
use waft_protocol::entity::accounts::{
    ONLINE_ACCOUNT_ENTITY_TYPE, ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
    ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
};

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| {
    waft_i18n::I18n::new(&[
        (
            "en-US",
            include_str!("../locales/en-US/gnome-online-accounts.ftl"),
        ),
        (
            "cs-CZ",
            include_str!("../locales/cs-CZ/gnome-online-accounts.ftl"),
        ),
    ])
});
fn main() -> Result<()> {
    PluginRunner::new(
        "gnome-online-accounts",
        &[
            ONLINE_ACCOUNT_ENTITY_TYPE,
            ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
            ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
        ],
    )
    .i18n(&I18N, "plugin-name", "plugin-description")
    .run(|notifier| async move {
        let lifecycle = GoaLifecycle::new(notifier);
        let supervisor = lifecycle.clone();
        spawn_monitored("goa/lifecycle", async move {
            supervisor.run(None).await;
            Ok(())
        });
        Ok(GoaPlugin { lifecycle })
    })
}
