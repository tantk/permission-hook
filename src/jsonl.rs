//! JSONL transcript parser for Claude Code transcripts

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// A message in the Claude Code transcript
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub message: MessageContent,
    #[serde(default)]
    pub timestamp: String,
}

/// Content of a message
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageContent {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: Vec<Content>,
}

/// Individual content block (text or tool use)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub input: serde_json::Value,
}

impl Message {
    /// Check if this is a user message
    pub fn is_user(&self) -> bool {
        self.msg_type == "user" || self.message.role == "user"
    }

    /// Check if this is an assistant message
    pub fn is_assistant(&self) -> bool {
        self.msg_type == "assistant" || self.message.role == "assistant"
    }

    /// Get all tool names used in this message
    pub fn get_tools(&self) -> Vec<String> {
        self.message.content
            .iter()
            .filter(|c| c.content_type == "tool_use")
            .map(|c| c.name.clone())
            .collect()
    }

    /// Get all text content from this message
    pub fn get_text(&self) -> String {
        self.message.content
            .iter()
            .filter(|c| c.content_type == "text")
            .map(|c| c.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get tool input by tool name
    pub fn get_tool_input(&self, tool_name: &str) -> Option<&serde_json::Value> {
        self.message.content
            .iter()
            .find(|c| c.content_type == "tool_use" && c.name == tool_name)
            .map(|c| &c.input)
    }
}

/// Parse a JSONL transcript file
pub fn parse_transcript(path: &str) -> Result<Vec<Message>, String> {
    let path = Path::new(path);
    if !path.exists() {
        return Err(format!("Transcript file not found: {}", path.display()));
    }

    let file = File::open(path)
        .map_err(|e| format!("Failed to open transcript: {}", e))?;

    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line: {}", e))?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<Message>(line) {
            Ok(msg) => messages.push(msg),
            Err(_) => {
                // Skip malformed lines - they might be partial or different format
                continue;
            }
        }
    }

    Ok(messages)
}

/// Get assistant messages after the last user message
pub fn get_recent_assistant_messages(messages: &[Message], max_count: usize) -> Vec<&Message> {
    // Find index of last user message
    let last_user_idx = messages.iter()
        .rposition(|m| m.is_user())
        .unwrap_or(0);

    // Get assistant messages after last user message
    messages[last_user_idx..]
        .iter()
        .filter(|m| m.is_assistant())
        .take(max_count)
        .collect()
}

/// Get the last N assistant messages regardless of user messages
pub fn get_last_assistant_messages(messages: &[Message], count: usize) -> Vec<&Message> {
    messages.iter()
        .filter(|m| m.is_assistant())
        .rev()
        .take(count)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_user_message(text: &str) -> Message {
        Message {
            msg_type: "user".into(),
            message: MessageContent {
                role: "user".into(),
                content: vec![Content {
                    content_type: "text".into(),
                    text: text.into(),
                    name: String::new(),
                    input: serde_json::Value::Null,
                }],
            },
            timestamp: "2025-01-01T12:00:00Z".into(),
        }
    }

    fn create_assistant_message(tools: &[&str], text: &str) -> Message {
        let mut content = Vec::new();

        for tool in tools {
            content.push(Content {
                content_type: "tool_use".into(),
                text: String::new(),
                name: tool.to_string(),
                input: serde_json::json!({"file_path": "/test/file.rs"}),
            });
        }

        content.push(Content {
            content_type: "text".into(),
            text: text.into(),
            name: String::new(),
            input: serde_json::Value::Null,
        });

        Message {
            msg_type: "assistant".into(),
            message: MessageContent {
                role: "assistant".into(),
                content,
            },
            timestamp: "2025-01-01T12:00:01Z".into(),
        }
    }

    #[test]
    fn test_message_is_user() {
        let msg = create_user_message("Hello");
        assert!(msg.is_user());
        assert!(!msg.is_assistant());
    }

    #[test]
    fn test_message_is_assistant() {
        let msg = create_assistant_message(&["Read"], "Done");
        assert!(msg.is_assistant());
        assert!(!msg.is_user());
    }

    #[test]
    fn test_get_tools() {
        let msg = create_assistant_message(&["Read", "Write", "Bash"], "Done");
        let tools = msg.get_tools();
        assert_eq!(tools, vec!["Read", "Write", "Bash"]);
    }

    #[test]
    fn test_get_text() {
        let msg = create_assistant_message(&[], "This is the response text");
        assert_eq!(msg.get_text(), "This is the response text");
    }

    #[test]
    fn test_get_recent_assistant_messages() {
        let messages = vec![
            create_user_message("First request"),
            create_assistant_message(&["Read"], "First response"),
            create_user_message("Second request"),
            create_assistant_message(&["Write"], "Second response"),
            create_assistant_message(&["Bash"], "Third response"),
        ];

        let recent = get_recent_assistant_messages(&messages, 10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].get_tools(), vec!["Write"]);
        assert_eq!(recent[1].get_tools(), vec!["Bash"]);
    }

    #[test]
    fn test_get_last_assistant_messages() {
        let messages = vec![
            create_user_message("Request"),
            create_assistant_message(&["Read"], "Response 1"),
            create_assistant_message(&["Write"], "Response 2"),
            create_assistant_message(&["Bash"], "Response 3"),
        ];

        let last = get_last_assistant_messages(&messages, 2);
        assert_eq!(last.len(), 2);
        assert_eq!(last[0].get_tools(), vec!["Write"]);
        assert_eq!(last[1].get_tools(), vec!["Bash"]);
    }
}
