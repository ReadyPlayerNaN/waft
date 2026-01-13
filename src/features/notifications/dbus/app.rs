use std::sync::Arc;

use super::super::types::AppIdent;
use super::hints::Hints;

pub fn resolve_app_ident(app_name: &str, hints: &Hints) -> Option<AppIdent> {
    if !app_name.is_empty() {
        return Some(AppIdent {
            ident: Arc::from(app_name),
            title: Some(Arc::from(app_name)),
        });
    } else if let Some(desktop_entry) = &hints.desktop_entry {
        return Some(AppIdent {
            ident: desktop_entry.clone(),
            title: Some(desktop_entry.clone()),
        });
    }
    None
}
