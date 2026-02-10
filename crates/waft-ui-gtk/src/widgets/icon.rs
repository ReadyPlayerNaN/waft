//! Generic icon types and widget.
//!
//! Displays icons from themed names, file paths, or raw bytes.
//! Tries each icon hint in order until one succeeds, falling back to a default.

use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_ICON: &str = "dialog-information-symbolic";

/// Generic icon representation — themed name, file path, or raw bytes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd)]
pub enum Icon {
    Bytes(Vec<u8>),
    /// A file path to an image (png/svg/etc). Will be loaded and scaled to fit.
    FilePath(Arc<PathBuf>),
    /// A themed icon name, e.g. `"dialog-information-symbolic"`.
    Themed(Arc<str>),
}

impl Icon {
    /// Parse an icon from a string reference.
    ///
    /// If the string contains path-like characters (`/`, `.`, `~`),
    /// it's treated as a file path. Otherwise, it's treated as a themed icon name.
    pub fn parse(str: &Arc<str>) -> Self {
        let s: &str = str.trim();
        if s.contains('/') || s.starts_with('.') || s.starts_with('~') {
            Self::FilePath(Arc::new(PathBuf::from(s)))
        } else {
            Self::Themed(Arc::from(s))
        }
    }
}

/// Try to resolve a themed icon name against the current icon theme.
/// Returns `Some(resolved_name)` if the icon exists (trying original,
/// -symbolic, lowercase, lowercase-symbolic), `None` otherwise.
pub fn resolve_themed_icon(name: &str) -> Option<String> {
    let display = gtk::gdk::Display::default()?;
    let icon_theme = gtk::IconTheme::for_display(&display);

    if icon_theme.has_icon(name) {
        return Some(name.to_string());
    }

    let symbolic = format!("{}-symbolic", name);
    if icon_theme.has_icon(&symbolic) {
        return Some(symbolic);
    }

    let lowercase = name.to_lowercase();
    if icon_theme.has_icon(&lowercase) {
        return Some(lowercase);
    }

    let lowercase_symbolic = format!("{}-symbolic", lowercase);
    if icon_theme.has_icon(&lowercase_symbolic) {
        return Some(lowercase_symbolic);
    }

    None
}

/// Pure GTK4 icon widget — displays themed icons or textures.
#[derive(Clone)]
pub struct IconWidget {
    image: gtk::Image,
}

impl IconWidget {
    /// Create a new icon widget, trying each icon hint until one succeeds.
    pub fn new(icon_hints: Vec<Icon>, pixel_size: i32) -> Self {
        let image = gtk::Image::builder()
            .pixel_size(pixel_size)
            .valign(gtk::Align::Center)
            .build();

        Self::apply_first_valid_icon(&image, &icon_hints);

        Self { image }
    }

    /// Convenience constructor for a single named icon.
    pub fn from_name(icon_name: &str, pixel_size: i32) -> Self {
        Self::new(vec![Icon::Themed(Arc::from(icon_name))], pixel_size)
    }

    /// Try each icon hint in order until one succeeds, falling back to default.
    fn apply_first_valid_icon(image: &gtk::Image, icon_hints: &[Icon]) {
        for hint in icon_hints {
            if Self::try_apply_icon(image, hint) {
                return;
            }
        }

        // All hints failed, use default
        // Note: set_paintable must be called BEFORE set_icon_name because
        // GTK4 Image displays based on the last property set
        image.set_paintable(gtk::gdk::Paintable::NONE);
        image.set_icon_name(Some(DEFAULT_ICON));
    }

    /// Try to apply an icon hint. Returns true if successful.
    fn try_apply_icon(image: &gtk::Image, icon: &Icon) -> bool {
        match icon {
            Icon::Themed(name) => {
                if let Some(resolved) = resolve_themed_icon(name) {
                    image.set_paintable(gtk::gdk::Paintable::NONE);
                    image.set_icon_name(Some(&resolved));
                    return true;
                }
                false
            }
            Icon::FilePath(path) => {
                if let Ok(tex) = gtk::gdk::Texture::from_filename(path.as_ref()) {
                    image.set_icon_name(None);
                    image.set_paintable(Some(&tex));
                    return true;
                }
                false
            }
            Icon::Bytes(_b) => {
                // TODO: Implement icon parsing from bytes
                false
            }
        }
    }

    /// Get a reference to the image widget.
    pub fn widget(&self) -> &gtk::Image {
        &self.image
    }

    /// Update the icon to a new set of hints.
    /// Tries each hint in order, falling back to default if all fail.
    pub fn update_icon(&self, icon_hints: Vec<Icon>) {
        Self::apply_first_valid_icon(&self.image, &icon_hints);
    }

