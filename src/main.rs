//! Claude Permission Hook - Fast permission handling with notifications
//!
//! This hook handles:
//! - PreToolUse: Permission decisions (allow/deny/prompt)
//! - Stop: Task completion detection and notifications
//! - SubagentStop: Subagent completion notifications
//! - Notification: Permission prompt notifications

mod config;
mod permission;
mod logging;
mod jsonl;
mod analyzer;
mod state;
mod dedup;
mod platform;

use config::{load_config, Config};
use permission::{HookInput, HookResponse, is_auto_approved, is_auto_denied, ask_llm, extract_details};
use logging::{log_decision, log_prompt, debug};
use analyzer::{analyze_transcript, get_status_for_pre_tool_use, Status};
use state::Manager as StateManager;
use dedup::Manager as DedupManager;

use std::io::{self, BufRead};

/// Handle PreToolUse hook event (permission decisions)
fn handle_pre_tool_use(config: &Config, input: &HookInput, state_mgr: &StateManager) {
    let tool_name = input.get_tool_name();
    let tool_input = input.get_tool_input();
    let details = extract_details(&tool_input);
    let details_ref = details.as_deref();

    // Tier 1: Check auto-approve
    if let Some(reason) = is_auto_approved(config, &tool_name, &tool_input) {
        log_decision(config, &tool_name, "allow", &reason, details_ref);
        debug(config, &format!("ALLOW: {} - {}", tool_name, reason));

        // Output JSON to actually allow the command
        let response = HookResponse::allow(&reason);
        println!("{}", serde_json::to_string(&response).unwrap());
        std::process::exit(0);
    }

    // Tier 2: Check auto-deny
    if let Some(reason) = is_auto_denied(config, &tool_name, &tool_input) {
        log_decision(config, &tool_name, "deny", &reason, details_ref);
        eprintln!("[permission-hook] DENY: {} - {}", tool_name, reason);
        std::process::exit(2);
    }

    // Tier 3: Ambiguous - use LLM if configured, otherwise prompt user
    if let Some((decision_type, reason)) = ask_llm(config, &tool_name, &tool_input) {
        log_decision(config, &tool_name, &decision_type, &reason, details_ref);
        if decision_type == "allow" {
            let response = HookResponse::allow(&reason);
            println!("{}", serde_json::to_string(&response).unwrap());
            std::process::exit(0);
        } else {
            eprintln!("{}", reason);
            std::process::exit(2);
        }
    }

    // Check for interactive tools (ExitPlanMode, AskUserQuestion)
    let status = get_status_for_pre_tool_use(&tool_name);
    if status != Status::Unknown {
        // Update state for interactive tools
        let session_id = input.get_session_id();
        let cwd = input.get_cwd();
        if let Err(e) = state_mgr.update_interactive_tool(&session_id, &tool_name, &cwd) {
            logging::warn(&format!("Failed to update interactive tool state: {}", e));
        }
        debug(config, &format!("Interactive tool: {} -> {:?}", tool_name, status));
    }

    // Fall through to Claude's default behavior (prompt user)
    let prompt_reason = format!("Prompting user for: {} ({})", tool_name, details_ref.unwrap_or("no details"));
    log_decision(config, &tool_name, "prompt", &prompt_reason, details_ref);
    log_prompt(&tool_name, details_ref);
    debug(config, &prompt_reason);

    // Exit 0 with no output = passthrough to Claude's native permissions
    std::process::exit(0);
}

/// Handle Stop hook event (task completion)
fn handle_stop(config: &Config, input: &HookInput, state_mgr: &StateManager, dedup_mgr: &DedupManager) {
    let session_id = input.get_session_id();
    let transcript_path = input.transcript_path.as_deref().unwrap_or("");

    debug(config, &format!("Stop event: session={}, transcript={}", session_id, transcript_path));

    // Phase 1: Early duplicate check
    if dedup_mgr.check_early_duplicate(&session_id, Some("Stop")) {
        debug(config, "Early duplicate detected, skipping");
        return;
    }

    // Analyze transcript if available
    if transcript_path.is_empty() || !platform::file_exists(transcript_path) {
        debug(config, "No transcript available");
        return;
    }

    let status = match analyze_transcript(transcript_path, config) {
        Ok(s) => s,
        Err(e) => {
            logging::warn(&format!("Failed to analyze transcript: {}", e));
            return;
        }
    };

    if status == Status::Unknown {
        debug(config, "Unknown status, skipping notification");
        return;
    }

    // Phase 2: Acquire lock
    match dedup_mgr.acquire_lock(&session_id, Some("Stop")) {
        Ok(true) => {}
        Ok(false) => {
            debug(config, "Failed to acquire lock (duplicate), skipping");
            return;
        }
        Err(e) => {
            logging::warn(&format!("Failed to acquire lock: {}", e));
            return;
        }
    }

    // Update state
    if let Err(e) = state_mgr.update_state(&session_id, status, "", &input.get_cwd()) {
        logging::warn(&format!("Failed to update state: {}", e));
    }

    // Log the status detection
    debug(config, &format!("Detected status: {:?}", status));

    // TODO Phase 2: Send notification here
    // For now, just log it
    log_decision(config, "Stop", "notify", status.as_str(), Some(&session_id));

    // Cleanup old locks/state
    let _ = dedup_mgr.cleanup(60);
    let _ = state_mgr.cleanup(60);
}

