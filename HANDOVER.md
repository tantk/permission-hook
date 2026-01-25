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
| **Repository** | `github.com:tantk/permission-hook` |

---

## Features

### 1. Permission Handling
- Auto-approve safe tools (Read, Glob, Grep, Edit, Write, etc.)
- Auto-approve safe bash commands (git, cargo, curl, adb, etc.)
- Auto-deny dangerous commands (rm -rf, git push --force)
- Inline script scanning (Python, Node, PowerShell, CMD)
- Heredoc parsing (`python << 'EOF'...EOF`)
- Protected path blocking (C:\Windows, C:\Program Files, /etc/)

### 2. Command Parsing
- **Segment splitting**: Commands split on `|`, `&&`, `||`, `;`
- **Path normalization**: `"C:\path\to\adb.exe" logcat` → `adb logcat`
- **Heredoc extraction**: Content between delimiters scanned for dangerous patterns
- **Redirection stripping**: `2>&1`, `>`, `>>` removed before matching

### 3. Desktop Notifications
- Task Complete - when Claude finishes work
- Question - when Claude needs permission
- Plan Ready - when plan is ready for review
- Session Limit - when limit reached
- BLOCKED - when command denied (alert sound)

### 4. Sound & Alerts
- Normal notifications: Windows "Asterisk" sound
- Blocked commands: Windows "Hand" sound (urgent alert)
- Alert notification popup with reason
- Configurable volume

### 5. Webhooks (Optional)
- Slack, Discord, Telegram, Custom JSON
- Retry with exponential backoff
- Circuit breaker and rate limiting

---

## Command Processing Flow

```
Input: cd /path && "C:\sdk\adb.exe" logcat | grep error

1. Split on operators (&&, |):
   - "cd /path"
   - "\"C:\sdk\adb.exe\" logcat"
   - "grep error"

2. Normalize paths:
   - "cd /path" → skip (cd always safe)
   - "adb logcat" → check patterns
   - "grep error" → check patterns

3. Check each segment:
   - All must match safe patterns → AUTO-APPROVE
   - Any matches dangerous pattern → AUTO-DENY
   - Otherwise → PROMPT USER
```

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
      "^git\\s+(-C\\s+\\S+\\s+)?(status|log|diff|branch|show|remote|fetch|add|stash|tag|push|pull|commit|init|config)",
      "^cargo\\s+(build|test|check|clippy|fmt|run)",
      "^adb\\s+(logcat|devices|shell\\s+(getprop|dumpsys|am\\s+start|pm\\s+list)|version)",
      "^curl\\s",
      "^wget\\s",
      "^ls(\\s|$)",
      "^dir\\s+",
      "^pwd$",
      "^cat\\s",
      "^head\\s",
      "^tail\\s",
      "^grep\\s",
      "^wc\\s",
      "^stat\\s",
      "^tree(\\s|$)",
      "^find\\s",
      "^timeout\\s",
      "^\\.?/?gradlew(\\.bat)?\\s+",
      "^npm\\s+(list|ls|outdated|view|info|search)",
      "^docker\\s+(ps|images|inspect|logs)",
      "^gh\\s+(repo|pr|issue|release|run|workflow)\\s+(view|list|status|diff|checks)"
    ]
  }
}
```

**Note:** Patterns match AFTER path normalization. No need to handle full paths like `"C:\...\adb.exe"` - they become `adb`.

### Inline Script Scanning

Scripts are auto-approved unless they contain dangerous patterns:

| Script Type | Syntax | Dangerous Patterns |
|-------------|--------|-------------------|
| Python | `python -c "..."` or `python << 'EOF'` | `os.remove`, `shutil.rmtree`, `subprocess` |
| Node | `node -e "..."` or `node << 'EOF'` | `child_process`, `fs.unlink`, `rimraf` |
| PowerShell | `powershell -Command "..."` | `Remove-Item`, `Format-Volume`, `Stop-Process` |
| CMD | `cmd /c "..."` | `del`, `rd`, `rmdir`, `erase`, `format`, `diskpart` |

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
2026-01-25T01:22:38,Read,Y,auto-approve tool,test.txt
2026-01-25T01:22:40,Bash,N,dangerous pattern,rm -rf /
2026-01-25T01:22:45,Bash,Y,safe pattern,git status
2026-01-25T01:22:50,Bash,Y,safe python,python << 'EOF'...
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
├── HANDOVER.md              # This file
├── config.example.json      # Example configuration
├── .github/workflows/
│   ├── ci.yml               # CI on push
│   └── release.yml          # Release on tag
├── src/
│   ├── main.rs              # Entry point, hook routing
│   ├── lib.rs               # Library exports
│   ├── config.rs            # Configuration loading
│   ├── permission.rs        # Permission logic + parsing
│   ├── logging.rs           # CSV logging
│   ├── analyzer.rs          # Status detection
│   ├── notifier.rs          # Desktop notifications
│   ├── audio.rs             # Sound playback
│   ├── webhook.rs           # HTTP webhooks
│   ├── summary.rs           # Text summarization
│   ├── jsonl.rs             # Transcript parsing
│   ├── state.rs             # Session state
│   ├── dedup.rs             # Deduplication
│   └── platform.rs          # Platform utilities
└── target/release/
    └── claude-permission-hook.exe
```

