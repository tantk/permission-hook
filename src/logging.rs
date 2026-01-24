//! Logging utilities for permission-hook

use crate::config::{get_config_dir, get_log_path, get_prompts_path, Config};
use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::Write;

const CSV_HEADER: &str = "timestamp,tool,decision,reason,details";

/// Truncate string to max length (UTF-8 safe)
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // Find a valid UTF-8 char boundary at or before max_len
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

/// Escape CSV field (wrap in quotes if contains comma, quote, or newline)
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Convert decision to short code
fn decision_code(decision: &str) -> &str {
    match decision {
        "allow" => "Y",
        "deny" => "N",
        "prompt" => "ASK",
        _ => decision,
    }
}

/// Log a permission decision
pub fn log_decision(config: &Config, tool: &str, decision: &str, reason: &str, details: Option<&str>) {
    if !config.logging.enabled {
        return;
    }

    let log_dir = get_config_dir();
    let _ = fs::create_dir_all(&log_dir);

    let log_path = get_log_path();

    // Check if file is empty/new to write header
    let needs_header = !log_path.exists() || fs::metadata(&log_path).map(|m| m.len() == 0).unwrap_or(true);

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        // Write header if new file
        if needs_header {
            let _ = writeln!(file, "{}", CSV_HEADER);
        }

        // Format: timestamp,tool,decision,reason,details
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let line = format!(
            "{},{},{},{},{}",
            timestamp,
            tool,
            decision_code(decision),
            escape_csv(&truncate(reason, 150)),
            escape_csv(&truncate(details.unwrap_or("-"), 100))
        );
        let _ = writeln!(file, "{}", line);
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
