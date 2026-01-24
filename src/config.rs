//! Configuration structures and loading for permission-hook

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

// ============================================================================
// Configuration Structures
// ============================================================================

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Config {
    #[serde(default)]
    pub auto_approve: AutoApproveConfig,
    #[serde(default)]
    pub auto_deny: AutoDenyConfig,
    #[serde(default)]
    pub inline_scripts: InlineScriptsConfig,
    #[serde(default)]
    pub ambiguous: AmbiguousConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AmbiguousConfig {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub llm: LlmConfig,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct LlmConfig {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AutoApproveConfig {
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub bash_patterns: Vec<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AutoDenyConfig {
    #[serde(default)]
    pub bash_patterns: Vec<String>,
    #[serde(default)]
    pub protected_paths: Vec<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct InlineScriptsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub dangerous_python_patterns: Vec<String>,
    #[serde(default)]
    pub dangerous_node_patterns: Vec<String>,
    #[serde(default)]
    pub dangerous_powershell_patterns: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub verbose: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self { enabled: true, verbose: false }
    }
}

// ============================================================================
// Notifications Configuration (Phase 1 prep for Phase 2)
// ============================================================================

#[derive(Debug, Deserialize, Default, Clone)]
pub struct NotificationsConfig {
    #[serde(default)]
    pub desktop: DesktopNotificationsConfig,
    #[serde(default)]
    pub webhook: WebhookConfig,
    #[serde(default = "default_cooldown")]
    pub suppress_question_after_task_complete_seconds: i64,
    #[serde(default = "default_cooldown")]
    pub suppress_question_after_any_notification_seconds: i64,
    #[serde(default)]
    pub notify_on_subagent_stop: bool,
    #[serde(default = "default_true")]
    pub notify_on_text_response: bool,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct DesktopNotificationsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sound: bool,
    #[serde(default = "default_volume")]
    pub volume: f32,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct WebhookConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_webhook_preset")]
    pub preset: String,
    #[serde(default)]
    pub telegram_chat_id: Option<String>,
    #[serde(default = "default_true")]
    pub retry_enabled: bool,
    #[serde(default = "default_retry_attempts")]
    pub retry_max_attempts: u32,
}

fn default_true() -> bool { true }
fn default_cooldown() -> i64 { 12 }
fn default_volume() -> f32 { 1.0 }
fn default_webhook_preset() -> String { "custom".to_string() }
fn default_retry_attempts() -> u32 { 3 }

// ============================================================================
// Path Helpers
// ============================================================================

pub fn get_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude-permission-hook")
}

pub fn get_config_path() -> PathBuf {
    get_config_dir().join("config.json")
}

pub fn get_log_path() -> PathBuf {
    get_config_dir().join("decisions.log")
}

pub fn get_prompts_path() -> PathBuf {
    get_config_dir().join("recent_prompts.log")
}

// ============================================================================
// Default Configuration
// ============================================================================

pub fn default_config() -> Config {
    Config {
        auto_approve: AutoApproveConfig {
            tools: vec![
                "Read".into(), "Glob".into(), "Grep".into(),
                "WebFetch".into(), "WebSearch".into(),
                "Task".into(), "TaskList".into(), "TaskGet".into(),
                "TaskCreate".into(), "TaskUpdate".into(),
            ],
            bash_patterns: vec![
                r"^git\s+(status|log|diff|branch|show|remote|fetch)".into(),
                r"^ls(\s|$)".into(),
                r"^pwd$".into(),
                r"^echo\s".into(),
                r"^cat\s".into(),
                r"^head\s".into(),
                r"^tail\s".into(),
                r"^npm\s+(list|ls|outdated|view|info|search)".into(),
                r"^node\s+--version".into(),
                r"^python3?\s+--version".into(),
                r"^pip3?\s+(list|show|search)".into(),
                r"^docker\s+(ps|images|inspect|logs)".into(),
                r"^gh\s+(repo|pr|issue|release|run|workflow)\s+(view|list|status|diff|checks)".into(),
                r"^gh\s+api\s".into(),
                r"^gh\s+auth\s+status".into(),
                r"^(whoami|hostname|date|uname|env)$".into(),
            ],
        },
        auto_deny: AutoDenyConfig {
            bash_patterns: vec![
                r"rm\s+(-rf?|--recursive)?\s*[/~]".into(),
                r"rm\s+-rf?\s+\*".into(),
                r"git\s+push.*--force".into(),
                r"git\s+reset\s+--hard".into(),
                r"curl.*\|\s*(ba)?sh".into(),
                r"wget.*\|\s*(ba)?sh".into(),
                r"sudo\s+rm".into(),
                r"npm\s+publish".into(),
                r"yarn\s+publish".into(),
                r"mkfs\.".into(),
                r"dd\s+.*of=/dev".into(),
                r">\s*/etc/".into(),
                r"chmod\s+(-R\s+)?777\s+/".into(),
            ],
            protected_paths: vec![
                r"^/etc/".into(),
                r"^/usr/".into(),
                r"^/bin/".into(),
                r"^/sbin/".into(),
                r"(?i)^C:\\Windows".into(),
                r"(?i)^C:\\Program Files".into(),
            ],
        },
        inline_scripts: InlineScriptsConfig {
            enabled: true,
            dangerous_python_patterns: vec![
                r"os\.remove".into(),
                r"os\.unlink".into(),
                r"os\.rmdir".into(),
                r"os\.system".into(),
                r"shutil\.rmtree".into(),
                r"subprocess".into(),
            ],
            dangerous_node_patterns: vec![
                r"child_process".into(),
                r"fs\.unlink".into(),
                r"fs\.rmdir".into(),
                r"fs\.rm\(".into(),
                r"rimraf".into(),
            ],
            dangerous_powershell_patterns: vec![
                r"(?i)Remove-Item".into(),
                r"(?i)rm\s+-r".into(),
                r"(?i)del\s+-r".into(),
                r"(?i)Stop-Process".into(),
                r"(?i)Kill".into(),
                r"(?i)Format-Volume".into(),
                r"(?i)Clear-Disk".into(),
                r"(?i)Initialize-Disk".into(),
                r"(?i)Invoke-Expression".into(),
                r"(?i)iex\s".into(),
                r"(?i)Start-Process.*-Verb\s+RunAs".into(),
                r"(?i)Set-ExecutionPolicy".into(),
                r"(?i)Disable-".into(),
                r"(?i)Stop-Service".into(),
                r"(?i)Uninstall-".into(),
            ],
        },
        ambiguous: AmbiguousConfig {
            mode: "ask".into(),
            llm: LlmConfig {
                model: "openai/gpt-4o-mini".into(),
                api_key: "".into(),
                base_url: "https://openrouter.ai/api/v1".into(),
            },
        },
        logging: LoggingConfig { enabled: true, verbose: false },
        notifications: NotificationsConfig::default(),
    }
}

// ============================================================================
// Config Loading
// ============================================================================

pub fn load_config() -> Config {
    let config_path = get_config_path();

    if let Ok(content) = fs::read_to_string(&config_path) {
        if let Ok(config) = serde_json::from_str(&content) {
            return config;
        }
    }

    default_config()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = default_config();
        assert!(config.auto_approve.tools.contains(&"Read".to_string()));
        assert!(config.logging.enabled);
    }

    #[test]
    fn test_notifications_defaults() {
        let config = NotificationsConfig::default();
        assert_eq!(config.suppress_question_after_task_complete_seconds, 0); // default from Default
    }
}