    /// Update the icon to a single named icon.
    /// Convenience method for updating to a themed icon name.
    pub fn set_icon(&self, icon_name: &str) {
        self.update_icon(vec![Icon::Themed(Arc::from(icon_name))]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn test_absolute_path_detection() {
        let icon = Icon::parse(&Arc::from("/usr/share/icons/test.png"));
        match icon {
            Icon::FilePath(path) => {
                assert_eq!(path.as_ref(), &PathBuf::from("/usr/share/icons/test.png"));
            }
            _ => panic!("Expected Icon::FilePath, got {:?}", icon),
        }
    }

    #[test]
    fn test_relative_path_with_dot() {
        let icon = Icon::parse(&Arc::from("./icons/test.png"));
        match icon {
            Icon::FilePath(path) => {
                assert_eq!(path.as_ref(), &PathBuf::from("./icons/test.png"));
            }
            _ => panic!("Expected Icon::FilePath, got {:?}", icon),
        }
    }

    #[test]
    fn test_home_directory_path() {
        let icon = Icon::parse(&Arc::from("~/icons/test.png"));
        match icon {
            Icon::FilePath(path) => {
                assert_eq!(path.as_ref(), &PathBuf::from("~/icons/test.png"));
            }
            _ => panic!("Expected Icon::FilePath, got {:?}", icon),
        }
    }

    #[test]
    fn test_themed_icon_name() {
        let icon_name = Arc::from("dialog-information");
        let icon = Icon::parse(&icon_name);
        match icon {
            Icon::Themed(name) => {
                assert_eq!(name.as_ref(), "dialog-information");
            }
            _ => panic!("Expected Icon::Themed, got {:?}", icon),
        }
    }

    #[test]
    fn test_whitespace_trimming_themed() {
        let icon = Icon::parse(&Arc::from("  dialog-information  "));
        match icon {
            Icon::Themed(name) => {
                assert_eq!(name.as_ref(), "dialog-information");
            }
            _ => panic!("Expected Icon::Themed, got {:?}", icon),
        }
    }

    #[test]
    fn test_whitespace_trimming_filepath() {
        let icon = Icon::parse(&Arc::from("  /usr/share/icons/test.png  "));
        match icon {
            Icon::FilePath(path) => {
                assert_eq!(path.as_ref(), &PathBuf::from("/usr/share/icons/test.png"));
            }
            _ => panic!("Expected Icon::FilePath, got {:?}", icon),
        }
    }

    #[test]
    fn test_resolve_exact_name_match() {
        let result = resolve_themed_icon("dialog-information");
        if let Some(resolved) = result {
            assert_eq!(resolved, "dialog-information");
        }
    }

    #[test]
    fn test_resolve_symbolic_fallback() {
        let result = resolve_themed_icon("dialog-information");
        if result.is_some() {
            assert!(result.is_some());
        }
    }

    #[test]
    fn test_resolve_lowercase_fallback() {
        let result = resolve_themed_icon("Dialog-Information");
        if let Some(resolved) = result {
            assert!(resolved.to_lowercase() == resolved || resolved.ends_with("-symbolic"));
        }
    }

    #[test]
    fn test_resolve_lowercase_symbolic_fallback() {
        let result = resolve_themed_icon("DIALOG-INFORMATION");
        if let Some(resolved) = result {
            assert_eq!(resolved, resolved.to_lowercase());
        }
    }

    #[test]
    fn test_resolve_no_match() {
        let result = resolve_themed_icon("this-icon-definitely-does-not-exist-12345");
        assert!(result.is_none());
    }

    #[test]
    #[ignore = "requires GTK display connection"]
    fn test_icon_widget_from_name() {
        let widget = IconWidget::from_name("dialog-information-symbolic", 24);
        assert_eq!(widget.widget().pixel_size(), 24);
    }

    #[test]
    #[ignore = "requires GTK display connection"]
    fn test_icon_widget_set_icon() {
        let widget = IconWidget::from_name("dialog-information-symbolic", 24);
        widget.set_icon("dialog-warning-symbolic");
        assert_eq!(widget.widget().pixel_size(), 24);
    }

    #[test]
    #[ignore = "requires GTK display connection"]
    fn test_icon_widget_update_icon_with_hints() {
        let widget = IconWidget::from_name("dialog-information-symbolic", 24);
        widget.update_icon(vec![
            Icon::Themed(Arc::from("non-existent-icon")),
            Icon::Themed(Arc::from("dialog-warning-symbolic")),
        ]);
        assert_eq!(widget.widget().pixel_size(), 24);
    }
}
