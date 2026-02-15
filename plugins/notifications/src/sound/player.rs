//! Sound playback via `canberra-gtk-play` subprocess.
//!
//! Spawns `canberra-gtk-play --id <sound_id>` as a child process.
//! Non-blocking: fires and forgets. Gracefully degrades when the binary
//! is not available.

use std::sync::atomic::{AtomicBool, Ordering};

/// Sound player that wraps `canberra-gtk-play` subprocess invocation.
pub struct SoundPlayer {
    /// Set to true after the first failed attempt to find canberra-gtk-play.
    /// Prevents log spam when the binary is not installed.
    warned_missing: AtomicBool,
}

impl SoundPlayer {
    pub fn new() -> Self {
        Self {
            warned_missing: AtomicBool::new(false),
        }
    }

    /// Play a sound by XDG theme name or file path.
    ///
    /// This spawns `canberra-gtk-play` as a subprocess. If the sound_id starts
    /// with `/`, it is treated as a file path and passed via `--file`. Otherwise,
    /// it is treated as an XDG sound theme name and passed via `--id`.
    ///
    /// Errors are logged but never propagated -- sound playback must not block
    /// or fail notification delivery.
    pub async fn play(&self, sound_id: &str) {
        let mut cmd = tokio::process::Command::new("canberra-gtk-play");

        if sound_id.starts_with('/') {
            cmd.arg("--file").arg(sound_id);
        } else {
            cmd.arg("--id").arg(sound_id);
        }

        match cmd.spawn() {
            Ok(mut child) => {
                // Reap the child process in a background task to avoid zombies
                tokio::spawn(async move {
                    match child.wait().await {
                        Ok(status) => {
                            if !status.success() {
                                log::debug!(
                                    "[notifications/sound] canberra-gtk-play exited with {}",
                                    status
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!(
                                "[notifications/sound] failed to wait on canberra-gtk-play: {e}"
                            );
                        }
                    }
                });
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    if !self.warned_missing.swap(true, Ordering::Relaxed) {
                        log::warn!(
                            "[notifications/sound] canberra-gtk-play not found -- \
                             notification sounds will be silent. \
                             Install libcanberra to enable sound playback."
                        );
                    }
                } else {
                    log::warn!(
                        "[notifications/sound] failed to spawn canberra-gtk-play: {e}"
                    );
                }
            }
        }
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
        assert!(!player.warned_missing.load(Ordering::Relaxed));
    }
}
