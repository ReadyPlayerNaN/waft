use std::sync::Arc;

use super::super::types::NotificationIcon;
use super::hints::Hints;

fn normalize_icon_name(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_whitespace() {
            out.push('-');
        } else if ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch.to_ascii_lowercase());
        } else {
            // drop punctuation and other symbols
        }
    }
    if out.is_empty() {
        input.to_ascii_lowercase()
    } else {
        out
    }
}

pub async fn resolve_notification_icon(
    app_icon: &str,
    app_name: &str,
    desktop_entry: Option<Arc<str>>,
    hints: &Hints,
) -> NotificationIcon {
    println!("resolve_notification_icon {:?}", app_icon);
    if let Some(bytes) = &hints.image_data {
        if !bytes.is_empty() {
            let i = NotificationIcon::Bytes(bytes.clone());
            if i.is_available().await {
                return i;
            }
        }
    }

    if let Some(path) = &hints.image_path {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            let i = NotificationIcon::FilePath(Arc::new(trimmed.into()));
            if i.is_available().await {
                return i;
            }
        }
    }

    let icon = app_icon.trim();
    if !icon.is_empty() {
        // Heuristic: treat as path if it contains a path separator or starts like a path.
        if icon.contains('/') || icon.starts_with('.') || icon.starts_with('~') {
            let i = NotificationIcon::FilePath(Arc::new(icon.into()));
            if i.is_available().await {
                return i;
            }
        } else {
            let i = NotificationIcon::Themed(icon.into());
            if i.is_available().await {
                return i;
            }
        }
    }

    let mut candidates: Vec<NotificationIcon> = Vec::new();

    if let Some(de) = desktop_entry {
        let trimmed = de.trim();
        if !trimmed.is_empty() {
            // Typical desktop-entry: "org.gnome.Nautilus.desktop" -> "org.gnome.Nautilus".
            let without_suffix = trimmed.strip_suffix(".desktop").unwrap_or(trimmed);
            candidates.push(NotificationIcon::Themed(without_suffix.into()));
            candidates.push(NotificationIcon::Themed(
                normalize_icon_name(without_suffix).into(),
            ));
        }
    }

    if !app_name.trim().is_empty() {
        candidates.push(NotificationIcon::Themed(
            normalize_icon_name(app_name).into(),
        ));
    }

    for cand in candidates {
        if cand.is_available().await {
            println!("Using candidate {:?} ", cand);
            return cand;
        }
    }

    NotificationIcon::Themed("dialog-information-symbolic".into())
}
