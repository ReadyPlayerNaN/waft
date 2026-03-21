//! Sound gallery: scanning, adding, and removing sound files.
//!
//! Manages sound files stored in `~/.config/waft/sounds/`.

use std::path::PathBuf;

use waft_protocol::entity::notification_sound::NotificationSound;

/// Maximum file size for uploaded sounds (5 MB).
const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;

/// Supported audio file extensions.
const SUPPORTED_EXTENSIONS: &[&str] = &["ogg", "oga", "wav", "flac", "mp3"];

/// Sound gallery managing files in the sounds directory.
pub struct SoundGallery {
    sounds_dir: PathBuf,
    sounds: Vec<NotificationSound>,
}

impl SoundGallery {
    /// Scan the sounds directory and build the gallery.
    pub fn scan() -> Self {
        let sounds_dir = sounds_directory();
        let sounds = scan_directory(&sounds_dir);
        Self { sounds_dir, sounds }
    }

    /// Get the current list of sounds.
    pub fn sounds(&self) -> &[NotificationSound] {
        &self.sounds
    }

    /// Add a sound file to the gallery from raw bytes.
    ///
    /// The filename stem is sanitized to kebab-case and deduplicated against
    /// existing sounds. Returns an error if the file exceeds the size limit,
    /// has an unsupported extension, or cannot be written to disk.
    pub fn add_sound(
        &mut self,
        filename: &str,
        data: &[u8],
    ) -> anyhow::Result<NotificationSound> {
        // Validate size
        if data.len() as u64 > MAX_FILE_SIZE {
            anyhow::bail!(
                "file exceeds maximum size of {} bytes ({} bytes provided)",
                MAX_FILE_SIZE,
                data.len()
            );
        }

        // Sanitize filename: only keep the base name (no path traversal)
        let base_filename = std::path::Path::new(filename)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid filename"))?;

        // Validate extension
        let ext = std::path::Path::new(base_filename)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        match ext {
            Some(ref e) if SUPPORTED_EXTENSIONS.contains(&e.as_str()) => {}
            _ => {
                anyhow::bail!(
                    "unsupported file extension: {}. Supported: {}",
                    ext.as_deref().unwrap_or("(none)"),
                    SUPPORTED_EXTENSIONS.join(", ")
                );
            }
        }
        let ext = ext.unwrap();

        // Sanitize the stem to kebab-case
        let raw_stem = std::path::Path::new(base_filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let stem = sanitize_sound_name(raw_stem);

        // Deduplicate against existing sounds
        let safe_filename = deduplicate_name(&stem, &ext, &self.sounds);

        // Ensure directory exists
        if let Err(e) = std::fs::create_dir_all(&self.sounds_dir) {
            anyhow::bail!("failed to create sounds directory: {e}");
        }

        // Write file
        let file_path = self.sounds_dir.join(&safe_filename);
        std::fs::write(&file_path, data)?;

        let sound = NotificationSound {
            filename: safe_filename.clone(),
            reference: format!("sounds/{safe_filename}"),
            size: data.len() as u64,
        };

        self.sounds.push(sound.clone());
        self.sounds.sort_by(|a, b| a.filename.cmp(&b.filename));

        Ok(sound)
    }

    /// Remove a sound file from the gallery.
    pub fn remove_sound(
        &mut self,
        filename: &str,
    ) -> anyhow::Result<()> {
        let file_path = self.sounds_dir.join(filename);
        if file_path.exists() {
            std::fs::remove_file(&file_path)?;
        }
        self.sounds.retain(|s| s.filename != filename);
        Ok(())
    }
}

/// Resolve a sound reference to a playable sound ID.
///
/// - `"sounds/foo.ogg"` -> `"/home/user/.config/waft/sounds/foo.ogg"`
/// - `"/absolute/path.ogg"` -> `"/absolute/path.ogg"` (unchanged)
/// - `"xdg-theme-name"` -> `"xdg-theme-name"` (unchanged)
pub fn resolve_sound_reference(reference: &str) -> String {
    if reference.starts_with("sounds/")
        && let Some(config_dir) = dirs::config_dir()
    {
        return config_dir
            .join("waft")
            .join(reference)
            .to_string_lossy()
            .into_owned();
    }
    reference.to_string()
}

/// Sanitize a sound name stem to kebab-case.
///
/// - Lowercases the input
/// - Replaces any character not in `[a-z0-9]` with a hyphen
/// - Collapses consecutive hyphens into one
/// - Trims leading and trailing hyphens
/// - Falls back to `"sound"` if the result is empty
fn sanitize_sound_name(stem: &str) -> String {
    let lowered = stem.to_lowercase();
    let replaced: String = lowered
        .chars()
        .map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() { c } else { '-' })
        .collect();

