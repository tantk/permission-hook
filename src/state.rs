//! Session state management for notification cooldowns and deduplication

use crate::analyzer::Status;
use crate::platform;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Per-session state stored in temp directory
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub session_id: String,
    #[serde(default)]
    pub last_interactive_tool: String,
    #[serde(default)]
    pub last_timestamp: i64,
    #[serde(default)]
    pub last_task_complete_time: i64,
    #[serde(default)]
    pub last_notification_time: i64,
    #[serde(default)]
    pub last_notification_status: String,
    #[serde(default)]
    pub last_notification_message: String,
    #[serde(default)]
    pub cwd: String,
}

/// State manager for session state
pub struct Manager {
    temp_dir: PathBuf,
}

impl Manager {
    pub fn new() -> Self {
        Self {
            temp_dir: platform::temp_dir(),
        }
    }

    /// Get state file path for a session
    fn get_state_path(&self, session_id: &str) -> PathBuf {
        self.temp_dir.join(format!("claude-session-state-{}.json", session_id))
    }

    /// Load session state
    pub fn load(&self, session_id: &str) -> Result<Option<SessionState>, String> {
        let path = self.get_state_path(session_id);

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read state file: {}", e))?;

        let state: SessionState = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse state file: {}", e))?;

        Ok(Some(state))
    }

    /// Save session state
    pub fn save(&self, state: &SessionState) -> Result<(), String> {
        let path = self.get_state_path(&state.session_id);

        let content = serde_json::to_string_pretty(state)
            .map_err(|e| format!("Failed to serialize state: {}", e))?;

        fs::write(&path, content)
            .map_err(|e| format!("Failed to write state file: {}", e))?;

        Ok(())
    }

    /// Delete session state
    pub fn delete(&self, session_id: &str) -> Result<(), String> {
        let path = self.get_state_path(session_id);

        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete state file: {}", e))?;
        }

