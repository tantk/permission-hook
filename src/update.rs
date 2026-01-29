//! Auto-update checking for claude-permission-hook
//!
//! Checks GitHub releases for new versions and notifies the user.

use crate::config::{Config, get_update_state_path};
use crate::logging;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// State persisted between update checks
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UpdateState {
    /// Unix timestamp of last check
    pub last_check: u64,
    /// Latest version found (if any)
    pub latest_version: Option<String>,
    /// Whether user has been notified about this version
    pub notified: bool,
}

/// GitHub release response (minimal fields)
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

impl UpdateState {
    /// Load state from disk
    pub fn load() -> Self {
        let path = get_update_state_path();
        if let Ok(content) = fs::read_to_string(&path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    /// Save state to disk
    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = get_update_state_path();
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)
    }

    /// Check if we should check for updates (based on interval)
    pub fn should_check(&self, interval_hours: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let interval_secs = interval_hours * 3600;
        now.saturating_sub(self.last_check) >= interval_secs
    }
}

/// Compare two semver version strings (e.g., "1.0.0" vs "1.0.1")
/// Returns true if `latest` is newer than `current`
fn is_newer_version(current: &str, latest: &str) -> bool {
    let parse_version = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };

    let current_parts = parse_version(current);
    let latest_parts = parse_version(latest);

    for i in 0..3 {
        let c = current_parts.get(i).copied().unwrap_or(0);
        let l = latest_parts.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    false
}

/// Fetch the latest release version from GitHub
fn fetch_latest_version(repo: &str) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("claude-permission-hook")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .send()
        .map_err(|e| format!("Failed to fetch releases: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("GitHub API returned status: {}", response.status()));
    }

    let release: GitHubRelease = response
        .json()
        .map_err(|e| format!("Failed to parse release JSON: {}", e))?;

    Ok(release.tag_name)
}

/// Check for updates and return Some((current, latest)) if update available
pub fn check_for_update(config: &Config) -> Option<(String, String)> {
    if !config.updates.check_enabled {
        return None;
    }

    let mut state = UpdateState::load();

    // Check if we should check based on interval
    if !state.should_check(config.updates.check_interval_hours) {
        // Still return update info if we have it and haven't notified
        if let Some(ref latest) = state.latest_version {
            if !state.notified && is_newer_version(VERSION, latest) {
                return Some((VERSION.to_string(), latest.clone()));
            }
        }
        return None;
    }

    // Perform the check
    logging::debug(config, &format!("Checking for updates from {}", config.updates.github_repo));

    match fetch_latest_version(&config.updates.github_repo) {
        Ok(latest) => {
            // Update state
            state.last_check = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let has_update = is_newer_version(VERSION, &latest);

            if has_update {
                state.latest_version = Some(latest.clone());
                state.notified = false;
                let _ = state.save();
                Some((VERSION.to_string(), latest))
            } else {
                state.latest_version = None;
                state.notified = false;
                let _ = state.save();
                None
            }
        }
        Err(e) => {
            logging::debug(config, &format!("Update check failed: {}", e));
            // Update last_check to avoid hammering the API on failure
            state.last_check = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = state.save();
            None
        }
    }
}

/// Mark that the user has been notified about the update
pub fn mark_notified() {
    let mut state = UpdateState::load();
    state.notified = true;
    let _ = state.save();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("1.0.0", "1.0.1"));
        assert!(is_newer_version("1.0.0", "1.1.0"));
        assert!(is_newer_version("1.0.0", "2.0.0"));
        assert!(is_newer_version("1.0.0", "v1.0.1"));
        assert!(!is_newer_version("1.0.1", "1.0.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        assert!(!is_newer_version("2.0.0", "1.9.9"));
    }

    #[test]
    fn test_should_check() {
        let mut state = UpdateState::default();
        assert!(state.should_check(24)); // Never checked, should check

        state.last_check = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(!state.should_check(24)); // Just checked, should not check
    }
}
