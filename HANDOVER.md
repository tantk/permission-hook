# Permission Hook - Complete Handover

## Quick Reference

| Item | Value |
|------|-------|
| **Binary Location** | `C:\Users\tanti\.local\bin\claude-permission-hook.exe` |
| **Config File** | `C:\Users\tanti\.claude-permission-hook\config.json` |
| **Log File** | `C:\Users\tanti\.claude-permission-hook\decisions.log` |
| **Claude Settings** | `C:\Users\tanti\.claude\settings.json` |
| **Source Code** | `C:\dev\plugin\src\` |
| **Language** | Rust |

---

## Features

### 1. Permission Handling
- Auto-approve safe tools (Read, Glob, Grep, Edit, Write, etc.)
- Auto-approve safe bash commands (git, cargo, curl, wget, etc.)
- Auto-deny dangerous commands (rm -rf, git push --force)
- Inline script scanning (Python, Node, PowerShell, CMD)
- Protected path blocking (C:\Windows, C:\Program Files, /etc/)

### 2. Desktop Notifications
- Task Complete - when Claude finishes work
- Question - when Claude needs permission
- Plan Ready - when plan is ready for review
- Session Limit - when limit reached

### 3. Sound & Alerts
- Normal notifications: Windows "Asterisk" sound
- Blocked commands: Windows "Hand" sound (urgent alert)
- Alert notification popup: "BLOCKED - Command denied by security policy"
- Configurable volume

### 4. Webhooks (Optional)
- Slack, Discord, Telegram, Custom JSON
- Retry with exponential backoff
- Circuit breaker and rate limiting

---

## Configuration

### Feature Toggles

```json
{
  "features": {
    "permission_checking": true,
    "notifications": true
  }
}
```

Set to `false` to disable either feature independently.

### Auto-Approve Tools

```json
{
  "auto_approve": {
    "tools": [
      "Read", "Glob", "Grep", "WebFetch", "WebSearch",
      "Task", "TaskList", "TaskGet", "TaskCreate", "TaskUpdate",
      "Edit", "Write"
    ]
  }
}
```

### Auto-Approve Bash Patterns

```json
{
  "auto_approve": {
    "bash_patterns": [
      "^(cd\\s+[^&]+&&\\s*)?git\\s+(status|log|diff|branch|show|remote|fetch)",
      "^(cmd\\s+/c\\s+)?cargo\\s+(build|test|check|clippy|fmt|run)",
      "^curl\\s",
      "^wget\\s",
      "^ls(\\s|$)",
      "^pwd$",
      "^npm\\s+(list|ls|outdated|view|info|search)",
      "^docker\\s+(ps|images|inspect|logs)",
      "^gh\\s+(repo|pr|issue|release|run|workflow)\\s+(view|list|status|diff|checks)"
    ]
  }
}
```

### Inline Script Scanning

Scripts are auto-approved unless they contain dangerous patterns:

| Script Type | Dangerous Patterns |
|-------------|-------------------|
| Python | `os.remove`, `shutil.rmtree`, `subprocess` |
| Node | `child_process`, `fs.unlink`, `rimraf` |
| PowerShell | `Remove-Item`, `Format-Volume`, `Stop-Process` |
| CMD | `del`, `rd`, `rmdir`, `erase`, `format`, `diskpart` |

### Auto-Deny Patterns

```json
{
  "auto_deny": {
    "bash_patterns": [
      "rm\\s+(-rf?|--recursive)?\\s*[/~]",
      "git\\s+push.*--force",
      "git\\s+reset\\s+--hard",
      "curl.*\\|\\s*(ba)?sh"
    ],
    "protected_paths": [
      "^/etc/", "^/usr/", "^/bin/",
      "(?i)^C:\\\\Windows",
      "(?i)^C:\\\\Program Files"
    ]
  }
}
```

---

## Log Format (CSV)

```
timestamp,tool,decision,reason,details
2026-01-24T19:22:38,Read,Y,"Tool ""Read"" is in auto-approve list",test.txt
2026-01-24T19:22:40,Bash,N,Dangerous pattern: rm -rf,rm -rf /
2026-01-24T19:22:45,Bash,ASK,Prompting user,python script.py
```

**Decision Codes:**
- `Y` = allow (auto-approved) - silent
- `N` = deny (blocked) - alert sound + notification
- `ASK` = prompt user - normal sound on task complete

---

## Notification Settings

```json
{
  "notifications": {
    "desktop": {
      "enabled": true,
      "sound": true,
      "volume": 1.0
    },
    "webhook": {
      "enabled": false,
      "preset": "slack",
      "url": ""
    },
    "suppress_question_after_any_notification_seconds": 12
  }
}
```

---

## Project Structure

```
C:\dev\plugin\
├── Cargo.toml
├── README.md
├── HANDOVER.md          # This file
├── src/
│   ├── main.rs          # Entry point, hook routing
│   ├── config.rs        # Configuration loading
│   ├── permission.rs    # Permission logic + inline script scanning
│   ├── logging.rs       # CSV logging
│   ├── analyzer.rs      # Status detection
│   ├── notifier.rs      # Desktop notifications
│   ├── audio.rs         # Sound playback
│   ├── webhook.rs       # HTTP webhooks
│   └── ...
└── target/release/
    └── claude-permission-hook.exe
```

---

## Claude Hook Registration

In `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [{ "matcher": ".*", "hooks": [{ "type": "command", "command": "path\\to\\claude-permission-hook.exe" }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "path\\to\\claude-permission-hook.exe" }] }],
    "SubagentStop": [{ "hooks": [{ "type": "command", "command": "path\\to\\claude-permission-hook.exe" }] }],
    "Notification": [{ "matcher": "permission_prompt", "hooks": [{ "type": "command", "command": "path\\to\\claude-permission-hook.exe" }] }]
  }
}
```

---

## Build & Deploy

```bash
# Build
cargo build --release

# Deploy
copy target\release\claude-permission-hook.exe %USERPROFILE%\.local\bin\
```

---

## Troubleshooting

### No notifications
1. Check `features.notifications: true`
2. Check `notifications.desktop.enabled: true`
3. Enable verbose logging: `"logging": { "verbose": true }`

### Commands not auto-approved
1. Check if tool is in `auto_approve.tools`
2. Check if bash pattern matches `auto_approve.bash_patterns`
3. Check log file for decision reason

### View recent decisions
```bash
type %USERPROFILE%\.claude-permission-hook\decisions.log
```
