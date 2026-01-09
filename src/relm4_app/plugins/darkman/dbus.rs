use anyhow::Result;
use std::sync::Arc;

use crate::dbus::DbusHandle;

use super::values::DarkmanMode;

pub const DARKMAN_DESTINATION: &str = "nl.whynothugo.darkman";
pub const DARKMAN_PATH: &str = "/nl/whynothugo/darkman";

pub async fn get_state(conn: &DbusHandle) -> Result<DarkmanMode> {
    let value = conn
        .get_property(DARKMAN_DESTINATION, DARKMAN_PATH, "Mode")
        .await?;

    Ok(value
        .as_deref()
        .and_then(DarkmanMode::from_str)
        .unwrap_or(DarkmanMode::Light))
}

pub async fn set_state(conn: Arc<DbusHandle>, mode: DarkmanMode) -> Result<()> {
    conn.set_property(DARKMAN_DESTINATION, DARKMAN_PATH, "Mode", mode.as_str())
        .await
}
