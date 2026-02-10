#!/usr/bin/env bash
set -euo pipefail

REPO="tantk/permission-hook"
BINARY_NAME="claude-permission-hook"
INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.claude-permission-hook"
CLAUDE_SETTINGS="$HOME/.claude/settings.json"

info()  { echo "[INFO]  $*"; }
error() { echo "[ERROR] $*" >&2; exit 1; }

# Detect OS
detect_os() {
  case "$(uname -s)" in
    Linux*)  echo "linux" ;;
    Darwin*) echo "macos" ;;
    *)       error "Unsupported OS: $(uname -s). Use the Windows installer or build from source." ;;
  esac
}

# Try to download pre-built binary from GitHub releases
download_binary() {
  local os="$1"
  local asset_name="$BINARY_NAME"

  info "Checking for latest release..."
  local latest_url
  latest_url="https://github.com/$REPO/releases/latest/download/$asset_name"

  local tmp_file
  tmp_file="$(mktemp)"

  if curl -fsSL -o "$tmp_file" "$latest_url" 2>/dev/null; then
    chmod +x "$tmp_file"
    # Verify it's actually an executable, not an HTML error page
    if file "$tmp_file" | grep -qiE "ELF|Mach-O"; then
      mkdir -p "$INSTALL_DIR"
      mv "$tmp_file" "$INSTALL_DIR/$BINARY_NAME"
      info "Downloaded pre-built binary to $INSTALL_DIR/$BINARY_NAME"
      return 0
    fi
  fi

  rm -f "$tmp_file"
  return 1
}

# Build from source
build_from_source() {
  info "No pre-built binary available. Building from source..."

  if ! command -v cargo &>/dev/null; then
    info "Rust not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
  fi

  local tmp_dir
  tmp_dir="$(mktemp -d)"
  trap "rm -rf '$tmp_dir'" EXIT

  info "Cloning repository..."
  git clone --depth 1 "https://github.com/$REPO.git" "$tmp_dir"

  info "Building release binary..."
  cargo build --release --manifest-path "$tmp_dir/Cargo.toml"

  mkdir -p "$INSTALL_DIR"
  cp "$tmp_dir/target/release/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
  chmod +x "$INSTALL_DIR/$BINARY_NAME"
  info "Built and installed to $INSTALL_DIR/$BINARY_NAME"
}

# Install the default config if none exists
install_config() {
  if [ -f "$CONFIG_DIR/config.json" ]; then
    info "Config already exists at $CONFIG_DIR/config.json, skipping."
    return
  fi

  mkdir -p "$CONFIG_DIR"
  info "Creating default config at $CONFIG_DIR/config.json"
  cat > "$CONFIG_DIR/config.json" << 'CONFIGEOF'
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
      "^/etc/",
      "^/usr/",
      "^/bin/"
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
CONFIGEOF
}

# Configure Claude Code hooks
configure_hooks() {
  local hook_cmd="$INSTALL_DIR/$BINARY_NAME"

  if [ -f "$CLAUDE_SETTINGS" ]; then
    if grep -q "claude-permission-hook" "$CLAUDE_SETTINGS" 2>/dev/null; then
      info "Hooks already configured in $CLAUDE_SETTINGS, skipping."
      return
    fi
    info "Claude settings file exists. Please add hooks manually (see hooks.example.json)."
    info "  Hook command: $hook_cmd"
    return
  fi

  # Create settings with hooks
  mkdir -p "$(dirname "$CLAUDE_SETTINGS")"
  info "Creating $CLAUDE_SETTINGS with hooks configured"
  cat > "$CLAUDE_SETTINGS" << HOOKSEOF
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": ".*",
        "hooks": [{ "type": "command", "command": "$hook_cmd" }]
      }
    ],
    "Stop": [
      {
        "hooks": [{ "type": "command", "command": "$hook_cmd" }]
      }
    ],
    "SubagentStop": [
      {
        "hooks": [{ "type": "command", "command": "$hook_cmd" }]
      }
    ],
    "Notification": [
      {
        "matcher": "permission_prompt",
        "hooks": [{ "type": "command", "command": "$hook_cmd" }]
      }
    ]
  }
}
HOOKSEOF
}

main() {
  info "Installing Claude Permission Hook..."

  local os
  os="$(detect_os)"
  info "Detected OS: $os"

  # Try download first, fall back to build
  if ! download_binary "$os"; then
    build_from_source
  fi

  install_config
  configure_hooks

  info ""
  info "Installation complete!"
  info "  Binary:  $INSTALL_DIR/$BINARY_NAME"
  info "  Config:  $CONFIG_DIR/config.json"
  info "  Hooks:   $CLAUDE_SETTINGS"
  info ""
  info "Restart Claude Code to activate."
}

main "$@"
