//! Status analyzer - State machine for determining notification status from transcripts

use crate::config::Config;
use crate::jsonl;

/// Notification status types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    TaskComplete,
    ReviewComplete,
    Question,
    PlanReady,
    SessionLimitReached,
    ApiError,
    Unknown,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::TaskComplete => "task_complete",
            Status::ReviewComplete => "review_complete",
            Status::Question => "question",
            Status::PlanReady => "plan_ready",
            Status::SessionLimitReached => "session_limit_reached",
            Status::ApiError => "api_error",
            Status::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// Tool categorization
const ACTIVE_TOOLS: &[&str] = &[
    "Write", "Edit", "Bash", "NotebookEdit", "SlashCommand",
    "KillShell", "Task", "MultiEdit"
];

const READ_LIKE_TOOLS: &[&str] = &[
    "Read", "Grep", "Glob"
];

const PASSIVE_TOOLS: &[&str] = &[
    "WebFetch", "WebSearch", "AskFollowupQuestion"
];

/// Check if a tool is an active tool (makes changes)
fn is_active_tool(tool: &str) -> bool {
    ACTIVE_TOOLS.contains(&tool)
}

/// Check if a tool is a read-like tool
fn is_read_like_tool(tool: &str) -> bool {
    READ_LIKE_TOOLS.contains(&tool)
}

/// Get status for PreToolUse event
pub fn get_status_for_pre_tool_use(tool_name: &str) -> Status {
    match tool_name {
        "ExitPlanMode" => Status::PlanReady,
        "AskUserQuestion" => Status::Question,
        _ => Status::Unknown,
    }
}

/// Check for session limit reached in text
fn check_session_limit(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    text_lower.contains("session limit reached") ||
    text_lower.contains("session limit has been reached")
}

/// Check for API 401 error with login prompt
fn check_api_error(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    text_lower.contains("api error: 401") &&
    (text_lower.contains("run /login") || text_lower.contains("please run /login"))
}