/// Handle SubagentStop hook event
fn handle_subagent_stop(config: &Config, input: &HookInput, state_mgr: &StateManager, dedup_mgr: &DedupManager) {
    if !config.notifications.notify_on_subagent_stop {
        debug(config, "SubagentStop notifications disabled");
        return;
    }

    // Handle same as Stop
    handle_stop(config, input, state_mgr, dedup_mgr);
}

/// Handle Notification hook event (permission prompt)
fn handle_notification(config: &Config, input: &HookInput, state_mgr: &StateManager, dedup_mgr: &DedupManager) {
    let session_id = input.get_session_id();

    debug(config, &format!("Notification event: session={}", session_id));

    // Phase 1: Early duplicate check
    if dedup_mgr.check_early_duplicate(&session_id, Some("Notification")) {
        debug(config, "Early duplicate detected, skipping");
        return;
    }

    // Check cooldown - suppress question after recent notification
    match state_mgr.should_suppress_question_after_any(
        &session_id,
        config.notifications.suppress_question_after_any_notification_seconds,
    ) {
        Ok(true) => {
            debug(config, "Question suppressed due to recent notification");
            return;
        }
        Ok(false) => {}
        Err(e) => {
            logging::warn(&format!("Failed to check cooldown: {}", e));
        }
    }

    // Phase 2: Acquire lock
    match dedup_mgr.acquire_lock(&session_id, Some("Notification")) {
        Ok(true) => {}
        Ok(false) => {
            debug(config, "Failed to acquire lock (duplicate), skipping");
            return;
        }
        Err(e) => {
            logging::warn(&format!("Failed to acquire lock: {}", e));
            return;
        }
    }

    let status = Status::Question;

    // Update state
    if let Err(e) = state_mgr.update_last_notification(&session_id, status, "Permission prompt") {
        logging::warn(&format!("Failed to update notification state: {}", e));
    }

    // Log the notification
    debug(config, "Detected status: Question (permission prompt)");

    // TODO Phase 2: Send notification here
    log_decision(config, "Notification", "notify", "question", Some(&session_id));
}

fn main() {
    let config = load_config();
    let state_mgr = StateManager::new();
    let dedup_mgr = DedupManager::new();

    // Read JSON from stdin
    let stdin = io::stdin();
    let input_str: String = stdin.lock().lines()
        .filter_map(|line| line.ok())
        .collect();

    // Strip UTF-8 BOM if present (Windows PowerShell may add this)
    let input_str = input_str.trim_start_matches('\u{feff}').trim();

    // Parse payload
    let input: HookInput = match serde_json::from_str(input_str) {
        Ok(p) => p,
        Err(_) => return, // Invalid input, let Claude handle it
    };

    // Route based on hook event type
    let hook_event = if input.hook_event_name.is_empty() {
        "PreToolUse".to_string() // Default for backward compatibility
    } else {
        input.hook_event_name.clone()
    };

    debug(&config, &format!("Hook event: {}", hook_event));

    match hook_event.as_str() {
        "PreToolUse" => handle_pre_tool_use(&config, &input, &state_mgr),
        "Stop" => handle_stop(&config, &input, &state_mgr, &dedup_mgr),
        "SubagentStop" => handle_subagent_stop(&config, &input, &state_mgr, &dedup_mgr),
        "Notification" => handle_notification(&config, &input, &state_mgr, &dedup_mgr),
        _ => {
            debug(&config, &format!("Unknown hook event: {}", hook_event));
            // Default to PreToolUse behavior
            handle_pre_tool_use(&config, &input, &state_mgr);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use config::default_config;

    #[test]
    fn test_config_loads() {
        let config = default_config();
        assert!(config.auto_approve.tools.contains(&"Read".to_string()));
    }

    #[test]
    fn test_hook_input_parsing() {
        let json = r#"{"tool_name": "Read", "tool_input": {"file_path": "test.txt"}}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.get_tool_name(), "Read");
    }

    #[test]
    fn test_hook_input_with_hook_event() {
        let json = r#"{"hook_event_name": "Stop", "session_id": "abc-123", "transcript_path": "/tmp/transcript.jsonl"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hook_event_name, "Stop");
        assert_eq!(input.get_session_id(), "abc-123");
    }

    #[test]
    fn test_status_detection() {
        assert_eq!(get_status_for_pre_tool_use("ExitPlanMode"), Status::PlanReady);
        assert_eq!(get_status_for_pre_tool_use("AskUserQuestion"), Status::Question);
        assert_eq!(get_status_for_pre_tool_use("Write"), Status::Unknown);
    }
}
