# Claude Permission Hook

A fast, Rust-based permission handler and notification system for Claude Code.

**Features:**
- Auto-approve safe operations (reading files, git status, etc.)
- Auto-block dangerous operations (rm -rf, force push, etc.) with alert sound
- Desktop notifications when Claude completes tasks or needs attention
- Webhook notifications (Slack, Discord, Telegram)
- Inline script scanning (Python, Node, PowerShell, CMD)
- Custom notification sounds
- Auto-update checking

**Performance:** ~1-5ms per call (vs ~50-100ms for Node.js hooks)

**Minimum Rust version:** 1.70 (if building from source)

## Quick Install (Linux / macOS)

```bash
curl -sSL https://raw.githubusercontent.com/tantk/permission-hook/master/install.sh | bash
```

This downloads the binary (or builds from source), creates the default config, and configures Claude Code hooks automatically.

## Installation

### Option 1: Download Release (Recommended)

**Windows:**
1. Download `claude-permission-hook.exe` from [Releases](https://github.com/tantk/permission-hook/releases)
2. Place in `%USERPROFILE%\.local\bin\` (create folder if needed)
3. Configure Claude Code (see below)

**Linux / macOS:**
1. Download `claude-permission-hook` from [Releases](https://github.com/tantk/permission-hook/releases)
2. Make it executable and place in `~/.local/bin/`:
   ```bash
   chmod +x claude-permission-hook
   mkdir -p ~/.local/bin
   mv claude-permission-hook ~/.local/bin/
   ```
3. Configure Claude Code (see below)

### Option 2: Build from Source

**Windows (PowerShell):**
```powershell
# Install Rust (if needed)
winget install Rustlang.Rust.MSVC

# Clone and build
git clone https://github.com/tantk/permission-hook.git
cd permission-hook
cargo build --release

# Copy to install location
mkdir $env:USERPROFILE\.local\bin -Force
copy target\release\claude-permission-hook.exe $env:USERPROFILE\.local\bin\
```

**Linux / macOS:**
```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/tantk/permission-hook.git
cd permission-hook
cargo build --release

# Copy to install location
mkdir -p ~/.local/bin
cp target/release/claude-permission-hook ~/.local/bin/
```

## Configuration

### Step 1: Configure Claude Code Hooks

**Windows** - Add to `%USERPROFILE%\.claude\settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": ".*",
        "hooks": [{ "type": "command", "command": "C:\\Users\\YOUR_USERNAME\\.local\\bin\\claude-permission-hook.exe" }]
      }
    ],
    "Stop": [
      {
        "hooks": [{ "type": "command", "command": "C:\\Users\\YOUR_USERNAME\\.local\\bin\\claude-permission-hook.exe" }]
      }
    ],
    "SubagentStop": [
      {
        "hooks": [{ "type": "command", "command": "C:\\Users\\YOUR_USERNAME\\.local\\bin\\claude-permission-hook.exe" }]
      }
    ],
    "Notification": [
      {
        "matcher": "permission_prompt",
        "hooks": [{ "type": "command", "command": "C:\\Users\\YOUR_USERNAME\\.local\\bin\\claude-permission-hook.exe" }]
      }
    ]
  }
}
```

Replace `YOUR_USERNAME` with your Windows username.

**Linux / macOS** - Add to `~/.claude/settings.json` (see also `hooks.example.json`):

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": ".*",
        "hooks": [{ "type": "command", "command": "~/.local/bin/claude-permission-hook" }]
      }
    ],
    "Stop": [
      {
        "hooks": [{ "type": "command", "command": "~/.local/bin/claude-permission-hook" }]
      }
    ],
    "SubagentStop": [
      {
        "hooks": [{ "type": "command", "command": "~/.local/bin/claude-permission-hook" }]
      }
    ],
    "Notification": [
      {
        "matcher": "permission_prompt",
        "hooks": [{ "type": "command", "command": "~/.local/bin/claude-permission-hook" }]
      }
    ]
  }
}
```

### Step 2: Create Plugin Config (Optional)

