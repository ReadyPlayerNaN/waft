//! Wallpaper gallery section -- smart container.
//!
//! Reads wallpaper files from the filesystem, displays thumbnails in FlowBox grids
//! organized by mode-specific sub-galleries. Subscribes to `wallpaper-manager` entity
//! for current wallpaper and mode changes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity::display::{DaySegment, WallpaperMode};

use super::thumbnail_widget::ThumbnailWidget;
use crate::i18n::t;

/// Image file extensions to include in gallery.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];

/// A single gallery group with a header and FlowBox of thumbnails.
struct GalleryGroup {
    group: adw::PreferencesGroup,
    thumbnails: Vec<ThumbnailWidget>,
}

/// Smart container for the wallpaper gallery.
pub struct GallerySection {
    pub root: gtk::Box,
    groups: Rc<RefCell<HashMap<String, GalleryGroup>>>,
    wallpaper_dir: Rc<RefCell<String>>,
    current_mode: Rc<RefCell<WallpaperMode>>,
    current_wallpaper: Rc<RefCell<Option<String>>>,
    action_callback: EntityActionCallback,
    current_urn: Rc<RefCell<Option<Urn>>>,
}

impl GallerySection {
    pub fn new(action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();

        Self {
            root,
            groups: Rc::new(RefCell::new(HashMap::new())),
            wallpaper_dir: Rc::new(RefCell::new(String::new())),
            current_mode: Rc::new(RefCell::new(WallpaperMode::Static)),
            current_wallpaper: Rc::new(RefCell::new(None)),
            action_callback: action_callback.clone(),
            current_urn: Rc::new(RefCell::new(None)),
        }
    }

    /// Update gallery with new wallpaper entity state.
    pub fn apply_props(
        &self,
        wallpaper_dir: &str,
        mode: &WallpaperMode,
        current_wallpaper: Option<&str>,
        urn: &Urn,
    ) {
        let dir_changed = {
            let old_dir = self.wallpaper_dir.borrow();
            *old_dir != wallpaper_dir
        };
        let mode_changed = {
            let old_mode = self.current_mode.borrow();
            *old_mode != *mode
        };

        *self.wallpaper_dir.borrow_mut() = wallpaper_dir.to_string();
        *self.current_mode.borrow_mut() = *mode;
        *self.current_wallpaper.borrow_mut() = current_wallpaper.map(|s| s.to_string());
        *self.current_urn.borrow_mut() = Some(urn.clone());

        if dir_changed || mode_changed {
            self.rebuild_groups();
        } else {
            self.update_selection();
        }
    }

    /// Rebuild all gallery groups for the current mode.
    fn rebuild_groups(&self) {
        // Remove existing groups from root
        {
            let groups = self.groups.borrow();
            for group in groups.values() {
                self.root.remove(&group.group);
            }
        }
        self.groups.borrow_mut().clear();

        let mode = *self.current_mode.borrow();
        let wallpaper_dir = self.wallpaper_dir.borrow().clone();
        let expanded_dir = expand_tilde(&wallpaper_dir);

        let folders = folders_for_mode(&mode);

        for (label_key, folder_name) in &folders {
            let folder_path = expanded_dir.join(folder_name);
            let gallery_group = self.create_gallery_group(label_key, folder_name, &folder_path);
            self.root.append(&gallery_group.group);
            self.groups.borrow_mut().insert(folder_name.to_string(), gallery_group);
        }

        self.update_selection();
    }

    /// Create a single gallery group with thumbnails from the given folder.
    fn create_gallery_group(
        &self,
        label_key: &str,
        folder_name: &str,
        folder_path: &Path,
    ) -> GalleryGroup {
        create_gallery_group_impl(
            label_key,
            folder_name,
            folder_path,
            &self.action_callback,
            &self.current_urn,
            &self.current_wallpaper,
            &self.groups,
            &self.wallpaper_dir,
            &self.current_mode,
            &self.root,
        )
    }

    /// Update which thumbnail is highlighted as selected.
    fn update_selection(&self) {
        let current = self.current_wallpaper.borrow();
        let groups = self.groups.borrow();

        for group in groups.values() {
            for thumb in &group.thumbnails {
                let selected = current
                    .as_deref()
                    .map(|c| paths_match(c, &thumb.path))
                    .unwrap_or(false);
                thumb.set_selected(selected);
            }
        }
    }
}

/// Return the gallery folders for a given wallpaper mode.
fn folders_for_mode(mode: &WallpaperMode) -> Vec<(&'static str, String)> {
    match mode {
        WallpaperMode::Static => vec![
            ("wallpaper-gallery-static", "static-mode".to_string()),
        ],
        WallpaperMode::StyleTracking => vec![
            ("wallpaper-gallery-dark", "dark".to_string()),
            ("wallpaper-gallery-light", "light".to_string()),
        ],
        WallpaperMode::DayTracking => {
            DaySegment::all()
                .iter()
                .map(|seg| {
                    let label_key = match seg {
                        DaySegment::EarlyMorning => "wallpaper-segment-early-morning",
                        DaySegment::Morning => "wallpaper-segment-morning",
                        DaySegment::Afternoon => "wallpaper-segment-afternoon",
                        DaySegment::Evening => "wallpaper-segment-evening",
                        DaySegment::Night => "wallpaper-segment-night",
                        DaySegment::MidnightOil => "wallpaper-segment-midnight-oil",
                    };
                    (label_key, seg.folder_name().to_string())
                })
                .collect()
        }
    }
}