        Ok(())
    }

    /// Update interactive tool state
    pub fn update_interactive_tool(&self, session_id: &str, tool: &str, cwd: &str) -> Result<(), String> {
        let mut state = self.load(session_id)?.unwrap_or_else(|| SessionState {
            session_id: session_id.to_string(),
            ..Default::default()
        });

        state.last_interactive_tool = tool.to_string();
        state.last_timestamp = platform::current_timestamp();
        state.cwd = cwd.to_string();

        self.save(&state)
    }

    /// Update task complete timestamp
    pub fn update_task_complete(&self, session_id: &str) -> Result<(), String> {
        let mut state = self.load(session_id)?.unwrap_or_else(|| SessionState {
            session_id: session_id.to_string(),
            ..Default::default()
        });

        state.last_task_complete_time = platform::current_timestamp();

        self.save(&state)
    }

    /// Update last notification
    pub fn update_last_notification(&self, session_id: &str, status: Status, message: &str) -> Result<(), String> {
        let mut state = self.load(session_id)?.unwrap_or_else(|| SessionState {
            session_id: session_id.to_string(),
            ..Default::default()
        });

        state.last_notification_time = platform::current_timestamp();
        state.last_notification_status = status.as_str().to_string();
        state.last_notification_message = message.to_string();

        self.save(&state)
    }

    /// Check if question should be suppressed after task complete
    pub fn should_suppress_question(&self, session_id: &str, cooldown_seconds: i64) -> Result<bool, String> {
        if cooldown_seconds <= 0 {
            return Ok(false);
        }

        let state = match self.load(session_id)? {
            Some(s) => s,
            None => return Ok(false),
        };

        if state.last_task_complete_time == 0 {
            return Ok(false);
        }

        let elapsed = platform::current_timestamp() - state.last_task_complete_time;
        Ok(elapsed < cooldown_seconds)
    }

    /// Check if question should be suppressed after any notification
    pub fn should_suppress_question_after_any(&self, session_id: &str, cooldown_seconds: i64) -> Result<bool, String> {
        if cooldown_seconds <= 0 {
            return Ok(false);
        }

        let state = match self.load(session_id)? {
            Some(s) => s,
            None => return Ok(false),
        };

        if state.last_notification_time == 0 {
            return Ok(false);
        }

        let elapsed = platform::current_timestamp() - state.last_notification_time;
        Ok(elapsed < cooldown_seconds)
    }

    /// Check if message is a duplicate
    pub fn is_duplicate_message(&self, session_id: &str, message: &str, window_seconds: i64) -> Result<bool, String> {
        if window_seconds <= 0 {
            return Ok(false);
        }

        let state = match self.load(session_id)? {
            Some(s) => s,
            None => return Ok(false),
        };

        if state.last_notification_message.is_empty() {
            return Ok(false);
        }

        // Check time window
        let elapsed = platform::current_timestamp() - state.last_notification_time;
        if elapsed >= window_seconds {
            return Ok(false);
        }

        // Normalize and compare messages
        let normalize = |s: &str| -> String {
            s.to_lowercase()
                .trim()
                .replace("..", ".")
                .to_string()
        };

        Ok(normalize(message) == normalize(&state.last_notification_message))
    }

    /// Update state based on status
    pub fn update_state(&self, session_id: &str, status: Status, tool: &str, cwd: &str) -> Result<(), String> {
        match status {
            Status::TaskComplete | Status::ReviewComplete => {
                self.update_task_complete(session_id)?;
            }
            Status::PlanReady | Status::Question => {
                if !tool.is_empty() {
                    self.update_interactive_tool(session_id, tool, cwd)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Cleanup old state files
    pub fn cleanup(&self, max_age_seconds: i64) -> Result<(), String> {
        let _pattern = "claude-session-state-*.json";
        let now = platform::current_timestamp();

        let entries = fs::read_dir(&self.temp_dir)
            .map_err(|e| format!("Failed to read temp dir: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("claude-session-state-") && name.ends_with(".json") {
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
        format!("test-state-{}-{:?}", platform::current_timestamp(), std::thread::current().id())
    }

    #[test]
    fn test_load_nonexistent() {
        let mgr = test_manager();
        let result = mgr.load("nonexistent-session-12345");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_save_and_load() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        let state = SessionState {
            session_id: session_id.clone(),
            last_interactive_tool: "ExitPlanMode".into(),
            last_timestamp: platform::current_timestamp(),
            cwd: "/test/dir".into(),
            ..Default::default()
        };

        mgr.save(&state).unwrap();

        let loaded = mgr.load(&session_id).unwrap().unwrap();
        assert_eq!(loaded.session_id, session_id);
        assert_eq!(loaded.last_interactive_tool, "ExitPlanMode");
        assert_eq!(loaded.cwd, "/test/dir");

        // Cleanup
        mgr.delete(&session_id).unwrap();
    }

    #[test]
    fn test_delete() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        let state = SessionState {
            session_id: session_id.clone(),
            ..Default::default()
        };

        mgr.save(&state).unwrap();
        assert!(mgr.load(&session_id).unwrap().is_some());

        mgr.delete(&session_id).unwrap();
        assert!(mgr.load(&session_id).unwrap().is_none());
    }

    #[test]
    fn test_should_suppress_question_within_cooldown() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        mgr.update_task_complete(&session_id).unwrap();

        let suppress = mgr.should_suppress_question(&session_id, 60).unwrap();
        assert!(suppress);

        // Cleanup
        mgr.delete(&session_id).unwrap();
    }

    #[test]
    fn test_should_not_suppress_zero_cooldown() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        mgr.update_task_complete(&session_id).unwrap();

        let suppress = mgr.should_suppress_question(&session_id, 0).unwrap();
        assert!(!suppress);

        // Cleanup
        mgr.delete(&session_id).unwrap();
    }

    #[test]
    fn test_is_duplicate_message() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        mgr.update_last_notification(&session_id, Status::TaskComplete, "Test message").unwrap();

        // Same message should be duplicate
        let is_dup = mgr.is_duplicate_message(&session_id, "Test message", 180).unwrap();
        assert!(is_dup);

        // Different message should not be duplicate
        let is_dup = mgr.is_duplicate_message(&session_id, "Different message", 180).unwrap();
        assert!(!is_dup);

        // Cleanup
        mgr.delete(&session_id).unwrap();
    }

    #[test]
    fn test_is_duplicate_normalized() {
        let mgr = test_manager();
        let session_id = unique_session_id();

        mgr.update_last_notification(&session_id, Status::TaskComplete, "Test Message..").unwrap();

        // Normalized (case + dots) should match
        let is_dup = mgr.is_duplicate_message(&session_id, "TEST MESSAGE.", 180).unwrap();
        assert!(is_dup);

        // Cleanup
        mgr.delete(&session_id).unwrap();
    }
}
