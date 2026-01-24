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
