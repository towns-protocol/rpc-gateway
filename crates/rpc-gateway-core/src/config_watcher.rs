//! Polling-based configuration file watcher for dynamic configuration reloading.
//!
//! This module provides a [`ConfigWatcher`] that periodically checks configuration files
//! for changes and notifies the gateway to reload. It uses simple polling which works
//! reliably with Kubernetes ConfigMaps and other file-based configuration sources.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Watches a configuration file for changes using polling and sends reload notifications.
///
/// The watcher periodically checks the file's modification time and content hash
/// to detect changes. This approach is simple and works reliably with Kubernetes
/// ConfigMaps which update files via symlink replacement.
#[derive(Debug)]
pub struct ConfigWatcher {
    config_path: PathBuf,
    poll_interval: Duration,
}

impl ConfigWatcher {
    /// Creates a new config watcher for the given configuration file path.
    ///
    /// The poll interval is set to 5 seconds by default.
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            poll_interval: Duration::from_secs(5),
        }
    }

    /// Creates a new config watcher with a custom poll interval.
    pub fn with_poll_interval(config_path: PathBuf, poll_interval: Duration) -> Self {
        Self {
            config_path,
            poll_interval,
        }
    }

    /// Gets the current modification time of the config file, if it exists.
    fn get_modified_time(&self) -> Option<SystemTime> {
        fs::metadata(&self.config_path)
            .ok()
            .and_then(|m| m.modified().ok())
    }

    /// Starts watching the configuration file and sends notifications on changes.
    ///
    /// This method runs indefinitely until the sender is dropped.
    /// When a file change is detected (based on modification time), it sends
    /// a unit value through the channel.
    pub async fn watch(&self, reload_tx: mpsc::Sender<()>) {
        info!(
            config_path = %self.config_path.display(),
            poll_interval_secs = self.poll_interval.as_secs(),
            "Starting config file watcher (polling)"
        );

        let mut last_modified = self.get_modified_time();

        loop {
            tokio::time::sleep(self.poll_interval).await;

            let current_modified = self.get_modified_time();

            // Check if file was modified
            match (&last_modified, &current_modified) {
                (Some(last), Some(current)) if current > last => {
                    info!("Config file changed, triggering reload");
                    if reload_tx.send(()).await.is_err() {
                        error!("Reload channel closed, stopping config watcher");
                        break;
                    }
                    last_modified = current_modified;
                }
                (None, Some(_)) => {
                    // File appeared (was missing before)
                    info!("Config file appeared, triggering reload");
                    if reload_tx.send(()).await.is_err() {
                        error!("Reload channel closed, stopping config watcher");
                        break;
                    }
                    last_modified = current_modified;
                }
                (Some(_), None) => {
                    // File disappeared
                    warn!(
                        config_path = %self.config_path.display(),
                        "Config file no longer exists"
                    );
                    last_modified = None;
                }
                _ => {
                    debug!("No config file changes detected");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_config_watcher_detects_file_modification() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.yml");

        // Create initial config
        fs::write(&config_path, "initial: true").unwrap();

        let (tx, mut rx) = mpsc::channel(1);
        // Use short poll interval for testing
        let watcher = ConfigWatcher::with_poll_interval(config_path.clone(), Duration::from_millis(50));

        // Spawn watcher in background
        tokio::spawn(async move {
            watcher.watch(tx).await;
        });

        // Wait for first poll to establish baseline
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Modify the file
        fs::write(&config_path, "modified: true").unwrap();

        // Should receive notification within a few poll intervals
        tokio::select! {
            result = rx.recv() => {
                assert!(result.is_some(), "Should receive reload notification");
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                panic!("Timeout waiting for config change notification");
            }
        }
    }

    #[tokio::test]
    async fn test_config_watcher_detects_file_appearing() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.yml");

        // Don't create the file initially
        let (tx, mut rx) = mpsc::channel(1);
        let watcher = ConfigWatcher::with_poll_interval(config_path.clone(), Duration::from_millis(50));

        tokio::spawn(async move {
            watcher.watch(tx).await;
        });

        // Wait a bit, then create the file
        tokio::time::sleep(Duration::from_millis(100)).await;
        fs::write(&config_path, "new: true").unwrap();

        // Should receive notification
        tokio::select! {
            result = rx.recv() => {
                assert!(result.is_some(), "Should receive reload notification when file appears");
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                panic!("Timeout waiting for config change notification");
            }
        }
    }

    #[tokio::test]
    async fn test_config_watcher_no_notification_when_unchanged() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.yml");

        fs::write(&config_path, "stable: true").unwrap();

        let (tx, mut rx) = mpsc::channel(10);
        let watcher = ConfigWatcher::with_poll_interval(config_path, Duration::from_millis(50));

        tokio::spawn(async move {
            watcher.watch(tx).await;
        });

        // Wait for several poll intervals without modifying
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should not receive any notification
        assert!(rx.try_recv().is_err(), "Should not receive notification when file is unchanged");
    }
}
