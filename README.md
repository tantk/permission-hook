# Claude Permission Hook

A fast, Rust-based permission handler and notification system for Claude Code.

- **3-tier security**: auto-approve safe ops, auto-block dangerous ones, prompt for everything else
- **Notifications**: desktop popups, custom sounds, webhooks (Slack/Discord/Telegram)
- **Inline script scanning**: Python, Node, PowerShell, CMD
- **Performance**: ~1-5ms per call (vs ~50-100ms for Node.js hooks)

## Quick Start

**Linux / macOS** (one-liner):
```bash
curl -sSL https://raw.githubusercontent.com/tantk/permission-hook/master/install.sh | bash
```

**Windows** (download):
1. Grab `claude-permission-hook.exe` from [Releases](https://github.com/tantk/permission-hook/releases)
2. Place in `%USERPROFILE%\.local\bin\`

**Build from source** (any platform, requires [Rust](https://rustup.rs/) 1.70+):
```bash
git clone https://github.com/tantk/permission-hook.git
cd permission-hook
cargo build --release
```
Binary outputs to `target/release/claude-permission-hook` (or `.exe` on Windows).

## Setup

### 1. Register the Hook

Add to your Claude Code settings file:
- **Windows:** `%USERPROFILE%\.claude\settings.json`
- **Linux / macOS:** `~/.claude/settings.json`

<details>
<summary>Windows config (click to expand)</summary>

```json
{
  "hooks": {
    "PreToolUse": [{ "matcher": ".*", "hooks": [{ "type": "command", "command": "C:\\Users\\YOUR_USERNAME\\.local\\bin\\claude-permission-hook.exe" }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "C:\\Users\\YOUR_USERNAME\\.local\\bin\\claude-permission-hook.exe" }] }],
    "SubagentStop": [{ "hooks": [{ "type": "command", "command": "C:\\Users\\YOUR_USERNAME\\.local\\bin\\claude-permission-hook.exe" }] }],
    "Notification": [{ "matcher": "permission_prompt", "hooks": [{ "type": "command", "command": "C:\\Users\\YOUR_USERNAME\\.local\\bin\\claude-permission-hook.exe" }] }]
  }
}
```
Replace `YOUR_USERNAME` with your Windows username.
</details>

<details>
<summary>Linux / macOS config (click to expand)</summary>

```json
{
  "hooks": {
    "PreToolUse": [{ "matcher": ".*", "hooks": [{ "type": "command", "command": "~/.local/bin/claude-permission-hook" }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "~/.local/bin/claude-permission-hook" }] }],
    "SubagentStop": [{ "hooks": [{ "type": "command", "command": "~/.local/bin/claude-permission-hook" }] }],
    "Notification": [{ "matcher": "permission_prompt", "hooks": [{ "type": "command", "command": "~/.local/bin/claude-permission-hook" }] }]
  }
}
```
Also available as `hooks.example.json` in this repo.
</details>

### 2. Configure the Plugin (Optional)

Create a config file at:
- **Windows:** `%USERPROFILE%\.claude-permission-hook\config.json`
- **Linux / macOS:** `~/.claude-permission-hook/config.json`

See [`config.example.json`](config.example.json) for a full example. The plugin works with sensible defaults if no config file exists.

**Restart Claude Code** to activate.

## How It Works

```
Claude wants to run a command
         |
         v
  TIER 1: Auto-Approve?        -- tool in safe list? safe bash pattern?
    YES -> Allow (silent)
    NO  |
        v
  TIER 2: Auto-Deny?           -- dangerous pattern? protected path?
    YES -> Block + Alert
    NO  |
        v
  TIER 3: Prompt User
