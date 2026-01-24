//! Webhook notifications with retry and circuit breaker

use crate::analyzer::Status;
use crate::config::Config;
use crate::summary::get_status_title;
use serde::Serialize;
use std::time::{Duration, Instant};

/// Webhook preset types
#[derive(Debug, Clone, PartialEq)]
pub enum WebhookPreset {
    Slack,
    Discord,
    Telegram,
    Custom,
}

impl From<&str> for WebhookPreset {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "slack" => WebhookPreset::Slack,
            "discord" => WebhookPreset::Discord,
            "telegram" => WebhookPreset::Telegram,
            _ => WebhookPreset::Custom,
        }
    }
}

/// Circuit breaker state
#[derive(Debug)]
pub struct CircuitBreaker {
    failure_count: u32,
    last_failure: Option<Instant>,
    is_open: bool,
    threshold: u32,
    recovery_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, recovery_timeout_secs: u64) -> Self {
        Self {
            failure_count: 0,
            last_failure: None,
            is_open: false,
            threshold,
            recovery_timeout: Duration::from_secs(recovery_timeout_secs),
        }
    }

    /// Check if circuit is open (blocking requests)
    pub fn is_open(&mut self) -> bool {
        if !self.is_open {
            return false;
        }

        // Check if recovery timeout has passed
        if let Some(last) = self.last_failure {
            if last.elapsed() >= self.recovery_timeout {
                self.is_open = false;
                self.failure_count = 0;
                return false;
            }
        }

        true
    }

    /// Record a successful request
    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.is_open = false;
    }

    /// Record a failed request
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());

        if self.failure_count >= self.threshold {
            self.is_open = true;
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, 30)
    }
}

/// Rate limiter using token bucket
#[derive(Debug)]
pub struct RateLimiter {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_update: Instant,
}

impl RateLimiter {
    pub fn new(requests_per_minute: f64) -> Self {
        let max_tokens = requests_per_minute;
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate: requests_per_minute / 60.0,
            last_update: Instant::now(),
        }
    }

    /// Try to acquire a token, returns true if allowed
    pub fn try_acquire(&mut self) -> bool {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_update = now;
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(10.0) // 10 requests per minute
    }
}

// ============================================================================
// Payload Formatters
// ============================================================================

#[derive(Debug, Serialize)]
struct SlackAttachment {
    color: String,
    title: String,
    text: String,
    footer: String,
}

#[derive(Debug, Serialize)]
struct SlackPayload {
    attachments: Vec<SlackAttachment>,
}

#[derive(Debug, Serialize)]
struct DiscordEmbed {
    title: String,
    description: String,
    color: u32,
    footer: DiscordFooter,
}

#[derive(Debug, Serialize)]
struct DiscordFooter {
    text: String,
}

#[derive(Debug, Serialize)]
struct DiscordPayload {
    embeds: Vec<DiscordEmbed>,
}

#[derive(Debug, Serialize)]
struct TelegramPayload {
    chat_id: String,
    text: String,
    parse_mode: String,
}

#[derive(Debug, Serialize)]
struct CustomPayload {
    status: String,
    title: String,
    message: String,
    session: String,
}

/// Get color for status (Slack format)
fn get_status_color_slack(status: Status) -> &'static str {
    match status {
        Status::TaskComplete | Status::ReviewComplete => "#36a64f", // green
        Status::Question => "#ff9900", // orange
        Status::PlanReady => "#2196f3", // blue
        Status::SessionLimitReached | Status::ApiError => "#ff0000", // red
        Status::Unknown => "#808080", // gray
    }
}

/// Get color for status (Discord format - decimal)
fn get_status_color_discord(status: Status) -> u32 {
    match status {
        Status::TaskComplete | Status::ReviewComplete => 3582783, // green
        Status::Question => 16750848, // orange
        Status::PlanReady => 2201331, // blue
        Status::SessionLimitReached | Status::ApiError => 16711680, // red
        Status::Unknown => 8421504, // gray
    }
}

/// Format payload for the configured preset
pub fn format_payload(
    preset: &WebhookPreset,
    status: Status,
    summary: &str,
    session_name: &str,
    chat_id: Option<&str>,
) -> Result<String, String> {
    match preset {
        WebhookPreset::Slack => {
            let payload = SlackPayload {
                attachments: vec![SlackAttachment {
                    color: get_status_color_slack(status).to_string(),
                    title: get_status_title(status).to_string(),
                    text: summary.to_string(),
                    footer: session_name.to_string(),
                }],
            };
            serde_json::to_string(&payload)
                .map_err(|e| format!("Failed to serialize Slack payload: {}", e))
        }
        WebhookPreset::Discord => {
            let payload = DiscordPayload {
                embeds: vec![DiscordEmbed {
                    title: get_status_title(status).to_string(),
                    description: summary.to_string(),
                    color: get_status_color_discord(status),
                    footer: DiscordFooter {
                        text: session_name.to_string(),
                    },
                }],
            };
            serde_json::to_string(&payload)
                .map_err(|e| format!("Failed to serialize Discord payload: {}", e))
        }
        WebhookPreset::Telegram => {
            let title = get_status_title(status);
            let text = format!("<b>{}</b>\n{}\n<i>{}</i>", title, summary, session_name);
            let payload = TelegramPayload {
                chat_id: chat_id.unwrap_or("").to_string(),
                text,
                parse_mode: "HTML".to_string(),
            };
            serde_json::to_string(&payload)
                .map_err(|e| format!("Failed to serialize Telegram payload: {}", e))
        }
        WebhookPreset::Custom => {
            let payload = CustomPayload {
                status: status.as_str().to_string(),
                title: get_status_title(status).to_string(),
                message: summary.to_string(),
                session: session_name.to_string(),
            };
            serde_json::to_string(&payload)
                .map_err(|e| format!("Failed to serialize custom payload: {}", e))
        }
    }
}

