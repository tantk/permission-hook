# Claude Permission Hook

A fast, Rust-based permission handler and notification system for Claude Code.

**Features:**
- Auto-approve safe operations (reading files, git status, etc.)
- Auto-block dangerous operations (rm -rf, force push, etc.) with alert sound
- Desktop notifications when Claude completes tasks or needs attention
- Inline script scanning (Python, Node, PowerShell, CMD)

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

## Log Format

Logs saved to:
- **Windows:** `%USERPROFILE%\.claude-permission-hook\decisions.log`
- **Linux / macOS:** `~/.claude-permission-hook/decisions.log`

```csv
timestamp,tool,decision,reason,details
2026-01-24T19:22:38,Read,Y,auto-approve tool,test.txt
2026-01-24T19:22:40,Bash,N,dangerous pattern,rm -rf /
2026-01-24T19:22:45,Bash,ASK,prompting user,python script.py
```

**Decision codes:** `Y` = allow, `N` = block, `ASK` = prompt user

## Feature Toggles

Disable features independently:

```json
{
  "features": {
    "permission_checking": false,
    "notifications": true
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

## Development

```bash
# Run tests
cargo test

# Build release
cargo build --release
```

Binary location:
- **Linux / macOS:** `target/release/claude-permission-hook`
- **Windows:** `target\release\claude-permission-hook.exe`

## License

MIT