Create the config file:
- **Windows:** `%USERPROFILE%\.claude-permission-hook\config.json`
- **Linux / macOS:** `~/.claude-permission-hook/config.json`

```json
{
  "features": {
    "permission_checking": true,
    "notifications": true
  },
  "auto_approve": {
    "tools": ["Read", "Glob", "Grep", "WebFetch", "WebSearch", "Task", "TaskList", "TaskGet", "TaskCreate", "TaskUpdate", "Edit", "Write"],
    "bash_patterns": [
      "^(cd\\s+[^&]+&&\\s*)?git\\s+(status|log|diff|branch|show|remote|fetch)",
      "^(cmd\\s+/c\\s+)?cargo\\s+(build|test|check|clippy|fmt|run)",
      "^curl\\s",
      "^wget\\s",
      "^ls(\\s|$)",
      "^pwd$",
      "^npm\\s+(list|ls|outdated|view|info|search)",
      "^docker\\s+(ps|images|inspect|logs)",
      "^gh\\s+(repo|pr|issue|release)\\s+(view|list|status)"
    ]
  },
  "auto_deny": {
    "bash_patterns": [
      "rm\\s+(-rf?|--recursive)?\\s*[/~]",
      "git\\s+push.*--force",
      "git\\s+reset\\s+--hard",
      "curl.*\\|\\s*(ba)?sh"
    ],
    "protected_paths": [
      "(?i)^C:\\\\Windows",
      "(?i)^C:\\\\Program Files"
    ]
  },
  "inline_scripts": {
    "enabled": true,
    "dangerous_python_patterns": ["os\\.remove", "shutil\\.rmtree", "subprocess"],
    "dangerous_node_patterns": ["child_process", "fs\\.unlink", "rimraf"],
    "dangerous_powershell_patterns": ["(?i)Remove-Item", "(?i)Format-Volume", "(?i)Stop-Process"],
    "dangerous_cmd_patterns": ["(?i)\\bdel\\b", "(?i)\\brd\\b", "(?i)\\bformat\\b"]
  },
  "notifications": {
    "desktop": {
      "enabled": true,
      "sound": true,
      "volume": 1.0
    }
  },
  "logging": {
    "enabled": true,
    "verbose": false
  }
}
```

**Restart Claude Code** to activate hooks.

## How It Works

```
Claude wants to run a command
         │
         ▼
┌─────────────────────────────┐
│   TIER 1: Auto-Approve?     │
│   • Tool in safe list?      │
│   • Safe bash pattern?      │
│   • Safe inline script?     │
│   YES → Allow (silent)      │
└──────────────┬──────────────┘
               │ NO
               ▼
┌─────────────────────────────┐
│   TIER 2: Auto-Deny?        │
│   • Dangerous pattern?      │
│   • Protected path?         │
│   YES → Block + Alert       │
└──────────────┬──────────────┘
               │ NO
               ▼
┌─────────────────────────────┐
│   TIER 3: Prompt User       │
└─────────────────────────────┘
```

## Features

### Permission Handling
| Decision | Sound | Notification |
|----------|-------|--------------|
| Allow | Silent | None |
| Block | Alert sound | "BLOCKED" popup |
| Prompt | Normal | On task complete |

### Trust Mode

For development workflows where you want minimal prompts, enable trust mode to auto-approve all commands **except** those matching `auto_deny` patterns:

```json
{
  "features": {
    "trust_mode": true
  }
}
```

Dangerous commands (`rm -rf /`, `git push --force`, etc.) are still blocked even in trust mode.

### Inline Script Scanning

Scripts are auto-approved unless they contain dangerous patterns:

| Type | Dangerous Patterns |
|------|-------------------|
| Python | `os.remove`, `shutil.rmtree`, `subprocess` |
| Node | `child_process`, `fs.unlink`, `rimraf` |
| PowerShell | `Remove-Item`, `Format-Volume`, `Stop-Process` |
| CMD | `del`, `rd`, `rmdir`, `format`, `diskpart` |

### Desktop Notifications
- Task complete
- Plan ready for review
- Permission required
- Session limit reached
- API authentication errors

