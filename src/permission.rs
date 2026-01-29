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
// Command Segment Parsing
// ============================================================================

/// Split a command on shell operators (|, &&, ||, ;) and return individual segments
fn split_command_segments(command: &str) -> Vec<String> {
    // Split on pipe, and, or, semicolon - but respect quoted strings
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = command.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                current.push(c);
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                current.push(c);
            }
            '\\' if in_double_quote || in_single_quote => {
                // Escaped character inside quotes - keep both
                current.push(c);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '|' if !in_single_quote && !in_double_quote => {
                // Check for || (logical OR)
                if chars.peek() == Some(&'|') {
                    chars.next(); // consume second |
                }
                // Either way, it's a segment boundary
                let trimmed = strip_redirections(current.trim());
                if !trimmed.is_empty() {
                    segments.push(trimmed);
                }
                current = String::new();
            }
            '&' if !in_single_quote && !in_double_quote => {
                // Check for && (logical AND)
                if chars.peek() == Some(&'&') {
                    chars.next(); // consume second &
                    let trimmed = strip_redirections(current.trim());
                    if !trimmed.is_empty() {
                        segments.push(trimmed);
                    }
                    current = String::new();
                } else {
                    // Single & (background) - just include it
                    current.push(c);
                }
            }
            ';' if !in_single_quote && !in_double_quote => {
                let trimmed = strip_redirections(current.trim());
                if !trimmed.is_empty() {
                    segments.push(trimmed);
                }
                current = String::new();
            }
            _ => {
                current.push(c);
            }
        }
    }

    // Don't forget the last segment
    let trimmed = strip_redirections(current.trim());
    if !trimmed.is_empty() {
        segments.push(trimmed);
    }

    if segments.is_empty() {
        vec![command.to_string()]
    } else {
        segments
    }
}

/// Strip simple redirections from a command segment (NOT heredocs - those are parsed separately)
fn strip_redirections(segment: &str) -> String {
    let segment = segment.trim();

    // Don't strip heredocs - they're handled by parse_heredoc
    // Only strip simple redirections like >, >>, 2>&1

    // Strip 2>&1 style first (before general redirects)
    let segment = if let Ok(redirect2_re) = Regex::new(r"\s*\d*>&\d*") {
        redirect2_re.replace_all(segment, "").to_string()
    } else {
        segment.to_string()
    };

    // Strip output redirections: >, >> with their targets (but NOT << which is heredoc)
    let segment = if let Ok(redirect_re) = Regex::new(r"\s*\d*>>?\s*\S+") {
        redirect_re.replace_all(&segment, "").to_string()
    } else {
        segment
    };

    // Strip input redirection < (single, not <<)
    let segment = if let Ok(redirect_re) = Regex::new(r"\s*<(?!<)\s*\S+") {
        redirect_re.replace_all(&segment, "").to_string()
    } else {
        segment
    };

    segment.trim().to_string()
}

/// Normalize a command by stripping path from the program name
/// "C:\path\to\adb.exe" logcat -c  →  adb logcat -c
/// /usr/bin/python3 script.py  →  python3 script.py
fn normalize_program_path(segment: &str) -> String {
    let segment = segment.trim();

    // Handle quoted path: "C:\path\to\program.exe" args
    if segment.starts_with('"') {
        if let Some(end_quote) = segment[1..].find('"') {
            let quoted_path = &segment[1..end_quote + 1];
            let rest = segment[end_quote + 2..].trim_start();

            // Extract program name from path
            let program = extract_program_name(quoted_path);

            if rest.is_empty() {
                return program;
            } else {
                return format!("{} {}", program, rest);
            }
        }
    }

    // Handle unquoted path with backslash or forward slash
    let first_space = segment.find(' ').unwrap_or(segment.len());
    let first_part = &segment[..first_space];

    if first_part.contains('\\') || first_part.contains('/') {
        let program = extract_program_name(first_part);
        let rest = segment[first_space..].trim_start();

        if rest.is_empty() {
            return program;
        } else {
            return format!("{} {}", program, rest);
        }
    }

    segment.to_string()
}