    let mut collapsed = String::with_capacity(replaced.len());
    let mut prev_hyphen = false;
    for c in replaced.chars() {
        if c == '-' {
            if !prev_hyphen {
                collapsed.push('-');
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }

    let trimmed = collapsed.trim_matches('-');
    if trimmed.is_empty() {
        "sound".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Produce a unique `{stem}.{ext}` filename not present in the existing sounds.
///
/// If `{stem}.{ext}` is already taken, appends `-2`, `-3`, etc. until a free
/// name is found.
fn deduplicate_name(stem: &str, ext: &str, sounds: &[NotificationSound]) -> String {
    let candidate = format!("{stem}.{ext}");
    if !sounds.iter().any(|s| s.filename == candidate) {
        return candidate;
    }

    let mut counter = 2u32;
    loop {
        let candidate = format!("{stem}-{counter}.{ext}");
        if !sounds.iter().any(|s| s.filename == candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// Get the sounds directory path.
fn sounds_directory() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("waft")
        .join("sounds")
}

/// Scan a directory for audio files and return sorted sound entries.
fn scan_directory(dir: &PathBuf) -> Vec<NotificationSound> {
    let mut sounds = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return sounds,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        let supported = matches!(ext, Some(ref e) if SUPPORTED_EXTENSIONS.contains(&e.as_str()));
        if !supported {
            continue;
        }

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

        sounds.push(NotificationSound {
            reference: format!("sounds/{filename}"),
            filename,
            size,
        });
    }

    sounds.sort_by(|a, b| a.filename.cmp(&b.filename));
    sounds
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_gallery() -> (tempfile::TempDir, SoundGallery) {
        let dir = tempfile::tempdir().unwrap();
        let gallery = SoundGallery {
            sounds_dir: dir.path().to_path_buf(),
            sounds: Vec::new(),
        };
        (dir, gallery)
    }

    #[test]
    fn scan_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let sounds = scan_directory(&dir.path().to_path_buf());
        assert!(sounds.is_empty());
    }

    #[test]
    fn scan_directory_with_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("alert.ogg"), b"fake ogg data").unwrap();
        fs::write(dir.path().join("bell.wav"), b"fake wav data").unwrap();
        fs::write(dir.path().join("readme.txt"), b"not audio").unwrap();

        let sounds = scan_directory(&dir.path().to_path_buf());
        assert_eq!(sounds.len(), 2);
        assert_eq!(sounds[0].filename, "alert.ogg");
        assert_eq!(sounds[1].filename, "bell.wav");
    }

    #[test]
    fn add_sound_writes_file_and_updates_list() {
        let (_dir, mut gallery) = temp_gallery();
        let data = b"fake audio content";
        let sound = gallery.add_sound("test.ogg", data).unwrap();

        assert_eq!(sound.filename, "test.ogg");
        assert_eq!(sound.reference, "sounds/test.ogg");
        assert_eq!(sound.size, data.len() as u64);
        assert_eq!(gallery.sounds().len(), 1);

        // File should exist on disk
        assert!(gallery.sounds_dir.join("test.ogg").exists());
    }

    #[test]
    fn add_sound_deduplicates_instead_of_replacing() {
        let (_dir, mut gallery) = temp_gallery();
        let first = gallery.add_sound("test.ogg", b"first").unwrap();
        let second = gallery.add_sound("test.ogg", b"second version").unwrap();

        assert_eq!(gallery.sounds().len(), 2);
        assert_eq!(first.filename, "test.ogg");
        assert_eq!(second.filename, "test-2.ogg");
        assert_eq!(second.size, b"second version".len() as u64);
    }

    #[test]
    fn add_sound_maintains_sorted_order() {
        let (_dir, mut gallery) = temp_gallery();
        gallery.add_sound("zebra.ogg", b"data").unwrap();
        gallery.add_sound("alpha.wav", b"data").unwrap();
        gallery.add_sound("middle.flac", b"data").unwrap();

        let names: Vec<&str> = gallery.sounds().iter().map(|s| s.filename.as_str()).collect();
        assert_eq!(names, vec!["alpha.wav", "middle.flac", "zebra.ogg"]);
    }

    #[test]
    fn remove_sound_deletes_file_and_updates_list() {
        let (_dir, mut gallery) = temp_gallery();
        gallery.add_sound("test.ogg", b"data").unwrap();
        assert_eq!(gallery.sounds().len(), 1);

        gallery.remove_sound("test.ogg").unwrap();
        assert!(gallery.sounds().is_empty());
        assert!(!gallery.sounds_dir.join("test.ogg").exists());
    }

    #[test]
    fn reject_files_over_size_limit() {
        let (_dir, mut gallery) = temp_gallery();
        let oversized = vec![0u8; (MAX_FILE_SIZE + 1) as usize];
        let result = gallery.add_sound("big.ogg", &oversized);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("maximum size"));
    }

    #[test]
    fn reject_unsupported_extension() {
        let (_dir, mut gallery) = temp_gallery();
        let result = gallery.add_sound("readme.txt", b"not audio");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unsupported"));
    }

    #[test]
    fn resolve_sounds_prefix() {
        let resolved = resolve_sound_reference("sounds/alert.ogg");
        // Should resolve to config_dir/waft/sounds/alert.ogg
        assert!(resolved.ends_with("waft/sounds/alert.ogg"));
        assert!(resolved.starts_with('/'));
    }

    #[test]
    fn resolve_absolute_path_unchanged() {
        let resolved = resolve_sound_reference("/usr/share/sounds/bell.ogg");
        assert_eq!(resolved, "/usr/share/sounds/bell.ogg");
    }

    #[test]
    fn resolve_xdg_name_unchanged() {
        let resolved = resolve_sound_reference("message-new-email");
        assert_eq!(resolved, "message-new-email");
    }

    #[test]
    fn sanitize_path_traversal() {
        let (_dir, mut gallery) = temp_gallery();
        let result = gallery.add_sound("../../../etc/passwd.ogg", b"data");
        // Should strip path traversal and sanitize the stem
        assert!(result.is_ok());
        let sound = result.unwrap();
        assert_eq!(sound.filename, "passwd.ogg");
    }

    #[test]
    fn sanitize_sound_name_basic() {
        assert_eq!(sanitize_sound_name("My Cool Alert"), "my-cool-alert");
    }

    #[test]
    fn sanitize_sound_name_unicode() {
        assert_eq!(sanitize_sound_name("Über Ding!!!"), "ber-ding");
    }

    #[test]
    fn sanitize_sound_name_hyphens_collapsed() {
        assert_eq!(sanitize_sound_name("---test---"), "test");
    }

    #[test]
    fn sanitize_sound_name_dots() {
        assert_eq!(sanitize_sound_name("..."), "sound");
    }

    #[test]
    fn sanitize_sound_name_empty() {
        assert_eq!(sanitize_sound_name(""), "sound");
    }

    #[test]
    fn sanitize_sound_name_underscores() {
        assert_eq!(sanitize_sound_name("café_latte"), "caf-latte");
    }

    #[test]
    fn sanitize_sound_name_preserves_digits() {
        assert_eq!(sanitize_sound_name("track01"), "track01");
    }

    #[test]
    fn sanitize_sound_name_only_special_chars() {
        assert_eq!(sanitize_sound_name("!!!@@@###"), "sound");
    }

    #[test]
    fn deduplicate_name_no_collision() {
        let sounds = vec![];
        assert_eq!(deduplicate_name("test", "ogg", &sounds), "test.ogg");
    }

    #[test]
    fn deduplicate_name_single_collision() {
        let sounds = vec![NotificationSound {
            filename: "test.ogg".to_string(),
            reference: "sounds/test.ogg".to_string(),
            size: 100,
        }];
        assert_eq!(deduplicate_name("test", "ogg", &sounds), "test-2.ogg");
    }

    #[test]
    fn deduplicate_name_multiple_collisions() {
        let sounds = vec![
            NotificationSound {
                filename: "test.ogg".to_string(),
                reference: "sounds/test.ogg".to_string(),
                size: 100,
            },
            NotificationSound {
                filename: "test-2.ogg".to_string(),
                reference: "sounds/test-2.ogg".to_string(),
                size: 100,
            },
            NotificationSound {
                filename: "test-3.ogg".to_string(),
                reference: "sounds/test-3.ogg".to_string(),
                size: 100,
            },
        ];
        assert_eq!(deduplicate_name("test", "ogg", &sounds), "test-4.ogg");
    }

    #[test]
    fn add_sound_sanitizes_filename() {
        let (_dir, mut gallery) = temp_gallery();
        let sound = gallery.add_sound("My Cool Alert.ogg", b"data").unwrap();
        assert_eq!(sound.filename, "my-cool-alert.ogg");
        assert_eq!(sound.reference, "sounds/my-cool-alert.ogg");
        assert!(gallery.sounds_dir.join("my-cool-alert.ogg").exists());
    }

    #[test]
    fn add_sound_duplicate_upload_creates_distinct_entries() {
        let (_dir, mut gallery) = temp_gallery();
        let first = gallery.add_sound("My File.ogg", b"data1").unwrap();
        let second = gallery.add_sound("My File.ogg", b"data2").unwrap();

        assert_eq!(first.filename, "my-file.ogg");
        assert_eq!(second.filename, "my-file-2.ogg");
        assert_eq!(gallery.sounds().len(), 2);
        assert!(gallery.sounds_dir.join("my-file.ogg").exists());
        assert!(gallery.sounds_dir.join("my-file-2.ogg").exists());
    }
}
