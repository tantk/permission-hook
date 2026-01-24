//! Desktop notification sender

use crate::analyzer::Status;
use crate::config::Config;
use crate::summary::{generate_session_name, get_status_title};
use notify_rust::Notification;

/// Send a desktop notification
pub fn send_notification(
    config: &Config,
    status: Status,
    summary: &str,
    session_id: &str,
    cwd: &str,
    git_branch: Option<&str>,
) -> Result<(), String> {
    if !config.notifications.desktop.enabled {
        return Ok(());
    }

    let title = get_status_title(status);
    let session_name = generate_session_name(session_id, cwd, git_branch);

    // Build notification body
    let body = if summary.is_empty() {
        session_name
    } else {
        format!("{}\n{}", session_name, summary)
    };

    // Send notification
    let result = Notification::new()
        .summary(title)
        .body(&body)
        .appname("Claude Code")
        .timeout(notify_rust::Timeout::Milliseconds(5000))
        .show();

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Failed to send notification: {}", e)),
    }
}

/// Send an alert notification for blocked/denied commands
pub fn send_alert_notification(
    config: &Config,
    tool: &str,
    reason: &str,
    details: Option<&str>,
) -> Result<(), String> {
    if !config.notifications.desktop.enabled {
        return Ok(());
    }

    let title = "BLOCKED";

    // Build prominent alert body
    let detail_str = details.unwrap_or("-");
    let body = format!(
        "Command denied by security policy\n\n{}: {}\nReason: {}",
        tool,
        truncate_detail(detail_str, 60),
        reason
    );

    // Send notification with longer timeout for alerts
    let result = Notification::new()
        .summary(title)
        .body(&body)
        .appname("Claude Code")
        .timeout(notify_rust::Timeout::Milliseconds(8000))
        .show();

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Failed to send alert notification: {}", e)),
    }
}

/// Truncate detail string for display (UTF-8 safe)
fn truncate_detail(s: &str, max_len: usize) -> String {
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

/// Check if notifications should be sent for this status
pub fn should_notify(config: &Config, status: Status) -> bool {
    if !config.notifications.desktop.enabled {
        return false;
    }

    match status {
        Status::TaskComplete | Status::ReviewComplete => true,
        Status::Question => true,
        Status::PlanReady => true,
        Status::SessionLimitReached => true,
        Status::ApiError => true,
        Status::Unknown => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_config;

    #[test]
    fn test_should_notify_enabled() {
        let mut config = default_config();
        config.notifications.desktop.enabled = true;

        assert!(should_notify(&config, Status::TaskComplete));
        assert!(should_notify(&config, Status::Question));
        assert!(should_notify(&config, Status::PlanReady));
        assert!(!should_notify(&config, Status::Unknown));
    }

    #[test]
    fn test_should_notify_disabled() {
        let mut config = default_config();
        config.notifications.desktop.enabled = false;

        assert!(!should_notify(&config, Status::TaskComplete));
        assert!(!should_notify(&config, Status::Question));
    }
}
