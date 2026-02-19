//! Sound playback via subprocess.
//!
//! - XDG theme sound IDs: `canberra-gtk-play --id <id>`
//! - File paths: `paplay <path>`
//!
//! `canberra-gtk-play` requires a GTK display context and cannot be used for
//! file playback from a daemon plugin. `paplay` speaks directly to
//! PulseAudio/PipeWire and works without a display.
//!
//! Both are non-blocking (fire and forget) and degrade gracefully when the
//! binary is not installed.

use std::sync::atomic::{AtomicBool, Ordering};

/// Sound player that dispatches to the appropriate backend.
pub struct SoundPlayer {
    /// Set to true after the first failed attempt to find canberra-gtk-play.
    warned_missing_canberra: AtomicBool,
    /// Set to true after the first failed attempt to find paplay.
    warned_missing_paplay: AtomicBool,
}

impl SoundPlayer {
    pub fn new() -> Self {
        Self {
            warned_missing_canberra: AtomicBool::new(false),
            warned_missing_paplay: AtomicBool::new(false),
        }
    }

    /// Play a sound by XDG theme name or absolute file path.
    ///
    /// - Absolute paths (`/…`) are played via `paplay`, which works in daemon
    ///   context without a display.
    /// - Everything else is treated as an XDG sound theme ID and played via
    ///   `canberra-gtk-play --id`.
    ///
    /// Errors are logged but never propagated — sound playback must not block
    /// or fail notification delivery.
    pub async fn play(&self, sound_id: &str) {
        if sound_id.starts_with('/') {
            self.play_file(sound_id).await;
        } else {
            self.play_theme_id(sound_id).await;
        }
    }

    async fn play_file(&self, path: &str) {
        let mut cmd = tokio::process::Command::new("paplay");
        cmd.arg(path);
        self.spawn_and_reap(cmd, "paplay", &self.warned_missing_paplay)
            .await;
    }

    async fn play_theme_id(&self, sound_id: &str) {
        let mut cmd = tokio::process::Command::new("canberra-gtk-play");
        cmd.arg("--id").arg(sound_id);
        self.spawn_and_reap(cmd, "canberra-gtk-play", &self.warned_missing_canberra)
            .await;
    }

    async fn spawn_and_reap(
        &self,
        mut cmd: tokio::process::Command,
        binary: &'static str,
        warned: &AtomicBool,
    ) {
        match cmd.spawn() {
            Ok(mut child) => {
                tokio::spawn(async move {
                    match child.wait().await {
                        Ok(status) => {
                            if !status.success() {
                                log::debug!(
                                    "[notifications/sound] {binary} exited with {status}"
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!(
                                "[notifications/sound] failed to wait on {binary}: {e}"
                            );
                        }
                    }
                });
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    if !warned.swap(true, Ordering::Relaxed) {
                        log::warn!(
                            "[notifications/sound] {binary} not found — \
                             install it to enable sound playback."
                        );
                    }
                } else {
                    log::warn!("[notifications/sound] failed to spawn {binary}: {e}");
                }
            }
        }
    }
}

impl Default for SoundPlayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn play_does_not_panic_with_missing_binary() {
        // SoundPlayer should gracefully handle the case where
        // canberra-gtk-play is not in PATH. This test verifies
        // it doesn't panic or block.
        let player = SoundPlayer::new();
        player.play("message-new-email").await;
        // If we get here without panic, the test passes
    }

    #[test]
    fn new_creates_player_without_warning() {
        let player = SoundPlayer::new();
        assert!(!player.warned_missing_canberra.load(Ordering::Relaxed));
        assert!(!player.warned_missing_paplay.load(Ordering::Relaxed));
    }
}