/// Extract program name from a path, stripping .exe extension
fn extract_program_name(path: &str) -> String {
    // Get the last component of the path
    let name = path
        .rsplit(|c| c == '\\' || c == '/')
        .next()
        .unwrap_or(path);

    // Strip .exe extension (case-insensitive)
    if name.to_lowercase().ends_with(".exe") {
        name[..name.len() - 4].to_string()
    } else {
        name.to_string()
    }
}

/// Check if a single command segment matches any of the patterns
fn segment_matches_patterns(segment: &str, patterns: &[String]) -> bool {
    // Normalize the segment first (strip paths)
    let normalized = normalize_program_path(segment);

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(&normalized) {
                return true;
            }
        }
    }
    false
}

// ============================================================================
// Inline Script Parsing
// ============================================================================

#[derive(Debug)]
pub struct InlineScript {
    pub script_type: String,
    pub content: String,
}

/// Parse heredoc syntax: python << 'EOF' ... EOF
fn parse_heredoc(command: &str) -> Option<InlineScript> {
    // Match: python/python3/node << 'DELIMITER' or <<DELIMITER or <<"DELIMITER"
    let heredoc_start = Regex::new(r#"(?s)^(python3?|node)\s*<<\s*['"]?(\w+)['"]?\s*\n(.*)"#).ok()?;

    if let Some(caps) = heredoc_start.captures(command) {
        let interpreter = caps.get(1)?.as_str();
        let delimiter = caps.get(2)?.as_str();
        let rest = caps.get(3)?.as_str();

        // Find the closing delimiter (must be on its own line)
        let end_pattern = format!(r"(?m)^{}\s*$", regex::escape(delimiter));
        if let Ok(end_re) = Regex::new(&end_pattern) {
            if let Some(end_match) = end_re.find(rest) {
                let content = &rest[..end_match.start()];
                let script_type = match interpreter {
                    "node" => "node",
                    _ => "python",
                };
                return Some(InlineScript {
                    script_type: script_type.into(),
                    content: content.trim().into(),
                });
            }
        }
    }

    None
}

pub fn parse_inline_script(command: &str) -> Option<InlineScript> {
    // Note: cd prefixes are now stripped by split_command_segments before this is called

    // Try heredoc parsing first (python << 'EOF' ... EOF)
    if let Some(script) = parse_heredoc(command) {
        return Some(script);
    }

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

            // Split into segments and check each one
            let segments = split_command_segments(command);

            // All segments must be approved
            let mut all_approved = true;
            let mut approval_reason = String::new();

            for segment in &segments {
                let segment = segment.trim();
                if segment.is_empty() || segment == "cd" || segment.starts_with("cd ") {
                    // cd is always safe, skip it
                    continue;
                }

                let mut segment_approved = false;

                // Check against safe patterns
                if segment_matches_patterns(segment, &config.auto_approve.bash_patterns) {
                    segment_approved = true;
                    if approval_reason.is_empty() {
                        approval_reason = "safe pattern".into();
                    }
                }

                // Check inline scripts (normalize path first)
                if !segment_approved && config.inline_scripts.enabled {
                    let normalized = normalize_program_path(segment);
                    if let Some(script) = parse_inline_script(&normalized) {
                        let (safe, reason) = is_inline_script_safe(config, &script);
                        if safe {
                            segment_approved = true;
                            approval_reason = reason;
                        }
                    }
                }

                if !segment_approved {
                    all_approved = false;
                    break;
                }
            }

            if all_approved && !approval_reason.is_empty() {
                return Some(approval_reason);
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
            // Split into segments and check each one
            let segments = split_command_segments(command);

            // If ANY segment matches dangerous pattern, deny
            for segment in &segments {
                if segment_matches_patterns(segment, &config.auto_deny.bash_patterns) {
                    return Some("dangerous pattern".into());
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

    #[test]
    fn test_parse_heredoc_python() {
        let command = "python << 'PYEOF'\nimport os\nprint('hello')\nPYEOF";
        let script = parse_heredoc(command);
        assert!(script.is_some());
        let script = script.unwrap();
        assert_eq!(script.script_type, "python");
        assert!(script.content.contains("import os"));
        assert!(script.content.contains("print"));
    }

    #[test]
    fn test_parse_heredoc_with_dangerous_content() {
        let config = test_config();
        let command = "python << 'EOF'\nimport os\nos.remove('file.txt')\nEOF";
        let script = parse_heredoc(command);
        assert!(script.is_some());
        let script = script.unwrap();
        let (safe, _reason) = is_inline_script_safe(&config, &script);
        assert!(!safe); // Should detect os.remove as dangerous
    }

    #[test]
    fn test_auto_approve_safe_heredoc() {
        let config = test_config();
        let command = "cd /path && python << 'EOF'\nimport pandas\nprint('hi')\nEOF";
        let input = serde_json::json!({"command": command});
        let result = is_auto_approved(&config, "Bash", &input);
        assert!(result.is_some()); // Should be approved - no dangerous patterns
    }

    #[test]
    fn test_normalize_quoted_windows_path() {
        let segment = r#""C:\Users\test\AppData\Local\adb.exe" logcat -c"#;
        let normalized = normalize_program_path(segment);
        assert_eq!(normalized, "adb logcat -c");
    }

    #[test]
    fn test_normalize_unquoted_windows_path() {
        // Paths with spaces should be quoted; unquoted paths typically don't have spaces
        let segment = r"C:\tools\git\bin\git.exe status";
        let normalized = normalize_program_path(segment);
        assert_eq!(normalized, "git status");
    }

    #[test]
    fn test_normalize_unix_path() {
        let segment = "/usr/bin/python3 script.py";
        let normalized = normalize_program_path(segment);
        assert_eq!(normalized, "python3 script.py");
    }

    #[test]
    fn test_normalize_no_path() {
        let segment = "git status";
        let normalized = normalize_program_path(segment);
        assert_eq!(normalized, "git status");
    }

    #[test]
    fn test_split_segments_simple_pipe() {
        let segments = split_command_segments("ls | head");
        assert_eq!(segments, vec!["ls", "head"]);
    }

    #[test]
    fn test_split_segments_and() {
        let segments = split_command_segments("cd /path && git status");
        assert_eq!(segments, vec!["cd /path", "git status"]);
    }

    #[test]
    fn test_split_segments_pipe_in_double_quotes() {
        // Pipe inside double quotes should NOT split
        let segments = split_command_segments(r#"grep -n "once_cell\|lazy_static" src/*.rs"#);
        assert_eq!(segments.len(), 1);
        assert!(segments[0].contains("once_cell"));
    }

    #[test]
    fn test_split_segments_pipe_in_single_quotes() {
        // Pipe inside single quotes should NOT split
        let segments = split_command_segments("grep -E 'foo|bar' file.txt");
        assert_eq!(segments.len(), 1);
        assert!(segments[0].contains("foo|bar"));
    }

    #[test]
    fn test_split_segments_mixed_quotes_and_pipe() {
        // cd && grep with pattern | head
        let cmd = r#"cd /path && grep -n "a\|b" file.rs | head -20"#;
        let segments = split_command_segments(cmd);
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0], "cd /path");
        assert!(segments[1].contains(r#""a\|b""#));
        assert_eq!(segments[2], "head -20");
    }

    #[test]
    fn test_grep_with_regex_pipe_auto_approved() {
        let config = test_config();
        let cmd = r#"cd "/path" && grep -n "once_cell\|lazy_static" src/*.rs | head -20"#;
        let input = serde_json::json!({"command": cmd});
        let result = is_auto_approved(&config, "Bash", &input);
        assert!(result.is_some(), "grep with regex pipe should be auto-approved");
    }
}
