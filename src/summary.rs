//! Summary generator for notification messages

use crate::analyzer::Status;
use crate::jsonl::{self, Message};
use regex::Regex;

const MAX_SUMMARY_LENGTH: usize = 150;

/// Generate a notification summary from transcript messages
pub fn generate_summary(messages: &[Message], status: Status) -> String {
    let text = get_relevant_text(messages, status);
    let cleaned = clean_markdown(&text);
    truncate_smart(&cleaned, MAX_SUMMARY_LENGTH)
}

/// Get relevant text based on status type
fn get_relevant_text(messages: &[Message], status: Status) -> String {
    let recent = jsonl::get_recent_assistant_messages(messages, 3);

    match status {
        Status::Question => {
            // For questions, look for AskUserQuestion content or last text
            for msg in recent.iter().rev() {
                if let Some(content) = extract_question_content(msg) {
                    return content;
                }
            }
            get_last_text_content(&recent)
        }
        Status::PlanReady => {
            // For plan ready, get the plan summary
            for msg in recent.iter().rev() {
                let text = msg.get_text();
                if !text.is_empty() {
                    return text;
                }
            }
            "Plan is ready for review".to_string()
        }
        Status::TaskComplete | Status::ReviewComplete => {
            // Get the completion message
            get_last_text_content(&recent)
        }
        Status::SessionLimitReached => {
            "Session limit reached - please start a new conversation".to_string()
        }
        Status::ApiError => {
            "API authentication error - please log in again".to_string()
        }
        Status::Unknown => {
            get_last_text_content(&recent)
        }
    }
}

/// Extract question content from AskUserQuestion tool
fn extract_question_content(msg: &Message) -> Option<String> {
    let tools = msg.get_tools();

    for tool in tools {
        if tool == "AskUserQuestion" {
            // Try to get the question from tool input
            if let Some(input) = msg.get_tool_input("AskUserQuestion") {
                if let Some(questions) = input.get("questions").and_then(|q| q.as_array()) {
                    if let Some(first) = questions.first() {
                        if let Some(q) = first.get("question").and_then(|q| q.as_str()) {
                            return Some(q.to_string());
                        }
                    }
                }
            }
        }
    }

    None
}

/// Get last non-empty text content from messages
fn get_last_text_content(messages: &[&Message]) -> String {
    for msg in messages.iter().rev() {
        let text = msg.get_text();
        if !text.is_empty() {
            return text;
        }
    }
    String::new()
}

/// Clean markdown formatting from text
pub fn clean_markdown(text: &str) -> String {
    let mut result = text.to_string();

    // Remove code blocks (```...```)
    if let Ok(re) = Regex::new(r"```[\s\S]*?```") {
        result = re.replace_all(&result, "[code]").to_string();
    }

    // Remove inline code (`...`)
    if let Ok(re) = Regex::new(r"`[^`]+`") {
        result = re.replace_all(&result, "").to_string();
    }

    // Remove links [text](url) -> text
    if let Ok(re) = Regex::new(r"\[([^\]]+)\]\([^)]+\)") {
        result = re.replace_all(&result, "$1").to_string();
    }

    // Remove bold/italic markers
    result = result.replace("**", "");
    result = result.replace("__", "");
    result = result.replace("*", "");
    result = result.replace("_", " ");

    // Remove headers (#, ##, etc)
    if let Ok(re) = Regex::new(r"^#{1,6}\s*") {
        result = re.replace_all(&result, "").to_string();
    }

    // Remove bullet points
    if let Ok(re) = Regex::new(r"^\s*[-*+]\s+") {
        result = re.replace_all(&result, "").to_string();
    }

    // Collapse multiple whitespace/newlines
    if let Ok(re) = Regex::new(r"\s+") {
        result = re.replace_all(&result, " ").to_string();
    }

    result.trim().to_string()
}

/// Truncate text smartly at word boundaries (UTF-8 safe)
pub fn truncate_smart(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }

    // Find a valid UTF-8 char boundary at or before max_len
    let mut end = max_len;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }

    if end == 0 {
        return "...".to_string();
    }

    // Find last space before the boundary
    let truncated = &text[..end];
    if let Some(last_space) = truncated.rfind(' ') {
        if last_space > end / 2 {
            return format!("{}...", &text[..last_space]);
        }
    }

    format!("{}...", truncated)
}

/// Get status-specific title prefix with emoji
pub fn get_status_title(status: Status) -> &'static str {
    match status {
        Status::TaskComplete => "✅ Task Complete",
        Status::ReviewComplete => "📋 Review Complete",
        Status::Question => "❓ Question",
        Status::PlanReady => "📝 Plan Ready",
        Status::SessionLimitReached => "⚠️ Session Limit",
        Status::ApiError => "🔐 Auth Error",
        Status::Unknown => "🔔 Notification",
    }
}

/// Generate session display name from session ID and optional context
pub fn generate_session_name(session_id: &str, cwd: &str, git_branch: Option<&str>) -> String {
    let mut parts = Vec::new();

    // Add git branch if available
    if let Some(branch) = git_branch {
        if !branch.is_empty() {
            parts.push(format!("[{}]", branch));
        }
    }

    // Add folder name from cwd
    if !cwd.is_empty() {
        if let Some(folder) = cwd.split(['/', '\\']).last() {
            if !folder.is_empty() {
                parts.push(folder.to_string());
            }
        }
    }

    // Fallback to truncated session ID
    if parts.is_empty() {
        let short_id = if session_id.len() > 8 {
            &session_id[..8]
        } else {
            session_id
        };
        parts.push(format!("Session {}", short_id));
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_markdown_code_blocks() {
        let input = "Here is code:\n```rust\nfn main() {}\n```\nDone.";
        let result = clean_markdown(input);
        assert!(result.contains("[code]"));
        assert!(!result.contains("```"));
    }

    #[test]
    fn test_clean_markdown_inline_code() {
        let input = "Run `cargo build` to compile";
        let result = clean_markdown(input);
        assert!(!result.contains("`"));
    }

    #[test]
    fn test_clean_markdown_links() {
        let input = "Check [this link](https://example.com) for more";
        let result = clean_markdown(input);
        assert!(result.contains("this link"));
        assert!(!result.contains("https://"));
    }

    #[test]
    fn test_clean_markdown_bold_italic() {
        let input = "This is **bold** and *italic* text";
        let result = clean_markdown(input);
        assert!(!result.contains("**"));
        assert!(!result.contains("*"));
    }

    #[test]
    fn test_truncate_smart_short() {
        let input = "Short text";
        let result = truncate_smart(input, 50);
        assert_eq!(result, "Short text");
    }

    #[test]
    fn test_truncate_smart_at_word() {
        let input = "This is a longer text that needs to be truncated at a word boundary";
        let result = truncate_smart(input, 30);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 33); // 30 + "..."
    }

    #[test]
    fn test_get_status_title() {
        assert!(get_status_title(Status::TaskComplete).contains("Task Complete"));
        assert!(get_status_title(Status::Question).contains("Question"));
        assert!(get_status_title(Status::PlanReady).contains("Plan Ready"));
    }

    #[test]
    fn test_generate_session_name_with_branch() {
        let name = generate_session_name("abc123", "/home/user/project", Some("main"));
        assert!(name.contains("[main]"));
        assert!(name.contains("project"));
    }

    #[test]
    fn test_generate_session_name_no_branch() {
        let name = generate_session_name("abc123", "/home/user/project", None);
        assert!(name.contains("project"));
    }

    #[test]
    fn test_generate_session_name_fallback() {
        let name = generate_session_name("abc123def456", "", None);
        assert!(name.contains("abc123de"));
    }
}
