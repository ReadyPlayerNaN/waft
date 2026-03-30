//! Wallpaper gallery section -- smart container.
//!
//! Reads wallpaper files from the filesystem, displays thumbnails in FlowBox grids
//! organized by mode-specific sub-galleries. Subscribes to `wallpaper-manager` entity
//! for current wallpaper and mode changes.
//!
//! Supports drag-and-drop: external file drops (copy) and inter-gallery moves (rename).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
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
        *self.current_wallpaper.borrow_mut() = current_wallpaper.map(std::string::ToString::to_string);
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

/// Copy a file into a gallery folder, generating a unique name if needed.
///
/// Returns `Some(dest_path)` on success, `None` on failure.
fn copy_file_to_gallery_folder(source: &Path, dest_dir: &Path) -> Option<PathBuf> {
    if let Err(e) = std::fs::create_dir_all(dest_dir) {
        log::warn!("[wallpaper/gallery] failed to create directory: {e}");
        return None;
    }

    let filename = source
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let stem = source
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = source
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();

    // Find a unique destination name
    let mut dest_path = dest_dir.join(&filename);
    let mut counter = 1u32;
    while dest_path.exists() {
        let new_name = if ext.is_empty() {
            format!("{stem}_{counter}")
        } else {
            format!("{stem}_{counter}.{ext}")
        };
        dest_path = dest_dir.join(new_name);
        counter += 1;
    }

    if let Err(e) = std::fs::copy(source, &dest_path) {
        log::warn!("[wallpaper/gallery] failed to copy file: {e}");
        return None;
    }

    log::debug!(
        "[wallpaper/gallery] copied {} to {}",
        source.display(),
        dest_path.display()
    );
    Some(dest_path)
}

