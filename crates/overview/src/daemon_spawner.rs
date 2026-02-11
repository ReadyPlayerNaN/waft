//! Daemon plugin spawner
//!
//! Spawns plugin daemon binaries as detached processes.

use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

/// Configuration for daemon spawning
#[derive(Debug, Clone)]
pub struct DaemonSpawnerConfig {
    /// Directory containing daemon binaries
    pub daemon_dir: PathBuf,
}

impl Default for DaemonSpawnerConfig {
    fn default() -> Self {
        Self {
            daemon_dir: Self::detect_daemon_dir(),
        }
    }
}

impl DaemonSpawnerConfig {
    /// Detect daemon binary directory (same logic as plugin loader)
    fn detect_daemon_dir() -> PathBuf {
        // Check environment variable first
        if let Ok(dir) = std::env::var("WAFT_DAEMON_DIR") {
            return PathBuf::from(dir);
        }

        // Check development builds
        let debug_dir = PathBuf::from("./target/debug");
        if debug_dir.join("waft-clock-daemon").exists() {
            return debug_dir;
        }

        let release_dir = PathBuf::from("./target/release");
        if release_dir.join("waft-clock-daemon").exists() {
            return release_dir;
        }

        // Fall back to production path
        PathBuf::from("/usr/bin")
    }
}

/// Tracks spawned daemon processes
pub struct DaemonSpawner {
    config: DaemonSpawnerConfig,
    spawned: Vec<SpawnedDaemon>,
}

/// A spawned daemon process
struct SpawnedDaemon {
    name: String,
    _child: Child, // Keep handle to prevent zombie, but don't wait
}

impl DaemonSpawner {
    /// Create a new daemon spawner
    pub fn new(config: DaemonSpawnerConfig) -> Self {
        Self {
            config,
            spawned: Vec::new(),
        }
    }

    /// Spawn all known daemon binaries
    pub fn spawn_all_daemons(&mut self) {
        let daemon_names = vec![
            "waft-clock-daemon",
            "waft-darkman-daemon",
            "waft-caffeine-daemon",
            "waft-systemd-actions-daemon",
            "waft-battery-daemon",
            "waft-keyboard-layout-daemon",
            "waft-brightness-daemon",
            "waft-blueman-daemon",
            "waft-audio-daemon",
            "waft-networkmanager-daemon",
            "waft-weather-daemon",
            "waft-sunsetr-daemon",
            "waft-eds-agenda-daemon",
        ];

        for name in daemon_names {
            if let Err(e) = self.spawn_daemon(name) {
                warn!("Failed to spawn daemon {}: {}", name, e);
            }
        }
    }

    /// Spawn a specific daemon binary
    fn spawn_daemon(&mut self, name: &str) -> Result<(), String> {
        let binary_path = self.config.daemon_dir.join(name);

        if !binary_path.exists() {
            debug!("Daemon binary not found: {}", binary_path.display());
            return Err(format!("Binary not found: {}", binary_path.display()));
        }

        info!("Spawning daemon: {}", name);

        // Spawn as detached process (inherit stderr for debugging)
        let child = Command::new(&binary_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Failed to spawn {}: {}", name, e))?;

        info!("Daemon {} spawned with PID {}", name, child.id());

        // Store child handle (prevents zombie processes)
        self.spawned.push(SpawnedDaemon {
            name: name.to_string(),
            _child: child,
        });

        Ok(())
    }

    /// Check if a daemon binary exists
    pub fn daemon_exists(&self, name: &str) -> bool {
        self.config.daemon_dir.join(name).exists()
    }

    /// Get list of spawned daemons
    pub fn spawned_count(&self) -> usize {
        self.spawned.len()
    }
}

impl Drop for DaemonSpawner {
    fn drop(&mut self) {
        // Daemons will continue running after overview exits
        // They'll detect socket closure and can clean up themselves
        debug!("DaemonSpawner dropping, {} daemons spawned", self.spawned.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_spawner_config_default() {
        let config = DaemonSpawnerConfig::default();
        assert!(config.daemon_dir.as_os_str().len() > 0);
    }

    #[test]
    fn test_daemon_spawner_creation() {
        let config = DaemonSpawnerConfig::default();
        let spawner = DaemonSpawner::new(config);
        assert_eq!(spawner.spawned_count(), 0);
    }
}
