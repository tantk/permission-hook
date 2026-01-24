# Claude Permission Hook

A fast, open-source auto-permission handler for Claude Code and other AI coding assistants.

**Written in Rust for minimal overhead (~1-5ms per call).**

## Inspiration

This project is inspired by and builds upon ideas from:

- **claudefast** by Jason McGhee - The original fast permission hook concept
- **[claude-code-permissions-hook](https://github.com/kornysietsma/claude-code-permissions-hook)** by Korny Sietsma - TOML-based permission system with audit logging

We've combined the best ideas from both projects and added:
- Inline script scanning (Python, Node.js, PowerShell)
- Built-in dangerous pattern detection
- MCP tool heuristics
- Optional LLM fallback for ambiguous cases

## What It Does

Intercepts permission prompts and automatically:
- **Approves** safe operations (reading files, git status, etc.)
- **Blocks** dangerous operations (rm -rf, force push, etc.)
- **Prompts you** for everything else

## Performance

| Implementation | Startup Time | Per-call Overhead |
|----------------|--------------|-------------------|
| Node.js hooks | ~50-100ms | ~50-100ms |
| **This (Rust)** | **~1-5ms** | **~1-5ms** |

## Installation

### Option 1: Download Pre-built Binary

Download from [Releases](../../releases):
- `claude-permission-hook-windows.exe`
- `claude-permission-hook-linux`
- `claude-permission-hook-macos`
- `claude-permission-hook-macos-arm64`

### Option 2: Build from Source

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/YOUR_USERNAME/claude-permission-hook.git
cd claude-permission-hook
cargo build --release

# Binary at: target/release/claude-permission-hook
```

### Configure Claude Code

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": ".*",
        "command": "/path/to/claude-permission-hook"
      }
    ]
  }
}
```

**Restart Claude Code once** to activate.

## How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│  Claude wants to run: Bash { command: "git status" }            │
└─────────────────────────────┬───────────────────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     TIER 1: Is it safe?                         │
│  • Tool in safe list? (Read, Glob, Grep)                       │
│  • Command matches safe pattern? (git status, ls)              │
│  • Inline script passes safety check?                          │
│  ✓ YES → Auto-approve                                          │
└─────────────────────────────┬───────────────────────────────────┘
                              │ NO
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   TIER 2: Is it dangerous?                      │
│  • Command matches dangerous pattern? (rm -rf, sudo rm)        │
│  • File path protected? (/etc/, C:\Windows)                    │
│  ✓ YES → Auto-block                                            │
└─────────────────────────────┬───────────────────────────────────┘
                              │ NO
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   TIER 3: Ambiguous                             │
│  • mode: "ask" → Prompt user (default)                         │
│  • mode: "llm" → Ask GPT-4o-mini (optional)                    │
└─────────────────────────────────────────────────────────────────┘
```

## What Gets Auto-Approved

### Tools
`Read`, `Glob`, `Grep`, `WebFetch`, `WebSearch`, `TaskList`, `TaskGet`

### Bash Commands
- `git status/log/diff/branch/show/remote/fetch`
- `ls`, `pwd`, `cat`, `head`, `tail`
- `npm list/outdated/view`, `node --version`
- `python --version`, `pip list`
- `docker ps/images/logs`

### Inline Scripts
Python/Node scripts auto-approved **unless** they contain:
- Python: `os.remove`, `os.system`, `shutil.rmtree`, `subprocess`
- Node: `child_process`, `fs.unlink`, `fs.rmdir`

### MCP Tools
Auto-approved if name contains: `get`, `list`, `read`, `fetch`, `search`, `view`

## What Gets Auto-Blocked

### Dangerous Commands
- `rm -rf /`, `rm -rf ~`, `rm -rf *`
- `git push --force`, `git reset --hard`
- `curl ... | sh`, `wget ... | sh`
- `sudo rm`, `npm publish`, `mkfs`, `dd of=/dev`

### Protected Paths
`/etc/`, `/usr/`, `/bin/`, `C:\Windows`, `C:\Program Files`

### MCP Tools
Blocked if name contains: `delete`, `remove`, `destroy`, `wipe`, `purge`

## Configuration

Config file: `~/.claude-permission-hook/config.json`

```json
{
  "auto_approve": {
    "tools": ["Read", "Glob", "Grep"],
    "bash_patterns": ["^git\\s+(status|log|diff)", "^ls(\\s|$)"]
  },
  "auto_deny": {
    "bash_patterns": ["rm\\s+-rf", "git\\s+push.*--force"],
    "protected_paths": ["^/etc/"]
  },
  "inline_scripts": {
    "enabled": true,
    "dangerous_python_patterns": ["os\\.remove", "subprocess"],
    "dangerous_node_patterns": ["child_process"]
  },
  "ambiguous": {
    "mode": "ask"
  },
  "logging": {
    "enabled": true
  }
}
```

**Config changes take effect immediately** (no restart needed).

## Logging

All decisions logged to `~/.claude-permission-hook/decisions.log`:

```json
{"timestamp":"2025-01-24T10:30:45Z","tool":"Bash","decision":"allow","reason":"...","details":"git status"}
{"timestamp":"2025-01-24T10:30:48Z","tool":"Bash","decision":"deny","reason":"...","details":"rm -rf /"}
{"timestamp":"2025-01-24T10:30:52Z","tool":"Bash","decision":"prompt","reason":"...","details":"npm test"}
```

## Optional: LLM for Tier 3

Instead of prompting, use an LLM to decide:

```json
{
  "ambiguous": {
    "mode": "llm",
    "llm": {
      "model": "openai/gpt-4o-mini",
      "api_key": "sk-or-v1-your-key",
      "base_url": "https://openrouter.ai/api/v1"
    }
  }
}
```

Cost: ~$0.0002 per decision (~$1 per 5,000 decisions)

## Compatibility

Works with:
- **Claude Code** (primary target)
- Any tool using stdin/stdout JSON hooks
- Potentially: Codex CLI, Gemini CLI, etc.

## License

MIT - Free to use, modify, and distribute.

## Acknowledgments

Special thanks to the projects that inspired this work:

- claudefast - Jason McGhee's fast permission hook that proved Rust is the way to go
- [claude-code-permissions-hook](https://github.com/kornysietsma/claude-code-permissions-hook) - Korny Sietsma's well-structured permission system with excellent audit logging

