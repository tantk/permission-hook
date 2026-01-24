//! Deduplication manager with two-phase locking

use crate::platform;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// Deduplication manager
pub struct Manager {
    temp_dir: PathBuf,
}

impl Manager {
    pub fn new() -> Self {
        Self {
            temp_dir: platform::temp_dir(),
        }
    }

    /// Get lock file path
    fn get_lock_path(&self, session_id: &str, hook_event: Option<&str>) -> PathBuf {
        let name = match hook_event {
            Some(event) => format!("claude-notification-{}-{}.lock", session_id, event),
            None => format!("claude-notification-{}.lock", session_id),
        };
        self.temp_dir.join(name)
    }

    /// Get content lock file path (for cross-hook dedup)
    fn get_content_lock_path(&self, session_id: &str) -> PathBuf {
        self.temp_dir.join(format!("claude-notification-content-{}.lock", session_id))
    }

    /// Phase 1: Early duplicate check (fast, non-blocking)
    /// Returns true if this is likely a duplicate
    pub fn check_early_duplicate(&self, session_id: &str, hook_event: Option<&str>) -> bool {
        let lock_path = self.get_lock_path(session_id, hook_event);

        if !lock_path.exists() {
            return false;
        }

        // Check if lock is fresh (< 2 seconds old)
        if let Some(mtime) = platform::file_mtime(lock_path.to_str().unwrap_or("")) {
            let age = platform::current_timestamp() - mtime;
            if age < 2 {
                return true; // Fresh lock = duplicate
            }
        }

        false // Stale lock, not a duplicate
    }

