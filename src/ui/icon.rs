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
    pub fn from_str(str: &Arc<str>) -> Self {
        let s: &str = str.trim();
        if s.contains('/') || s.starts_with('.') || s.starts_with('~') {
            Self::FilePath(Arc::new(PathBuf::from(s)))
        } else {
            Self::Themed(Arc::clone(str))
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
pub struct IconWidget {
    image: gtk::Image,
}

impl IconWidget {
    /// Create a new icon widget, trying each icon hint until one succeeds.
    pub fn new(icon_hints: Vec<Icon>) -> Self {
        let image = gtk::Image::builder()
            .pixel_size(32)
            .valign(gtk::Align::Start)
            .build();

        Self::apply_first_valid_icon(&image, &icon_hints);

        Self { image }
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
}