/// Analyze transcript to determine status
pub fn analyze_transcript(transcript_path: &str, config: &Config) -> Result<Status, String> {
    let messages = jsonl::parse_transcript(transcript_path)?;

    if messages.is_empty() {
        return Ok(Status::Unknown);
    }

    // Get recent assistant messages (after last user message, max 15)
    let recent_messages = jsonl::get_recent_assistant_messages(&messages, 15);

    if recent_messages.is_empty() {
        return Ok(Status::Unknown);
    }

    // Priority 1: Check for session limit in last 3 assistant messages
    let last_3: Vec<_> = recent_messages.iter().rev().take(3).collect();
    for msg in &last_3 {
        let text = msg.get_text();
        if check_session_limit(&text) {
            return Ok(Status::SessionLimitReached);
        }
    }

    // Priority 2: Check for API 401 error
    for msg in &last_3 {
        let text = msg.get_text();
        if check_api_error(&text) {
            return Ok(Status::ApiError);
        }
    }

    // Collect all tools from recent messages
    let mut all_tools: Vec<String> = Vec::new();
    let mut total_text_length = 0;

    for msg in &recent_messages {
        all_tools.extend(msg.get_tools());
        total_text_length += msg.get_text().len();
    }

    if all_tools.is_empty() {
        // No tools used - check if we should notify on text response
        let notify_on_text = config.notifications.notify_on_text_response;
        if notify_on_text && total_text_length > 0 {
            return Ok(Status::TaskComplete);
        }
        return Ok(Status::Unknown);
    }

    // Get the last tool used
    let last_tool = all_tools.last().map(|s| s.as_str()).unwrap_or("");

    // Priority 3: ExitPlanMode as last tool
    if last_tool == "ExitPlanMode" {
        return Ok(Status::PlanReady);
    }

    // Priority 4: AskUserQuestion as last tool
    if last_tool == "AskUserQuestion" {
        return Ok(Status::Question);
    }

    // Priority 5: ExitPlanMode exists + tools after it -> task_complete
    if all_tools.contains(&"ExitPlanMode".to_string()) {
        let exit_plan_idx = all_tools.iter().position(|t| t == "ExitPlanMode").unwrap();
        if exit_plan_idx < all_tools.len() - 1 {
            return Ok(Status::TaskComplete);
        }
    }

    // Check for active tools
    let has_active_tool = all_tools.iter().any(|t| is_active_tool(t));

    // Priority 6: Review detection (read-like tools, no active tools, long text)
    if !has_active_tool {
        let has_read_like = all_tools.iter().any(|t| is_read_like_tool(t));
        if has_read_like && total_text_length > 200 {
            return Ok(Status::ReviewComplete);
        }
    }

    // Priority 7: Active tool as last tool
    if is_active_tool(last_tool) {
        return Ok(Status::TaskComplete);
    }

    // Priority 8: Any tool used
    if !all_tools.is_empty() {
        return Ok(Status::TaskComplete);
    }

    Ok(Status::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_transcript(messages: &[(&str, &[&str], &str)]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();

        for (role, tools, text) in messages {
            let mut content = Vec::new();

            for tool in *tools {
                content.push(serde_json::json!({
                    "type": "tool_use",
                    "name": tool,
                    "input": {"file_path": "/test/file.rs"}
                }));
            }

            content.push(serde_json::json!({
                "type": "text",
                "text": text
            }));

            let msg = serde_json::json!({
                "type": role,
                "message": {
                    "role": role,
                    "content": content
                },
                "timestamp": "2025-01-01T12:00:00Z"
            });

            writeln!(file, "{}", serde_json::to_string(&msg).unwrap()).unwrap();
        }

        file
    }

    #[test]
    fn test_status_as_str() {
        assert_eq!(Status::TaskComplete.as_str(), "task_complete");
        assert_eq!(Status::ReviewComplete.as_str(), "review_complete");
        assert_eq!(Status::Question.as_str(), "question");
        assert_eq!(Status::PlanReady.as_str(), "plan_ready");
    }

    #[test]
    fn test_get_status_for_pre_tool_use() {
        assert_eq!(get_status_for_pre_tool_use("ExitPlanMode"), Status::PlanReady);
        assert_eq!(get_status_for_pre_tool_use("AskUserQuestion"), Status::Question);
        assert_eq!(get_status_for_pre_tool_use("Write"), Status::Unknown);
    }

    #[test]
    fn test_is_active_tool() {
        assert!(is_active_tool("Write"));
        assert!(is_active_tool("Bash"));
        assert!(is_active_tool("Edit"));
        assert!(!is_active_tool("Read"));
        assert!(!is_active_tool("Glob"));
    }

    #[test]
    fn test_is_read_like_tool() {
        assert!(is_read_like_tool("Read"));
        assert!(is_read_like_tool("Grep"));
        assert!(is_read_like_tool("Glob"));
        assert!(!is_read_like_tool("Write"));
    }

    #[test]
    fn test_check_session_limit() {
        assert!(check_session_limit("Session limit reached. Please start a new conversation."));
        assert!(check_session_limit("The session limit has been reached"));
        assert!(check_session_limit("SESSION LIMIT REACHED"));
        assert!(!check_session_limit("Everything is fine"));
    }

    #[test]
    fn test_check_api_error() {
        assert!(check_api_error("API Error: 401 - Please run /login"));
        assert!(check_api_error("api error: 401 · please run /login"));
        assert!(!check_api_error("API Error: 500"));
        assert!(!check_api_error("Please run /login")); // Missing 401
    }

    #[test]
    fn test_analyze_task_complete() {
        let file = create_test_transcript(&[
            ("user", &[], "Write a function"),
            ("assistant", &["Write"], "Done! I created the function."),
        ]);

        let config = Config::default();
        let status = analyze_transcript(file.path().to_str().unwrap(), &config).unwrap();
        assert_eq!(status, Status::TaskComplete);
    }

    #[test]
    fn test_analyze_review_complete() {
        let file = create_test_transcript(&[
            ("user", &[], "Review my code"),
            ("assistant", &["Read", "Read", "Grep"], &"a".repeat(250)),
        ]);

        let config = Config::default();
        let status = analyze_transcript(file.path().to_str().unwrap(), &config).unwrap();
        assert_eq!(status, Status::ReviewComplete);
    }

    #[test]
    fn test_analyze_review_short_text_is_task_complete() {
        let file = create_test_transcript(&[
            ("user", &[], "Check the file"),
            ("assistant", &["Read"], "Looks good!"),
        ]);

        let config = Config::default();
        let status = analyze_transcript(file.path().to_str().unwrap(), &config).unwrap();
        assert_eq!(status, Status::TaskComplete); // Short text = not review
    }

    #[test]
    fn test_analyze_plan_ready() {
        let file = create_test_transcript(&[
            ("user", &[], "Plan the feature"),
            ("assistant", &["ExitPlanMode"], "Here's my plan"),
        ]);

        let config = Config::default();
        let status = analyze_transcript(file.path().to_str().unwrap(), &config).unwrap();
        assert_eq!(status, Status::PlanReady);
    }

    #[test]
    fn test_analyze_question() {
        let file = create_test_transcript(&[
            ("user", &[], "Help me"),
            ("assistant", &["AskUserQuestion"], "What would you like?"),
        ]);

        let config = Config::default();
        let status = analyze_transcript(file.path().to_str().unwrap(), &config).unwrap();
        assert_eq!(status, Status::Question);
    }

    #[test]
    fn test_analyze_session_limit() {
        let file = create_test_transcript(&[
            ("user", &[], "Continue"),
            ("assistant", &[], "Session limit reached. Please start a new conversation."),
        ]);

        let config = Config::default();
        let status = analyze_transcript(file.path().to_str().unwrap(), &config).unwrap();
        assert_eq!(status, Status::SessionLimitReached);
    }

    #[test]
    fn test_analyze_api_error() {
        let file = create_test_transcript(&[
            ("user", &[], "Do something"),
            ("assistant", &[], "API Error: 401 - Please run /login to authenticate"),
        ]);

        let config = Config::default();
        let status = analyze_transcript(file.path().to_str().unwrap(), &config).unwrap();
        assert_eq!(status, Status::ApiError);
    }

    #[test]
    fn test_analyze_read_plus_write_is_task_complete() {
        // Has active tool, so not review even with read tools
        let file = create_test_transcript(&[
            ("user", &[], "Fix the issue"),
            ("assistant", &["Read", "Read", "Edit"], &"a".repeat(300)),
        ]);

        let config = Config::default();
        let status = analyze_transcript(file.path().to_str().unwrap(), &config).unwrap();
        assert_eq!(status, Status::TaskComplete);
    }

    #[test]
    fn test_analyze_exit_plan_then_tools() {
        // ExitPlanMode followed by other tools = task_complete
        let file = create_test_transcript(&[
            ("user", &[], "Implement the plan"),
            ("assistant", &["ExitPlanMode", "Write", "Bash"], "Done!"),
        ]);

        let config = Config::default();
        let status = analyze_transcript(file.path().to_str().unwrap(), &config).unwrap();
        assert_eq!(status, Status::TaskComplete);
    }
}