    /// Phase 2: Acquire lock atomically
    /// Returns true if lock was acquired, false if duplicate
    pub fn acquire_lock(&self, session_id: &str, hook_event: Option<&str>) -> Result<bool, String> {
        let lock_path = self.get_lock_path(session_id, hook_event);

        // Check if lock exists and is fresh
        if lock_path.exists() {
            if let Some(mtime) = platform::file_mtime(lock_path.to_str().unwrap_or("")) {
                let age = platform::current_timestamp() - mtime;
                if age < 2 {
                    return Ok(false); // Fresh lock = duplicate
                }
                // Stale lock - remove it
                let _ = fs::remove_file(&lock_path);
            }
        }

        // Try to create lock file atomically
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                // Write timestamp to lock file
                let _ = write!(file, "{}", platform::current_timestamp());
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Another process beat us to it
                Ok(false)
            }
            Err(e) => Err(format!("Failed to create lock file: {}", e)),
        }
    }

    /// Release lock (for explicit release, though usually we let it age out)
    pub fn release_lock(&self, session_id: &str, hook_event: Option<&str>) -> Result<(), String> {
        let lock_path = self.get_lock_path(session_id, hook_event);

        if lock_path.exists() {
            fs::remove_file(&lock_path)
                .map_err(|e| format!("Failed to release lock: {}", e))?;
        }

        Ok(())
    }

    /// Acquire content lock (for cross-hook dedup, 5 second TTL)
    pub fn acquire_content_lock(&self, session_id: &str) -> Result<bool, String> {
        let lock_path = self.get_content_lock_path(session_id);

        // Check if lock exists and is fresh (5 second TTL)
        if lock_path.exists() {
            if let Some(mtime) = platform::file_mtime(lock_path.to_str().unwrap_or("")) {
                let age = platform::current_timestamp() - mtime;
                if age < 5 {
                    return Ok(false); // Lock is held
                }
                // Stale lock - remove it
                let _ = fs::remove_file(&lock_path);
            }
        }

        // Try to create lock file atomically
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                let _ = write!(file, "{}", platform::current_timestamp());
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Ok(false)
            }
            Err(e) => Err(format!("Failed to create content lock: {}", e)),
        }
    }

    /// Release content lock
    pub fn release_content_lock(&self, session_id: &str) -> Result<(), String> {
        let lock_path = self.get_content_lock_path(session_id);

        if lock_path.exists() {
            fs::remove_file(&lock_path)
                .map_err(|e| format!("Failed to release content lock: {}", e))?;
        }

        Ok(())
    }

    /// Cleanup old lock files
    pub fn cleanup(&self, max_age_seconds: i64) -> Result<(), String> {
        let now = platform::current_timestamp();

        let entries = fs::read_dir(&self.temp_dir)
            .map_err(|e| format!("Failed to read temp dir: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("claude-notification-") && name.ends_with(".lock") {
                    if let Some(mtime) = platform::file_mtime(path.to_str().unwrap_or("")) {
                        if now - mtime > max_age_seconds {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Cleanup all locks for a specific session
    pub fn cleanup_for_session(&self, session_id: &str) -> Result<(), String> {
        let entries = fs::read_dir(&self.temp_dir)
            .map_err(|e| format!("Failed to read temp dir: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.contains(session_id) && name.ends_with(".lock") {
                    fs::remove_file(&path)
                        .map_err(|e| format!("Failed to remove lock: {}", e))?;
                }
            }
        }

        Ok(())
    }
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager() -> Manager {
        Manager::new()
    }

    fn unique_session_id() -> String {
        format!("test-dedup-{}-{:?}", platform::current_timestamp(), std::thread::current().id())
    }

    #[test]
    fn test_check_early_duplicate_no_lock() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        let is_dup = mgr.check_early_duplicate(&session_id, None);
        assert!(!is_dup);
    }

    #[test]
    fn test_acquire_lock() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        // First acquisition should succeed
        let acquired = mgr.acquire_lock(&session_id, None).unwrap();
        assert!(acquired);

        // Second acquisition should fail (fresh lock)
        let acquired = mgr.acquire_lock(&session_id, None).unwrap();
        assert!(!acquired);

        // Cleanup
        mgr.release_lock(&session_id, None).unwrap();
    }

    #[test]
    fn test_acquire_lock_with_hook_event() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        // Locks with different hook events should be independent
        let acquired1 = mgr.acquire_lock(&session_id, Some("Stop")).unwrap();
        assert!(acquired1);

        let acquired2 = mgr.acquire_lock(&session_id, Some("Notification")).unwrap();
        assert!(acquired2);

        // Same hook event should fail
        let acquired3 = mgr.acquire_lock(&session_id, Some("Stop")).unwrap();
        assert!(!acquired3);

        // Cleanup
        mgr.release_lock(&session_id, Some("Stop")).unwrap();
        mgr.release_lock(&session_id, Some("Notification")).unwrap();
    }

    #[test]
    fn test_release_lock() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        mgr.acquire_lock(&session_id, None).unwrap();
        mgr.release_lock(&session_id, None).unwrap();

        // Should be able to acquire again after release
        let acquired = mgr.acquire_lock(&session_id, None).unwrap();
        assert!(acquired);

        // Cleanup
        mgr.release_lock(&session_id, None).unwrap();
    }

    #[test]
    fn test_content_lock() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        let acquired = mgr.acquire_content_lock(&session_id).unwrap();
        assert!(acquired);

        let acquired = mgr.acquire_content_lock(&session_id).unwrap();
        assert!(!acquired);

        mgr.release_content_lock(&session_id).unwrap();

        let acquired = mgr.acquire_content_lock(&session_id).unwrap();
        assert!(acquired);

        // Cleanup
        mgr.release_content_lock(&session_id).unwrap();
    }

    #[test]
    fn test_cleanup_for_session() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        mgr.acquire_lock(&session_id, Some("Stop")).unwrap();
        mgr.acquire_lock(&session_id, Some("Notification")).unwrap();
        mgr.acquire_content_lock(&session_id).unwrap();

        mgr.cleanup_for_session(&session_id).unwrap();

        // All locks should be cleaned up, can acquire again
        let acquired = mgr.acquire_lock(&session_id, Some("Stop")).unwrap();
        assert!(acquired);

        // Cleanup
        mgr.cleanup_for_session(&session_id).unwrap();
    }
}