/// Send webhook with retry logic
pub fn send_webhook(
    config: &Config,
    status: Status,
    summary: &str,
    session_name: &str,
    circuit_breaker: &mut CircuitBreaker,
    rate_limiter: &mut RateLimiter,
) -> Result<(), String> {
    let webhook_config = &config.notifications.webhook;

    if !webhook_config.enabled {
        return Ok(());
    }

    if webhook_config.url.is_empty() {
        return Err("Webhook URL not configured".to_string());
    }

    // Check circuit breaker
    if circuit_breaker.is_open() {
        return Err("Circuit breaker is open".to_string());
    }

    // Check rate limit
    if !rate_limiter.try_acquire() {
        return Err("Rate limit exceeded".to_string());
    }

    let preset = WebhookPreset::from(webhook_config.preset.as_str());
    let chat_id = webhook_config.telegram_chat_id.as_deref();
    let payload = format_payload(&preset, status, summary, session_name, chat_id)?;

    let max_attempts = if webhook_config.retry_enabled {
        webhook_config.retry_max_attempts.max(1)
    } else {
        1
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut last_error = String::new();

    for attempt in 0..max_attempts {
        if attempt > 0 {
            // Exponential backoff: 1s, 2s, 4s, max 10s
            let delay = Duration::from_secs((1 << attempt).min(10));
            std::thread::sleep(delay);
        }

        let result = client
            .post(&webhook_config.url)
            .header("Content-Type", "application/json")
            .body(payload.clone())
            .send();

        match result {
            Ok(response) => {
                if response.status().is_success() {
                    circuit_breaker.record_success();
                    return Ok(());
                }
                last_error = format!("HTTP {}: {}", response.status(), response.status().canonical_reason().unwrap_or("Unknown"));
            }
            Err(e) => {
                last_error = format!("Request failed: {}", e);
            }
        }

        circuit_breaker.record_failure();
    }

    Err(format!("Webhook failed after {} attempts: {}", max_attempts, last_error))
}

/// Check if webhooks should be sent for this status
pub fn should_send_webhook(config: &Config, status: Status) -> bool {
    if !config.notifications.webhook.enabled {
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

    #[test]
    fn test_webhook_preset_from_str() {
        assert_eq!(WebhookPreset::from("slack"), WebhookPreset::Slack);
        assert_eq!(WebhookPreset::from("SLACK"), WebhookPreset::Slack);
        assert_eq!(WebhookPreset::from("discord"), WebhookPreset::Discord);
        assert_eq!(WebhookPreset::from("telegram"), WebhookPreset::Telegram);
        assert_eq!(WebhookPreset::from("custom"), WebhookPreset::Custom);
        assert_eq!(WebhookPreset::from("unknown"), WebhookPreset::Custom);
    }

    #[test]
    fn test_circuit_breaker_initial_state() {
        let mut cb = CircuitBreaker::new(3, 30);
        assert!(!cb.is_open());
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let mut cb = CircuitBreaker::new(3, 30);

        cb.record_failure();
        assert!(!cb.is_open());

        cb.record_failure();
        assert!(!cb.is_open());

        cb.record_failure();
        assert!(cb.is_open());
    }

    #[test]
    fn test_circuit_breaker_success_resets() {
        let mut cb = CircuitBreaker::new(3, 30);

        cb.record_failure();
        cb.record_failure();
        cb.record_success();

        cb.record_failure();
        cb.record_failure();
        assert!(!cb.is_open()); // Still not open because success reset the count
    }

    #[test]
    fn test_rate_limiter_allows_initial() {
        let mut rl = RateLimiter::new(10.0);
        assert!(rl.try_acquire());
    }

    #[test]
    fn test_rate_limiter_exhausts_tokens() {
        let mut rl = RateLimiter::new(3.0);

        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        assert!(!rl.try_acquire()); // Should be exhausted
    }

    #[test]
    fn test_format_payload_slack() {
        let result = format_payload(
            &WebhookPreset::Slack,
            Status::TaskComplete,
            "Test message",
            "test-session",
            None,
        );
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("attachments"));
        assert!(json.contains("Task Complete"));
    }

    #[test]
    fn test_format_payload_discord() {
        let result = format_payload(
            &WebhookPreset::Discord,
            Status::Question,
            "Test message",
            "test-session",
            None,
        );
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("embeds"));
        assert!(json.contains("Question"));
    }

    #[test]
    fn test_format_payload_telegram() {
        let result = format_payload(
            &WebhookPreset::Telegram,
            Status::PlanReady,
            "Test message",
            "test-session",
            Some("123456"),
        );
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("chat_id"));
        assert!(json.contains("123456"));
        assert!(json.contains("Plan Ready"));
    }

    #[test]
    fn test_format_payload_custom() {
        let result = format_payload(
            &WebhookPreset::Custom,
            Status::TaskComplete,
            "Test message",
            "test-session",
            None,
        );
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("\"status\":\"task_complete\""));
    }

    #[test]
    fn test_status_colors() {
        assert_eq!(get_status_color_slack(Status::TaskComplete), "#36a64f");
        assert_eq!(get_status_color_slack(Status::Question), "#ff9900");
        assert_eq!(get_status_color_slack(Status::ApiError), "#ff0000");

        assert_eq!(get_status_color_discord(Status::TaskComplete), 3582783);
        assert_eq!(get_status_color_discord(Status::Question), 16750848);
    }
}
