//! Logging utilities for permission-hook

use crate::config::{get_config_dir, get_log_path, get_prompts_path, Config};
use chrono::Utc;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;

#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub tool: String,
    pub decision: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Truncate string to max length
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Log a permission decision
pub fn log_decision(config: &Config, tool: &str, decision: &str, reason: &str, details: Option<&str>) {
    if !config.logging.enabled {
        return;
    }

    let log_dir = get_config_dir();
    let _ = fs::create_dir_all(&log_dir);

    let entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
        tool: tool.to_string(),
        decision: decision.to_string(),
        reason: truncate(reason, 150),
        details: details.map(|d| truncate(d, 100)),
    };

    if let Ok(json) = serde_json::to_string(&entry) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(get_log_path())
        {
            let _ = writeln!(file, "{}", json);
        }
    }
}

/// Log a prompt event to separate file for easy checking
pub fn log_prompt(tool: &str, details: Option<&str>) {
    let prompts_path = get_prompts_path();
    let log_dir = get_config_dir();
    let _ = fs::create_dir_all(&log_dir);

    // Read existing prompts, keep only last 50 lines
    let existing: Vec<String> = fs::read_to_string(&prompts_path)
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect();

    let skip_count = if existing.len() > 50 { existing.len() - 50 } else { 0 };
    let mut lines: Vec<String> = existing.into_iter().skip(skip_count).collect();

    // Add new prompt
    let timestamp = Utc::now().format("%H:%M:%S").to_string();
    let detail_str = details.unwrap_or("-");
    lines.push(format!("{} | {} | {}", timestamp, tool, detail_str));

    // Write back
    let _ = fs::write(&prompts_path, lines.join("\n") + "\n");
}

/// Debug logging (only when verbose is enabled)
pub fn debug(config: &Config, message: &str) {
    if config.logging.verbose {
        eprintln!("[permission-hook] {}", message);
    }
}

/// Warning logging (always shown)
pub fn warn(message: &str) {
    eprintln!("[permission-hook] WARN: {}", message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello...");
    }
}
