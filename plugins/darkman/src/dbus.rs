use anyhow::Result;
use std::sync::Arc;

use waft_core::dbus::DbusHandle;

use super::values::DarkmanMode;

pub const DARKMAN_DESTINATION: &str = "nl.whynothugo.darkman";
pub const DARKMAN_PATH: &str = "/nl/whynothugo/darkman";

/// Get darkman mode via nl.whynothugo.darkman.Mode property.
/// Returns Light if property is missing or invalid.
pub async fn get_state(conn: &DbusHandle) -> Result<DarkmanMode> {
    let value = conn
        .get_property(DARKMAN_DESTINATION, DARKMAN_PATH, "Mode")
        .await?;

    Ok(value
        .as_deref()
        .and_then(DarkmanMode::from_str)
        .unwrap_or(DarkmanMode::Light))
}

/// Set darkman mode via nl.whynothugo.darkman.Mode property.
pub async fn set_state(conn: Arc<DbusHandle>, mode: DarkmanMode) -> Result<()> {
    conn.set_property(DARKMAN_DESTINATION, DARKMAN_PATH, "Mode", mode.as_str())
        .await
}