### Custom Notification Sounds

Place `.wav` or `.mp3` files in the sounds directory to customize notification sounds:
- **Windows:** `%USERPROFILE%\.claude-permission-hook\sounds\`
- **Linux / macOS:** `~/.claude-permission-hook/sounds/`

| File | Trigger |
|------|---------|
| `task-complete.wav` | Task or review finished |
| `question.wav` | Claude asks a question |
| `plan-ready.wav` | Plan ready for review |
| `alert.wav` | Blocked command or error |

Falls back to system sounds if custom files are not found.

### Webhook Notifications

Send notifications to Slack, Discord, Telegram, or any custom endpoint. Includes automatic retry with exponential backoff, circuit breaker (stops after repeated failures), and rate limiting.

```json
{
  "notifications": {
    "webhook": {
      "enabled": true,
      "preset": "slack",
      "url": "https://hooks.slack.com/services/YOUR/WEBHOOK/URL",
      "retry_enabled": true,
      "retry_max_attempts": 3
    }
  }
}
```

Supported presets:

| Preset | Format |
|--------|--------|
| `slack` | Slack attachment with color-coded status |
| `discord` | Discord embed with color-coded status |
| `telegram` | HTML-formatted message (requires `telegram_chat_id`) |
| `custom` | Generic JSON: `{ status, title, message, session }` |

**Telegram example:**
```json
{
  "notifications": {
    "webhook": {
      "enabled": true,
      "preset": "telegram",
      "url": "https://api.telegram.org/bot<YOUR_TOKEN>/sendMessage",
      "telegram_chat_id": "123456789"
    }
  }
}
```

### Ambiguous Command Evaluation

For commands that don't match auto-approve or auto-deny patterns, optionally use an LLM to evaluate whether they're safe:

```json
{
  "ambiguous": {
    "mode": "ask",
    "llm": {
      "model": "openai/gpt-4o-mini",
      "api_key": "your-api-key",
      "base_url": "https://openrouter.ai/api/v1"
    }
  }
}
```

Set `mode` to `"ask"` to always prompt the user, or configure an LLM to auto-evaluate ambiguous commands.

### Auto-Update Checking

Periodically check GitHub for new releases:

```json
{
  "updates": {
    "check_enabled": true,
    "check_interval_hours": 24,
    "github_repo": "tantk/permission-hook"
  }
}
```

When an update is available, you'll see a notification with the current and latest version.

## Logging

### Decision Log

All permission decisions are logged to a CSV file:
- **Windows:** `%USERPROFILE%\.claude-permission-hook\decisions.log`
- **Linux / macOS:** `~/.claude-permission-hook/decisions.log`

```csv
timestamp,tool,decision,reason,details
2026-01-24T19:22:38,Read,Y,auto-approve tool,test.txt
2026-01-24T19:22:40,Bash,N,dangerous pattern,rm -rf /
2026-01-24T19:22:45,Bash,ASK,prompting user,python script.py
```

**Decision codes:** `Y` = allow, `N` = block, `ASK` = prompt user

### Prompt Log

Commands that required user approval are tracked separately for quick review:
- **Windows:** `%USERPROFILE%\.claude-permission-hook\recent_prompts.log`
- **Linux / macOS:** `~/.claude-permission-hook/recent_prompts.log`

```
14:22:45 | Bash | python script.py
14:23:10 | Bash | npm install express
```

Only the last 50 entries are kept.

### Verbose Mode

Enable debug output to `stderr` for troubleshooting:

```json
{
  "logging": {
    "enabled": true,
    "verbose": true
  }
}
```

This logs internal decision-making details like pattern matching, config loading, and hook event processing.

## Feature Toggles

Disable features independently:

```json
{
  "features": {
    "permission_checking": false,
    "notifications": true,
    "trust_mode": false
  }
}
```

## Troubleshooting

### Commands not auto-approved
1. Check if tool is in `auto_approve.tools`
2. Check if pattern matches `auto_approve.bash_patterns`
3. Enable verbose logging: `"logging": { "verbose": true }`
4. Check log:
   - **Windows:** `type %USERPROFILE%\.claude-permission-hook\decisions.log`
   - **Linux / macOS:** `cat ~/.claude-permission-hook/decisions.log`

### No notifications
1. Check `features.notifications: true`
2. Check `notifications.desktop.enabled: true`

### No notifications (Linux)
1. Ensure a notification daemon is running (e.g., `dunst`, `mako`, or your desktop environment's built-in notifications)
2. The `notify-send` command should be available (`sudo apt install libnotify-bin` on Debian/Ubuntu)

### No alert sound on block
1. Check `notifications.desktop.sound: true`
2. Check Windows sound settings
3. On Linux, ensure `paplay` (PulseAudio) or `aplay` (ALSA) is available

### Webhook not working
1. Check `notifications.webhook.enabled: true`
2. Verify the `url` is correct for your preset
3. For Telegram, ensure `telegram_chat_id` is set
4. Enable verbose logging to see HTTP errors

## All Config Options

| Section | Key | Type | Default | Description |
|---------|-----|------|---------|-------------|
| `features` | `permission_checking` | bool | `true` | Enable/disable permission checking |
| `features` | `notifications` | bool | `true` | Enable/disable all notifications |
| `features` | `trust_mode` | bool | `false` | Auto-approve everything except auto_deny |
| `auto_approve` | `tools` | string[] | `["Read", "Glob", ...]` | Tools to always approve |
| `auto_approve` | `bash_patterns` | string[] | `[...]` | Regex patterns for safe bash commands |
| `auto_deny` | `bash_patterns` | string[] | `[...]` | Regex patterns for dangerous commands |
| `auto_deny` | `protected_paths` | string[] | `[...]` | Path patterns to always block writes to |
| `inline_scripts` | `enabled` | bool | `true` | Scan inline scripts for danger |
| `ambiguous` | `mode` | string | `"ask"` | How to handle ambiguous commands |
| `ambiguous.llm` | `model` | string | `""` | LLM model for evaluating commands |
| `ambiguous.llm` | `api_key` | string | `""` | API key for LLM service |
| `ambiguous.llm` | `base_url` | string | `""` | API base URL |
| `logging` | `enabled` | bool | `true` | Enable decision logging |
| `logging` | `verbose` | bool | `false` | Enable debug output to stderr |
| `notifications.desktop` | `enabled` | bool | `false` | Enable desktop notifications |
| `notifications.desktop` | `sound` | bool | `false` | Enable notification sounds |
| `notifications.desktop` | `volume` | float | `1.0` | Sound volume (0.0 - 1.0) |
| `notifications.webhook` | `enabled` | bool | `false` | Enable webhook notifications |
| `notifications.webhook` | `preset` | string | `"custom"` | `slack`, `discord`, `telegram`, or `custom` |
| `notifications.webhook` | `url` | string | `""` | Webhook endpoint URL |
| `notifications.webhook` | `telegram_chat_id` | string | `""` | Telegram chat ID |
| `notifications.webhook` | `retry_enabled` | bool | `true` | Retry failed webhooks |
| `notifications.webhook` | `retry_max_attempts` | int | `3` | Max retry attempts |
| `notifications` | `suppress_question_after_task_complete_seconds` | int | `12` | Cooldown after task complete |
| `notifications` | `suppress_question_after_any_notification_seconds` | int | `12` | Cooldown after any notification |
| `notifications` | `notify_on_subagent_stop` | bool | `false` | Notify when subagents finish |
| `updates` | `check_enabled` | bool | `false` | Check for new versions |
| `updates` | `check_interval_hours` | int | `24` | Hours between update checks |
| `updates` | `github_repo` | string | `"anthropics/claude-code"` | GitHub repo to check |

## Development

```bash
# Run tests
cargo test

# Build release
cargo build --release

# Build with custom sound support
cargo build --release --features sound
```

Binary location:
- **Linux / macOS:** `target/release/claude-permission-hook`
- **Windows:** `target\release\claude-permission-hook.exe`

## License

MIT
