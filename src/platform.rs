//! Cross-platform utilities

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Get current Unix timestamp in seconds
pub fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Check if a file exists
pub fn file_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// Get file modification time as Unix timestamp
pub fn file_mtime(path: &str) -> Option<i64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

/// Get temp directory path
pub fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir()
}

/// Get git branch name from a directory
pub fn get_git_branch(cwd: &str) -> Option<String> {
    if cwd.is_empty() {
        return None;
    }

    let git_head = Path::new(cwd).join(".git/HEAD");
    if !git_head.exists() {
        return None;
    }

    std::fs::read_to_string(&git_head)
        .ok()
        .and_then(|content| {
            let content = content.trim();
            if content.starts_with("ref: refs/heads/") {
                Some(content.strip_prefix("ref: refs/heads/")?.to_string())
            } else {
                // Detached HEAD - return short hash
                Some(content.chars().take(7).collect())
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        assert!(ts > 1700000000); // After Nov 2023
    }

    #[test]
    fn test_file_exists() {
        assert!(file_exists("Cargo.toml"));
        assert!(!file_exists("nonexistent_file_12345.xyz"));
    }

    #[test]
    fn test_temp_dir() {
        let dir = temp_dir();
        assert!(dir.exists());
    }
}
