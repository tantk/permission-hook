//! Permission checking logic for auto-approve/deny decisions

use crate::config::Config;
use regex::Regex;
use serde::{Deserialize, Serialize};

// ============================================================================
// Input/Output Structures
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct HookInput {
    #[serde(default)]
    pub hook_event_name: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
    #[serde(default)]
    pub input: Option<serde_json::Value>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

impl HookInput {
    pub fn get_tool_name(&self) -> String {
        self.tool_name.clone()
            .or_else(|| self.tool.clone())
            .unwrap_or_default()
    }

    pub fn get_tool_input(&self) -> serde_json::Value {
        self.tool_input.clone()
            .or_else(|| self.input.clone())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    }

    pub fn get_session_id(&self) -> String {
        self.session_id.clone().unwrap_or_else(|| "unknown".to_string())
    }

    pub fn get_cwd(&self) -> String {
        self.cwd.clone().unwrap_or_default()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookSpecificOutput {
    pub hook_event_name: String,
    pub permission_decision: String,
    pub permission_decision_reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookResponse {
    pub hook_specific_output: HookSpecificOutput,
    pub suppress_output: bool,
}

impl HookResponse {
    pub fn allow(reason: &str) -> Self {
        Self {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PreToolUse".into(),
                permission_decision: "allow".into(),
                permission_decision_reason: reason.into(),
            },
            suppress_output: true,
        }
    }

    pub fn deny(reason: &str) -> Self {
        Self {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PreToolUse".into(),
                permission_decision: "deny".into(),
                permission_decision_reason: reason.into(),
            },
            suppress_output: true,
        }
    }
}

// ============================================================================
// Inline Script Parsing
// ============================================================================

#[derive(Debug)]
pub struct InlineScript {
    pub script_type: String,
    pub content: String,
}

/// Strip common prefixes like "cd path &&" from commands
fn strip_cd_prefix(command: &str) -> &str {
    // Match: cd <path> && <rest>
    if let Some(pos) = command.find("&&") {
        let prefix = &command[..pos];
        if prefix.trim().starts_with("cd ") {
            return command[pos + 2..].trim();
        }
    }
    command
}

pub fn parse_inline_script(command: &str) -> Option<InlineScript> {
    // Strip common prefixes like "cd path &&" before parsing
    let command = strip_cd_prefix(command);

    // Python: python -c "..." or python3 -c "..." (handles multi-line)
    let python_re = Regex::new(r#"(?s)^python3?\s+-c\s+["'](.*)["']"#).ok()?;
    if let Some(caps) = python_re.captures(command) {
        return Some(InlineScript {
            script_type: "python".into(),
            content: caps.get(1)?.as_str().into(),
        });
    }

    // Python with multi-line content (quotes may span lines)
    let python_re2 = Regex::new(r#"(?s)^python3?\s+-c\s+["']?(.*)"#).ok()?;
    if let Some(caps) = python_re2.captures(command) {
        return Some(InlineScript {
            script_type: "python".into(),
            content: caps.get(1)?.as_str().into(),
        });
    }

    // Node: node -e "..." (handles multi-line)
    let node_re = Regex::new(r#"(?s)^node\s+-e\s+["'](.*)["']"#).ok()?;
    if let Some(caps) = node_re.captures(command) {
        return Some(InlineScript {
            script_type: "node".into(),
            content: caps.get(1)?.as_str().into(),
        });
    }

    // Node with multi-line content
    let node_re2 = Regex::new(r#"(?s)^node\s+-e\s+["']?(.*)"#).ok()?;
    if let Some(caps) = node_re2.captures(command) {
        return Some(InlineScript {
            script_type: "node".into(),
            content: caps.get(1)?.as_str().into(),
        });
    }

    // PowerShell: powershell -Command "..." (handles multi-line)
    let ps_re = Regex::new(r#"(?si)^powershell(?:\.exe)?\s+(?:-Command|-c)\s+["'](.*)["']"#).ok()?;
    if let Some(caps) = ps_re.captures(command) {
        return Some(InlineScript {
            script_type: "powershell".into(),
            content: caps.get(1)?.as_str().into(),
        });
    }

    // PowerShell with multi-line content
    let ps_re2 = Regex::new(r#"(?si)^powershell(?:\.exe)?\s+(?:-Command|-c)\s+["']?(.*)"#).ok()?;
    if let Some(caps) = ps_re2.captures(command) {
        return Some(InlineScript {
            script_type: "powershell".into(),
            content: caps.get(1)?.as_str().into(),
        });
    }

    // CMD: cmd /c "..." (handles multi-line)
    let cmd_re = Regex::new(r#"(?si)^cmd(?:\.exe)?\s+/c\s+["'](.*)["']"#).ok()?;
    if let Some(caps) = cmd_re.captures(command) {
        return Some(InlineScript {
            script_type: "cmd".into(),
            content: caps.get(1)?.as_str().into(),
        });
    }

    // CMD with multi-line content
    let cmd_re2 = Regex::new(r#"(?si)^cmd(?:\.exe)?\s+/c\s+["']?(.*)"#).ok()?;
    if let Some(caps) = cmd_re2.captures(command) {
        return Some(InlineScript {
            script_type: "cmd".into(),
            content: caps.get(1)?.as_str().into(),
        });
    }

    None
}

pub fn is_inline_script_safe(config: &Config, script: &InlineScript) -> (bool, String) {
    let patterns = match script.script_type.as_str() {
        "python" => &config.inline_scripts.dangerous_python_patterns,
        "node" => &config.inline_scripts.dangerous_node_patterns,
        "powershell" => &config.inline_scripts.dangerous_powershell_patterns,
        "cmd" => &config.inline_scripts.dangerous_cmd_patterns,
        _ => return (false, "Unknown script type".into()),
    };

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(&script.content) {
                return (false, format!("dangerous {}", script.script_type));
            }
        }
    }

    (true, format!("safe {}", script.script_type))
}

// ============================================================================
// Permission Checks
// ============================================================================

/// Check if tool/command should be auto-approved
pub fn is_auto_approved(config: &Config, tool_name: &str, input: &serde_json::Value) -> Option<String> {
    // Check if tool is in auto-approve list
    if config.auto_approve.tools.iter().any(|t| t == tool_name) {
        return Some("auto-approve tool".into());
    }

    // Check Bash commands
    if tool_name == "Bash" {
        if let Some(command) = input.get("command").and_then(|c| c.as_str()) {
            let command = command.trim();

            // Check against safe patterns
            for pattern in &config.auto_approve.bash_patterns {
                if let Ok(re) = Regex::new(pattern) {
                    if re.is_match(command) {
                        return Some("safe pattern".into());
                    }
                }
            }

            // Check inline scripts
            if config.inline_scripts.enabled {
                if let Some(script) = parse_inline_script(command) {
                    let (safe, reason) = is_inline_script_safe(config, &script);
                    if safe {
                        return Some(reason);
                    }
                }
            }
        }
    }

    // Check MCP tools - auto-approve read-only operations
    if tool_name.starts_with("mcp__") {
        let mcp_tool_name = tool_name.split("__").last().unwrap_or("").to_lowercase();
        let safe_patterns = ["get", "list", "read", "fetch", "search", "find", "query", "view", "show", "describe", "inspect", "status", "health"];

        for pattern in safe_patterns {
            if mcp_tool_name.contains(pattern) {
                return Some("read-only MCP".into());
            }
        }
    }

    None
}

/// Check if tool/command should be auto-denied
pub fn is_auto_denied(config: &Config, tool_name: &str, input: &serde_json::Value) -> Option<String> {
    // Check Bash commands against dangerous patterns
    if tool_name == "Bash" {
        if let Some(command) = input.get("command").and_then(|c| c.as_str()) {
            for pattern in &config.auto_deny.bash_patterns {
                if let Ok(re) = Regex::new(pattern) {
                    if re.is_match(command) {
                        return Some("dangerous pattern".into());
                    }
                }
            }
        }
    }

    // Check file operations against protected paths
    if ["Write", "Edit", "NotebookEdit"].contains(&tool_name) {
        let file_path = input.get("file_path")
            .or_else(|| input.get("path"))
            .or_else(|| input.get("notebook_path"))
            .and_then(|p| p.as_str())
            .unwrap_or("");

        for pattern in &config.auto_deny.protected_paths {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(file_path) {
                    return Some("protected path".into());
                }
            }
        }
    }

    // Check MCP tools for destructive operations
    if tool_name.starts_with("mcp__") {
        let mcp_tool_name = tool_name.split("__").last().unwrap_or("").to_lowercase();
        let dangerous_patterns = ["delete", "remove", "destroy", "drop", "clear", "wipe", "purge", "erase", "reset", "truncate"];

        for pattern in dangerous_patterns {
            if mcp_tool_name.contains(pattern) {
                return Some("destructive MCP".into());
            }
        }
    }

    None
}

/// Ask LLM for decision (optional Tier 3)
pub fn ask_llm(config: &Config, tool_name: &str, input: &serde_json::Value) -> Option<(String, String)> {
    if config.ambiguous.mode != "llm" || config.ambiguous.llm.api_key.is_empty() {
        return None;
    }

    let prompt = format!(
        "You are a security analyzer for a coding assistant. Analyze this tool request and decide if it's SAFE or DANGEROUS.\n\n\
        Tool: {}\n\
        Input: {}\n\n\
        Rules:\n\
        - SAFE: Read operations, standard dev commands, file edits in project directories\n\
        - DANGEROUS: System modifications, data deletion, network attacks, credential exposure\n\n\
        Respond with only: SAFE or DANGEROUS",
        tool_name,
        serde_json::to_string_pretty(input).unwrap_or_default()
    );

    let base_url = if config.ambiguous.llm.base_url.is_empty() {
        "https://openrouter.ai/api/v1"
    } else {
        &config.ambiguous.llm.base_url
    };

    let model = if config.ambiguous.llm.model.is_empty() {
        "openai/gpt-4o-mini"
    } else {
        &config.ambiguous.llm.model
    };

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!("{}/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", config.ambiguous.llm.api_key))
        .json(&serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 10
        }))
        .send()
        .ok()?;

    let data: serde_json::Value = response.json().ok()?;
    let answer = data["choices"][0]["message"]["content"]
        .as_str()?
        .trim()
        .to_uppercase();

    if answer == "SAFE" {
        Some(("allow".into(), "LLM determined operation is safe".into()))
    } else if answer == "DANGEROUS" {
        Some(("deny".into(), "LLM determined operation is dangerous".into()))
    } else {
        None
    }
}

/// Extract details for logging from tool input
pub fn extract_details(input: &serde_json::Value) -> Option<String> {
    input.get("command")
        .or_else(|| input.get("file_path"))
        .or_else(|| input.get("pattern"))
        .or_else(|| input.get("url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_config;

    fn test_config() -> Config {
        default_config()
    }

    #[test]
    fn test_auto_approve_read_tool() {
        let config = test_config();
        let input = serde_json::json!({"file_path": "test.txt"});
        let result = is_auto_approved(&config, "Read", &input);
        assert!(result.is_some());
    }

    #[test]
    fn test_auto_approve_git_status() {
        let config = test_config();
        let input = serde_json::json!({"command": "git status"});
        let result = is_auto_approved(&config, "Bash", &input);
        assert!(result.is_some());
    }

    #[test]
    fn test_auto_deny_rm_rf() {
        let config = test_config();
        let input = serde_json::json!({"command": "rm -rf /"});
        let result = is_auto_denied(&config, "Bash", &input);
        assert!(result.is_some());
    }

    #[test]
    fn test_auto_deny_protected_path() {
        let config = test_config();
        let input = serde_json::json!({"file_path": "/etc/passwd"});
        let result = is_auto_denied(&config, "Write", &input);
        assert!(result.is_some());
    }

    #[test]
    fn test_hook_response_allow() {
        let response = HookResponse::allow("Test reason");
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"permissionDecision\":\"allow\""));
    }

    #[test]
    fn test_hook_response_deny() {
        let response = HookResponse::deny("Test reason");
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"permissionDecision\":\"deny\""));
    }
}
