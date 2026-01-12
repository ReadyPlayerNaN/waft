use std::sync::Arc;

use super::super::types::{AppIdent, NotificationIcon};
use super::hints::Hints;

pub fn resolve_app_ident(app_name: &str, hints: &Hints) -> Option<AppIdent> {
    if !app_name.is_empty() {
        return Some(AppIdent {
            ident: Arc::from(app_name),
            title: Some(Arc::from(app_name)),
            icon: None,
        });
    } else if let Some(desktop_entry) = &hints.desktop_entry {
        // Check if we have image data or image path from hints
        let icon = if let Some(image_data) = &hints.image_data {
            Some(NotificationIcon::Bytes(image_data.clone()))
        } else if let Some(image_path) = &hints.image_path {
            Some(NotificationIcon::FilePath(Arc::new(
                std::path::PathBuf::from(image_path.as_ref()),
            )))
        } else {
            // Try to use desktop entry as a themed icon name
            Some(NotificationIcon::Themed(desktop_entry.clone()))
        };

        return Some(AppIdent {
            ident: desktop_entry.clone(),
            title: Some(desktop_entry.clone()),
            icon,
        });
    }
    None
}