```

| Decision | Sound | Notification |
|----------|-------|--------------|
| Allow | Silent | None |
| Block | Alert sound | "BLOCKED" popup |
| Prompt | Normal | On task complete |

## Features

### Permission Rules

**Auto-approve** - configure which tools and bash patterns are always allowed:
```json
{
  "auto_approve": {
    "tools": ["Read", "Glob", "Grep", "Edit", "Write"],
    "bash_patterns": ["^git\\s+(status|log|diff)", "^ls(\\s|$)", "^pwd$"]
  }
}
```

**Auto-deny** - dangerous commands and protected paths are always blocked:
```json
{
  "auto_deny": {
    "bash_patterns": ["rm\\s+-rf?\\s*[/~]", "git\\s+push.*--force"],
    "protected_paths": ["^/etc/", "(?i)^C:\\\\Windows"]
  }
}
```

**Inline script scanning** - scripts are approved unless they contain dangerous patterns:

| Language | Blocked Patterns |
|----------|-----------------|
| Python | `os.remove`, `shutil.rmtree`, `subprocess` |
| Node | `child_process`, `fs.unlink`, `rimraf` |
| PowerShell | `Remove-Item`, `Format-Volume`, `Stop-Process` |
| CMD | `del`, `rd`, `rmdir`, `format`, `diskpart` |

**Trust mode** - auto-approve everything *except* auto-deny patterns (for dev workflows):
```json
{ "features": { "trust_mode": true } }
```

### Notifications

**Desktop notifications** for: task complete, plan ready, permission required, session limit, auth errors.

**Webhook notifications** to Slack, Discord, Telegram, or custom endpoints with retry, circuit breaker, and rate limiting:

```json
{
  "notifications": {
    "webhook": {
      "enabled": true,
      "preset": "slack",
      "url": "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
    }
  }
}
```

| Preset | Format |
|--------|--------|
| `slack` | Attachment with color-coded status |
| `discord` | Embed with color-coded status |
| `telegram` | HTML message (requires `telegram_chat_id`) |
| `custom` | `{ status, title, message, session }` |

**Custom sounds** - place `.wav` or `.mp3` files in `~/.claude-permission-hook/sounds/`:

| File | Trigger |
|------|---------|
| `task-complete.wav` | Task finished |
| `question.wav` | Claude asks a question |
| `plan-ready.wav` | Plan ready for review |
| `alert.wav` | Blocked command |

### Ambiguous Commands

Commands that don't match approve/deny rules can optionally be evaluated by an LLM:

```json
{
  "ambiguous": {
    "mode": "ask",
    "llm": {
      "model": "openai/gpt-4o-mini",
      "api_key": "your-key",
      "base_url": "https://openrouter.ai/api/v1"
    }
  }
}
```

### Auto-Update

Check GitHub for new releases periodically:

```json
{ "updates": { "check_enabled": true, "check_interval_hours": 24 } }
```

## Logging

All permission decisions are logged to CSV:

```csv
timestamp,tool,decision,reason,details
2026-01-24T19:22:38,Read,Y,auto-approve tool,test.txt
2026-01-24T19:22:40,Bash,N,dangerous pattern,rm -rf /
2026-01-24T19:22:45,Bash,ASK,prompting user,python script.py
```

**Decision codes:** `Y` = allow, `N` = block, `ASK` = prompt user

Commands that required user approval are also tracked in a separate prompt log (last 50 entries).

| File | Location (Linux/macOS) | Location (Windows) |
|------|----------------------|-------------------|
| Decision log | `~/.claude-permission-hook/decisions.log` | `%USERPROFILE%\.claude-permission-hook\decisions.log` |
| Prompt log | `~/.claude-permission-hook/recent_prompts.log` | `%USERPROFILE%\.claude-permission-hook\recent_prompts.log` |

Enable verbose debug output to stderr with `"logging": { "verbose": true }`.

## Config Reference

<details>
<summary>Full list of all config options (click to expand)</summary>

| Section | Key | Type | Default | Description |
|---------|-----|------|---------|-------------|
| `features` | `permission_checking` | bool | `true` | Enable permission checking |
| `features` | `notifications` | bool | `true` | Enable all notifications |
| `features` | `trust_mode` | bool | `false` | Auto-approve everything except auto_deny |
| `auto_approve` | `tools` | string[] | `[...]` | Tools to always approve |
| `auto_approve` | `bash_patterns` | string[] | `[...]` | Regex patterns for safe bash commands |
| `auto_deny` | `bash_patterns` | string[] | `[...]` | Regex patterns for dangerous commands |
| `auto_deny` | `protected_paths` | string[] | `[...]` | Path patterns to block |
| `inline_scripts` | `enabled` | bool | `true` | Scan inline scripts |
| `ambiguous` | `mode` | string | `"ask"` | How to handle ambiguous commands |
| `ambiguous.llm` | `model` | string | `""` | LLM model for evaluation |
| `ambiguous.llm` | `api_key` | string | `""` | API key |
| `ambiguous.llm` | `base_url` | string | `""` | API base URL |
| `logging` | `enabled` | bool | `true` | Enable decision logging |
| `logging` | `verbose` | bool | `false` | Debug output to stderr |
| `notifications.desktop` | `enabled` | bool | `false` | Desktop notifications |
| `notifications.desktop` | `sound` | bool | `false` | Notification sounds |
| `notifications.desktop` | `volume` | float | `1.0` | Sound volume (0.0-1.0) |
| `notifications.webhook` | `enabled` | bool | `false` | Webhook notifications |
| `notifications.webhook` | `preset` | string | `"custom"` | `slack`/`discord`/`telegram`/`custom` |
| `notifications.webhook` | `url` | string | `""` | Webhook URL |
| `notifications.webhook` | `telegram_chat_id` | string | `""` | Telegram chat ID |
| `notifications.webhook` | `retry_enabled` | bool | `true` | Retry failed webhooks |
| `notifications.webhook` | `retry_max_attempts` | int | `3` | Max retry attempts |
| `notifications` | `suppress_question_after_task_complete_seconds` | int | `12` | Cooldown after task complete |
| `notifications` | `suppress_question_after_any_notification_seconds` | int | `12` | Cooldown after any notification |
| `notifications` | `notify_on_subagent_stop` | bool | `false` | Notify on subagent finish |
| `notifications` | `notify_on_text_response` | bool | `true` | Notify on text response |
| `updates` | `check_enabled` | bool | `false` | Check for new versions |
| `updates` | `check_interval_hours` | int | `24` | Hours between checks |
| `updates` | `github_repo` | string | `"tantk/permission-hook"` | Repo to check for updates |

</details>

## Troubleshooting

**Commands not auto-approved** - Check `auto_approve.tools` and `bash_patterns`. Enable `"verbose": true` and check `decisions.log`.

**No notifications** - Verify `features.notifications`, `notifications.desktop.enabled` are `true`. On Linux, ensure a notification daemon is running and `notify-send` is installed (`sudo apt install libnotify-bin`).

**No sounds** - Check `notifications.desktop.sound: true`. On Linux, ensure `paplay` or `aplay` is available.

**Webhook issues** - Verify `url` matches your preset format. For Telegram, `telegram_chat_id` is required. Enable verbose logging to see HTTP errors.

## Development

```bash
cargo test                              # Run tests
cargo build --release                   # Build release
cargo build --release --features sound  # Build with custom sound support
```

## License

MIT