/// Remove the old gallery group and recreate it from disk, inserting at the correct position.
#[allow(clippy::too_many_arguments)]
fn refresh_gallery_group(
    folder_name: &str,
    groups: &Rc<RefCell<HashMap<String, GalleryGroup>>>,
    wallpaper_dir: &Rc<RefCell<String>>,
    current_mode: &Rc<RefCell<WallpaperMode>>,
    current_wallpaper: &Rc<RefCell<Option<String>>>,
    action_callback: &EntityActionCallback,
    current_urn: &Rc<RefCell<Option<Urn>>>,
    root: &gtk::Box,
) {
    let dir = wallpaper_dir.borrow().clone();
    let expanded = expand_tilde(&dir);
    let folder_path = expanded.join(folder_name);

    let mode = *current_mode.borrow();
    let folders = folders_for_mode(&mode);

    let label_key = folders
        .iter()
        .find(|(_, name)| *name == folder_name)
        .map(|(key, _)| key.to_string())
        .unwrap_or_else(|| folder_name.to_string());

    if let Some(old) = groups.borrow_mut().remove(folder_name) {
        root.remove(&old.group);
    }

    let new_group = create_gallery_group_impl(
        &label_key,
        folder_name,
        &folder_path,
        action_callback,
        current_urn,
        current_wallpaper,
        groups,
        wallpaper_dir,
        current_mode,
        root,
    );

    let insert_idx = folders
        .iter()
        .position(|(_, name)| *name == folder_name)
        .unwrap_or(0);

    if insert_idx == 0 {
        root.prepend(&new_group.group);
    } else {
        let groups_borrow = groups.borrow();
        let prev_folder = &folders[insert_idx - 1].1;
        if let Some(prev_group) = groups_borrow.get(prev_folder) {
            root.insert_child_after(
                &new_group.group,
                Some(&prev_group.group),
            );
        } else {
            root.append(&new_group.group);
        }
    }

    groups.borrow_mut().insert(folder_name.to_string(), new_group);
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
        .filter_map(std::result::Result::ok)
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

/// Check if a file extension is a supported image format.
fn is_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Determine the source gallery folder name from a file path, given the wallpaper base dir.
fn source_folder_name(file_path: &Path, wallpaper_dir: &Path) -> Option<String> {
    let parent = file_path.parent()?;
    let folder_name = parent.file_name()?;
    // Verify the file is actually inside the wallpaper directory
    if parent.parent()? == wallpaper_dir {
        Some(folder_name.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Compute a unique destination path in a folder, appending _1, _2, etc. if name exists.
fn unique_dest_path(filename: &str, dest_dir: &Path) -> PathBuf {
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut dest_path = dest_dir.join(filename);
    let mut counter = 1u32;
    while dest_path.exists() {
        let new_name = if ext.is_empty() {
            format!("{stem}_{counter}")
        } else {
            format!("{stem}_{counter}.{ext}")
        };
        dest_path = dest_dir.join(new_name);
        counter += 1;
    }
    dest_path
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

    // Wire add button (uses extracted helpers)
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

                if copy_file_to_gallery_folder(&source_path, &folder_inner).is_none() {
                    return;
                }

                refresh_gallery_group(
                    &folder_name_inner,
                    &groups_inner,
                    &wallpaper_dir_inner,
                    &current_mode_inner,
                    &current_wallpaper_inner,
                    &action_callback_inner,
                    &current_urn_inner,
                    &root_inner,
                );
            });
        });
    }

    // FlowBox for thumbnails — minimum height ensures it receives drops when empty
    let flow_box = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .homogeneous(true)
        .max_children_per_line(6)
        .min_children_per_line(3)
        .row_spacing(4)
        .column_spacing(4)
        .height_request(80)
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

    // External file DropTarget (accepts FileList, COPY action)
    {
        let drop_target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);

        let flow_box_for_enter = flow_box.clone();
        drop_target.connect_enter(move |_target, _x, _y| {
            flow_box_for_enter.add_css_class("drop-target-hover");
            gdk::DragAction::COPY
        });

        let flow_box_for_leave = flow_box.clone();
        drop_target.connect_leave(move |_target| {
            flow_box_for_leave.remove_css_class("drop-target-hover");
        });

        let folder_for_drop = folder_path.to_path_buf();
        let groups_for_drop = groups.clone();
        let wallpaper_dir_for_drop = wallpaper_dir.clone();
        let current_mode_for_drop = current_mode.clone();
        let current_wallpaper_for_drop = current_wallpaper.clone();
        let action_callback_for_drop = action_callback.clone();
        let current_urn_for_drop = current_urn.clone();
        let root_for_drop = root.clone();
        let folder_name_for_drop = folder_name.to_string();
        let flow_box_for_drop = flow_box.clone();

        drop_target.connect_drop(move |_target, value, _x, _y| {
            flow_box_for_drop.remove_css_class("drop-target-hover");

            let file_list: gdk::FileList = match value.get() {
                Ok(fl) => fl,
                Err(e) => {
                    log::warn!("[wallpaper/gallery] failed to get FileList from drop: {e}");
                    return false;
                }
            };

            let mut copied = false;
            for file in file_list.files() {
                let Some(path) = file.path() else { continue };
                if !is_image_extension(&path) {
                    log::debug!("[wallpaper/gallery] skipping non-image file: {}", path.display());
                    continue;
                }
                if copy_file_to_gallery_folder(&path, &folder_for_drop).is_some() {
                    copied = true;
                }
            }

            if copied {
                refresh_gallery_group(
                    &folder_name_for_drop,
                    &groups_for_drop,
                    &wallpaper_dir_for_drop,
                    &current_mode_for_drop,
                    &current_wallpaper_for_drop,
                    &action_callback_for_drop,
                    &current_urn_for_drop,
                    &root_for_drop,
                );
            }

            true
        });

        flow_box.add_controller(drop_target);
    }

    // Inter-gallery DropTarget (accepts STRING path, MOVE action)
    {
        let drop_target_internal =
            gtk::DropTarget::new(gtk::glib::Type::STRING, gdk::DragAction::MOVE);

        let flow_box_for_enter = flow_box.clone();
        drop_target_internal.connect_enter(move |_target, _x, _y| {
            flow_box_for_enter.add_css_class("drop-target-hover");
            gdk::DragAction::MOVE
        });

        let flow_box_for_leave = flow_box.clone();
        drop_target_internal.connect_leave(move |_target| {
            flow_box_for_leave.remove_css_class("drop-target-hover");
        });

        let target_folder_name = folder_name.to_string();
        let groups_for_move = groups.clone();
        let wallpaper_dir_for_move = wallpaper_dir.clone();
        let current_mode_for_move = current_mode.clone();
        let current_wallpaper_for_move = current_wallpaper.clone();
        let action_callback_for_move = action_callback.clone();
        let current_urn_for_move = current_urn.clone();
        let root_for_move = root.clone();
        let flow_box_for_drop = flow_box.clone();

        drop_target_internal.connect_drop(move |_target, value, _x, _y| {
            flow_box_for_drop.remove_css_class("drop-target-hover");

            let source_path_str: String = match value.get() {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("[wallpaper/gallery] failed to get string from drop: {e}");
                    return false;
                }
            };

            let source_path = PathBuf::from(&source_path_str);
            let dir = wallpaper_dir_for_move.borrow().clone();
            let expanded = expand_tilde(&dir);

            // Determine source folder
            let Some(source_folder) = source_folder_name(&source_path, &expanded) else {
                log::warn!(
                    "[wallpaper/gallery] dropped file not inside wallpaper dir: {}",
                    source_path.display()
                );
                return false;
            };

            // No-op if same folder
            if source_folder == target_folder_name {
                return true;
            }

            let dest_dir = expanded.join(&target_folder_name);
            if let Err(e) = std::fs::create_dir_all(&dest_dir) {
                log::warn!("[wallpaper/gallery] failed to create directory: {e}");
                return false;
            }

            let filename = source_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let dest_path = unique_dest_path(&filename, &dest_dir);

            if let Err(e) = std::fs::rename(&source_path, &dest_path) {
                log::warn!(
                    "[wallpaper/gallery] failed to move file {} to {}: {e}",
                    source_path.display(),
                    dest_path.display()
                );
                return false;
            }

            log::debug!(
                "[wallpaper/gallery] moved {} to {}",
                source_path.display(),
                dest_path.display()
            );

            // Refresh both source and target groups
            refresh_gallery_group(
                &source_folder,
                &groups_for_move,
                &wallpaper_dir_for_move,
                &current_mode_for_move,
                &current_wallpaper_for_move,
                &action_callback_for_move,
                &current_urn_for_move,
                &root_for_move,
            );
            refresh_gallery_group(
                &target_folder_name,
                &groups_for_move,
                &wallpaper_dir_for_move,
                &current_mode_for_move,
                &current_wallpaper_for_move,
                &action_callback_for_move,
                &current_urn_for_move,
                &root_for_move,
            );

            true
        });

        flow_box.add_controller(drop_target_internal);
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

    #[test]
    fn unique_dest_path_no_conflict() {
        let dir = std::env::temp_dir().join("waft-test-unique-dest");
        let _ = std::fs::create_dir_all(&dir);
        let result = unique_dest_path("image.png", &dir);
        assert_eq!(result.file_name().unwrap().to_str().unwrap(), "image.png");
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn unique_dest_path_with_conflict() {
        let dir = std::env::temp_dir().join("waft-test-unique-dest-conflict");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("image.png"), b"test").expect("write file");

        let result = unique_dest_path("image.png", &dir);
        assert_eq!(result.file_name().unwrap().to_str().unwrap(), "image_1.png");

        // Cleanup
        let _ = std::fs::remove_file(dir.join("image.png"));
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn is_image_extension_accepts_images() {
        assert!(is_image_extension(Path::new("photo.png")));
        assert!(is_image_extension(Path::new("photo.JPG")));
        assert!(is_image_extension(Path::new("photo.webp")));
    }

    #[test]
    fn is_image_extension_rejects_non_images() {
        assert!(!is_image_extension(Path::new("doc.pdf")));
        assert!(!is_image_extension(Path::new("script.sh")));
        assert!(!is_image_extension(Path::new("noext")));
    }

    #[test]
    fn source_folder_name_works() {
        let base = PathBuf::from("/home/user/wallpapers");
        let file = PathBuf::from("/home/user/wallpapers/dark/photo.png");
        assert_eq!(source_folder_name(&file, &base), Some("dark".to_string()));
    }

    #[test]
    fn source_folder_name_outside_base_returns_none() {
        let base = PathBuf::from("/home/user/wallpapers");
        let file = PathBuf::from("/tmp/other/photo.png");
        assert_eq!(source_folder_name(&file, &base), None);
    }
}
