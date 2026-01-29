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
mod summary;
mod notifier;
mod audio;
mod webhook;
mod update;

use config::{load_config, Config};
use permission::{HookInput, HookResponse, is_auto_approved, is_auto_denied, ask_llm, extract_details};
use logging::{log_decision, log_prompt, debug};
use analyzer::{analyze_transcript, get_status_for_pre_tool_use, Status};
use state::Manager as StateManager;
use dedup::Manager as DedupManager;
use notifier::{send_notification, send_alert_notification, should_notify};
use summary::{generate_summary, generate_session_name};
use audio::{play_sound, play_alert_sound};
use webhook::{send_webhook, should_send_webhook, CircuitBreaker, RateLimiter};
use update::{check_for_update, mark_notified};

use std::io::{self, BufRead};

/// Handle PreToolUse hook event (permission decisions)
fn handle_pre_tool_use(config: &Config, input: &HookInput, state_mgr: &StateManager) {
    // Skip permission checking if disabled
    if !config.features.permission_checking {
        debug(config, "Permission checking disabled, passing through");
        std::process::exit(0);
    }

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

        // Send alert notification and sound
        if config.features.notifications {
            let _ = send_alert_notification(config, &tool_name, &reason, details_ref);
            let _ = play_alert_sound(config);
        }

        eprintln!("[permission-hook] DENY: {} - {}", tool_name, reason);
        std::process::exit(2);
    }

    // Trust mode: auto-approve everything that wasn't denied
    if config.features.trust_mode {
        let reason = "trust mode enabled";
        log_decision(config, &tool_name, "allow", reason, details_ref);
        debug(config, &format!("ALLOW (trust mode): {} - {}", tool_name, details_ref.unwrap_or("no details")));

        let response = HookResponse::allow(reason);
        println!("{}", serde_json::to_string(&response).unwrap());
        std::process::exit(0);
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
fn handle_stop(
    config: &Config,
    input: &HookInput,
    state_mgr: &StateManager,
    dedup_mgr: &DedupManager,
    circuit_breaker: &mut CircuitBreaker,
    rate_limiter: &mut RateLimiter,
) {
    // Skip if notifications feature is disabled
    if !config.features.notifications {
        debug(config, "Notifications feature disabled, skipping Stop handler");
        return;
    }

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

    // Generate summary and session name for notifications
    let cwd = input.get_cwd();
    let git_branch = platform::get_git_branch(&cwd);
    let summary = match jsonl::parse_transcript(transcript_path) {
        Ok(messages) => generate_summary(&messages, status),
        Err(_) => String::new(),
    };
    let session_name = generate_session_name(&session_id, &cwd, git_branch.as_deref());

    // Send desktop notification if enabled
    if should_notify(config, status) {
        if let Err(e) = send_notification(
            config,
            status,
            &summary,
            &session_id,
            &cwd,
            git_branch.as_deref(),
        ) {
            logging::warn(&format!("Failed to send notification: {}", e));
        } else {
            debug(config, &format!("Notification sent: {} - {}", status.as_str(), summary));

            // Play notification sound
            if let Err(e) = play_sound(config, status) {
                debug(config, &format!("Sound playback failed: {}", e));
            }
        }
    }

    // Send webhook if enabled
    if should_send_webhook(config, status) {
        if let Err(e) = send_webhook(config, status, &summary, &session_name, circuit_breaker, rate_limiter) {
            logging::warn(&format!("Webhook failed: {}", e));
        } else {
            debug(config, "Webhook sent successfully");
        }
    }

    log_decision(config, "Stop", "notify", status.as_str(), Some(&session_id));

    // Cleanup old locks/state
    let _ = dedup_mgr.cleanup(60);
    let _ = state_mgr.cleanup(60);

    // Check for updates (non-blocking, cached)
    if let Some((current, latest)) = check_for_update(config) {
        let update_msg = format!("Update available: v{} → {}", current, latest);
        debug(config, &update_msg);

        // Send update notification
        if let Err(e) = notifier::send_update_notification(config, &current, &latest) {
            logging::warn(&format!("Failed to send update notification: {}", e));
        } else {
            mark_notified();
        }
    }
}

/// Handle SubagentStop hook event
fn handle_subagent_stop(
    config: &Config,
    input: &HookInput,
    state_mgr: &StateManager,
    dedup_mgr: &DedupManager,
    circuit_breaker: &mut CircuitBreaker,
    rate_limiter: &mut RateLimiter,
) {
    // Skip if notifications feature is disabled
    if !config.features.notifications {
        debug(config, "Notifications feature disabled, skipping SubagentStop handler");
        return;
    }

    if !config.notifications.notify_on_subagent_stop {
        debug(config, "SubagentStop notifications disabled");
        return;
    }

    // Handle same as Stop
    handle_stop(config, input, state_mgr, dedup_mgr, circuit_breaker, rate_limiter);
}

/// Handle Notification hook event (permission prompt)
fn handle_notification(
    config: &Config,
    input: &HookInput,
    state_mgr: &StateManager,
    dedup_mgr: &DedupManager,
    circuit_breaker: &mut CircuitBreaker,
    rate_limiter: &mut RateLimiter,
) {
    // Skip if notifications feature is disabled
    if !config.features.notifications {
        debug(config, "Notifications feature disabled, skipping Notification handler");
        return;
    }

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

    // Generate session name for notifications
    let cwd = input.get_cwd();
    let git_branch = platform::get_git_branch(&cwd);
    let summary = "Permission required";
    let session_name = generate_session_name(&session_id, &cwd, git_branch.as_deref());

    // Send desktop notification if enabled
    if should_notify(config, status) {
        if let Err(e) = send_notification(
            config,
            status,
            summary,
            &session_id,
            &cwd,
            git_branch.as_deref(),
        ) {
            logging::warn(&format!("Failed to send notification: {}", e));
        } else {
            debug(config, "Notification sent: question - Permission required");

            // Play notification sound
            if let Err(e) = play_sound(config, status) {
                debug(config, &format!("Sound playback failed: {}", e));
            }
        }
    }

    // Send webhook if enabled
    if should_send_webhook(config, status) {
        if let Err(e) = send_webhook(config, status, summary, &session_name, circuit_breaker, rate_limiter) {
            logging::warn(&format!("Webhook failed: {}", e));
        } else {
            debug(config, "Webhook sent successfully");
        }
    }

    log_decision(config, "Notification", "notify", "question", Some(&session_id));
}

fn main() {
    let config = load_config();
    let state_mgr = StateManager::new();
    let dedup_mgr = DedupManager::new();

    // Webhook state (fresh each invocation - persistent state would require file-based storage)
    let mut circuit_breaker = CircuitBreaker::default();
    let mut rate_limiter = RateLimiter::default();

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
        "Stop" => handle_stop(&config, &input, &state_mgr, &dedup_mgr, &mut circuit_breaker, &mut rate_limiter),
        "SubagentStop" => handle_subagent_stop(&config, &input, &state_mgr, &dedup_mgr, &mut circuit_breaker, &mut rate_limiter),
        "Notification" => handle_notification(&config, &input, &state_mgr, &dedup_mgr, &mut circuit_breaker, &mut rate_limiter),
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