---

## Key Source Files

### `src/permission.rs`
- `split_command_segments()` - Split on `|`, `&&`, `||`, `;`
- `strip_redirections()` - Remove `>`, `>>`, `2>&1` (not heredocs)
- `normalize_program_path()` - Strip directory and .exe from paths
- `parse_heredoc()` - Extract content from `<< 'EOF'...EOF`
- `parse_inline_script()` - Detect `-c` and heredoc scripts
- `is_inline_script_safe()` - Check for dangerous patterns
- `is_auto_approved()` - Main approval logic
- `is_auto_denied()` - Main deny logic

### `src/notifier.rs`
- `send_notification()` - Status notifications
- `send_alert_notification()` - BLOCKED alerts

### `src/audio.rs`
- `play_sound()` - Normal notification sound
- `play_alert_sound()` - Urgent alert sound

---

## Claude Hook Registration

In `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [{ "matcher": ".*", "hooks": [{ "type": "command", "command": "C:\\Users\\tanti\\.local\\bin\\claude-permission-hook.exe" }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "C:\\Users\\tanti\\.local\\bin\\claude-permission-hook.exe" }] }],
    "SubagentStop": [{ "hooks": [{ "type": "command", "command": "C:\\Users\\tanti\\.local\\bin\\claude-permission-hook.exe" }] }],
    "Notification": [{ "matcher": "permission_prompt", "hooks": [{ "type": "command", "command": "C:\\Users\\tanti\\.local\\bin\\claude-permission-hook.exe" }] }]
  }
}
```

---

## Build & Deploy

```bash
# Build
cargo build --release

# Run tests
cargo test

# Deploy (Windows)
copy target\release\claude-permission-hook.exe %USERPROFILE%\.local\bin\

# Deploy (Git Bash)
cp target/release/claude-permission-hook.exe ~/.local/bin/
```

**Note:** No restart needed - Claude invokes the exe fresh each time.

---

## GitHub Actions

- **CI** (`ci.yml`): Runs on every push to master - builds and tests
- **Release** (`release.yml`): Runs on tag push (v*) - creates release with Windows exe

To create a release:
```bash
git tag v1.0.2
git push origin v1.0.2
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
3. Remember: paths are normalized, so `^adb\s` matches `"C:\...\adb.exe" logcat`
4. Check log file for decision reason

### Full-path commands still prompting
- Paths are normalized: `"C:\path\to\prog.exe" args` → `prog args`
- Pattern should match the program name without path

### Heredocs not approved
- Check if heredoc content contains dangerous patterns
- Verify delimiter format: `<< 'EOF'`, `<<EOF`, `<< "EOF"`

### View recent decisions
```bash
type %USERPROFILE%\.claude-permission-hook\decisions.log
```

### View verbose output
```bash
# Enable in config
"logging": { "enabled": true, "verbose": true }
```

---

## Recent Changes (v1.0.2)

1. **Command segment parsing** - Split on shell operators before checking
2. **Heredoc parsing** - Scan `python << 'EOF'` content for dangerous patterns
3. **Path normalization** - Strip directories and .exe from commands
4. **New patterns** - grep, wc, stat, tree, find, timeout, adb shell am start
5. **Simplified patterns** - No need for `cd prefix &&` handling (automatic)