/// Load thumbnail widgets from a directory.
fn load_thumbnails(folder: &Path) -> Vec<ThumbnailWidget> {
    let entries = match std::fs::read_dir(folder) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("[wallpaper/gallery] failed to read directory {}: {e}", folder.display());
            return Vec::new();
        }
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .collect();

    paths.sort();

    paths
        .iter()
        .map(|path| {
            let filename = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            ThumbnailWidget::new(&path.to_string_lossy(), &filename)
        })
        .collect()
}

/// Canonicalize a path, returning `None` if the path doesn't exist.
fn normalize_path(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// Compare two paths for equality. Tries fast string comparison first,
/// then falls back to canonicalization for symlink/tilde differences.
fn paths_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (normalize_path(a), normalize_path(b)) {
        (Some(na), Some(nb)) => na == nb,
        _ => false,
    }
}

/// Expand `~` prefix to home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    PathBuf::from(path)
}

/// Unified gallery group constructor used by both initial build and add-button refresh.
#[allow(clippy::too_many_arguments)]
fn create_gallery_group_impl(
    label_key: &str,
    folder_name: &str,
    folder_path: &Path,
    action_callback: &EntityActionCallback,
    current_urn: &Rc<RefCell<Option<Urn>>>,
    current_wallpaper: &Rc<RefCell<Option<String>>>,
    groups: &Rc<RefCell<HashMap<String, GalleryGroup>>>,
    wallpaper_dir: &Rc<RefCell<String>>,
    current_mode: &Rc<RefCell<WallpaperMode>>,
    root: &gtk::Box,
) -> GalleryGroup {
    let group = adw::PreferencesGroup::builder()
        .title(t(label_key))
        .build();

    // Add button in header
    let add_button = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .tooltip_text(t("wallpaper-gallery-add"))
        .build();
    group.set_header_suffix(Some(&add_button));

    // Wire add button
    {
        let folder = folder_path.to_path_buf();
        let groups_ref = groups.clone();
        let wallpaper_dir_ref = wallpaper_dir.clone();
        let current_mode_ref = current_mode.clone();
        let current_wallpaper_ref = current_wallpaper.clone();
        let action_callback_ref = action_callback.clone();
        let current_urn_ref = current_urn.clone();
        let root_ref = root.clone();
        let folder_name_owned = folder_name.to_string();

        add_button.connect_clicked(move |button| {
            let dialog = gtk::FileDialog::builder()
                .title(t("wallpaper-browse-title"))
                .modal(true)
                .build();

            let filter = gtk::FileFilter::new();
            filter.set_name(Some(&t("wallpaper-image-files")));
            filter.add_mime_type("image/png");
            filter.add_mime_type("image/jpeg");
            filter.add_mime_type("image/webp");
            filter.add_mime_type("image/gif");
            filter.add_mime_type("image/bmp");

            let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            dialog.set_filters(Some(&filters));
            dialog.set_default_filter(Some(&filter));

            let window = button.root().and_then(|r| r.downcast::<gtk::Window>().ok());

            let folder_inner = folder.clone();
            let groups_inner = groups_ref.clone();
            let wallpaper_dir_inner = wallpaper_dir_ref.clone();
            let current_mode_inner = current_mode_ref.clone();
            let current_wallpaper_inner = current_wallpaper_ref.clone();
            let action_callback_inner = action_callback_ref.clone();
            let current_urn_inner = current_urn_ref.clone();
            let root_inner = root_ref.clone();
            let folder_name_inner = folder_name_owned.clone();

            dialog.open(window.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
                let file = match result {
                    Ok(f) => f,
                    Err(e) => {
                        if !e.matches(gtk::gio::IOErrorEnum::Cancelled) {
                            log::warn!("[wallpaper/gallery] file dialog error: {e}");
                        }
                        return;
                    }
                };

                let Some(source_path) = file.path() else {
                    log::warn!("[wallpaper/gallery] selected file has no path");
                    return;
                };

                let dest_dir = folder_inner.clone();

                // Create destination directory
                if let Err(e) = std::fs::create_dir_all(&dest_dir) {
                    log::warn!("[wallpaper/gallery] failed to create directory: {e}");
                    return;
                }

                let filename = source_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let dest_path = dest_dir.join(&filename);

                // Copy file (synchronous -- typical wallpaper images are small enough)
                if let Err(e) = std::fs::copy(&source_path, &dest_path) {
                    log::warn!("[wallpaper/gallery] failed to copy file: {e}");
                    return;
                }

                log::debug!(
                    "[wallpaper/gallery] copied {} to {}",
                    source_path.display(),
                    dest_path.display()
                );

                // Refresh the gallery group
                let dir = wallpaper_dir_inner.borrow().clone();
                let expanded = expand_tilde(&dir);
                let folder_path = expanded.join(&folder_name_inner);

                let mode = *current_mode_inner.borrow();
                let folders = folders_for_mode(&mode);

                let label_key = folders
                    .iter()
                    .find(|(_, name)| *name == folder_name_inner)
                    .map(|(key, _)| key.to_string())
                    .unwrap_or_else(|| folder_name_inner.clone());

                if let Some(old) = groups_inner.borrow_mut().remove(&folder_name_inner) {
                    root_inner.remove(&old.group);
                }

                let new_group = create_gallery_group_impl(
                    &label_key,
                    &folder_name_inner,
                    &folder_path,
                    &action_callback_inner,
                    &current_urn_inner,
                    &current_wallpaper_inner,
                    &groups_inner,
                    &wallpaper_dir_inner,
                    &current_mode_inner,
                    &root_inner,
                );

                let insert_idx = folders
                    .iter()
                    .position(|(_, name)| *name == folder_name_inner)
                    .unwrap_or(0);

                if insert_idx == 0 {
                    root_inner.prepend(&new_group.group);
                } else {
                    let groups_borrow = groups_inner.borrow();
                    let prev_folder = &folders[insert_idx - 1].1;
                    if let Some(prev_group) = groups_borrow.get(prev_folder) {
                        root_inner.insert_child_after(
                            &new_group.group,
                            Some(&prev_group.group),
                        );
                    } else {
                        root_inner.append(&new_group.group);
                    }
                }

                groups_inner
                    .borrow_mut()
                    .insert(folder_name_inner, new_group);
            });
        });
    }

    // FlowBox for thumbnails
    let flow_box = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .homogeneous(true)
        .max_children_per_line(6)
        .min_children_per_line(3)
        .row_spacing(4)
        .column_spacing(4)
        .build();

    group.add(&flow_box);

    let thumbnails = load_thumbnails(folder_path);

    for thumb in &thumbnails {
        let flow_child = gtk::FlowBoxChild::new();
        flow_child.set_child(Some(&thumb.root));
        flow_box.append(&flow_child);
    }

    // Wire selection
    {
        let cb = action_callback.clone();
        let urn_ref = current_urn.clone();
        let thumbs: Vec<String> = thumbnails.iter().map(|t| t.path.clone()).collect();

        flow_box.connect_child_activated(move |_, child| {
            let idx = child.index() as usize;
            if let Some(path) = thumbs.get(idx)
                && let Some(ref urn) = *urn_ref.borrow() {
                    cb(
                        urn.clone(),
                        "set-wallpaper".to_string(),
                        serde_json::json!({ "path": path }),
                    );
                }
        });
    }

    // Update selection state
    {
        let current = current_wallpaper.borrow();
        for thumb in &thumbnails {
            let selected = current
                .as_deref()
                .map(|c| paths_match(c, &thumb.path))
                .unwrap_or(false);
            thumb.set_selected(selected);
        }
    }

    if thumbnails.is_empty() {
        let empty_label = gtk::Label::builder()
            .label(t("wallpaper-gallery-empty"))
            .css_classes(["dim-label"])
            .margin_top(12)
            .margin_bottom(12)
            .build();
        group.add(&empty_label);
    }

    GalleryGroup {
        group,
        thumbnails,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_matches() {
        assert!(paths_match(
            "/home/user/wallpaper.png",
            "/home/user/wallpaper.png"
        ));
    }

    #[test]
    fn different_paths_do_not_match() {
        assert!(!paths_match("/home/user/a.png", "/home/user/b.png"));
    }

    #[test]
    fn nonexistent_same_string_matches() {
        assert!(paths_match(
            "/nonexistent/path.png",
            "/nonexistent/path.png"
        ));
    }

    #[test]
    fn nonexistent_different_strings_do_not_match() {
        assert!(!paths_match("/nonexistent/a.png", "/nonexistent/b.png"));
    }

    #[test]
    fn symlinked_paths_match() {
        let dir = std::env::temp_dir().join("waft-test-paths-match");
        let _ = std::fs::create_dir_all(&dir);
        let real_file = dir.join("real.png");
        let symlink = dir.join("link.png");

        std::fs::write(&real_file, b"test").expect("write real file");
        // Remove stale symlink from previous test run
        let _ = std::fs::remove_file(&symlink);
        std::os::unix::fs::symlink(&real_file, &symlink).expect("create symlink");

        let real_str = real_file.to_string_lossy().to_string();
        let link_str = symlink.to_string_lossy().to_string();

        assert!(paths_match(&real_str, &link_str));

        // Cleanup
        let _ = std::fs::remove_file(&symlink);
        let _ = std::fs::remove_file(&real_file);
        let _ = std::fs::remove_dir(&dir);
    }
}
