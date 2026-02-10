#!/usr/bin/env bash
set -euo pipefail

REPO="tantk/permission-hook"
BINARY_NAME="claude-permission-hook"
INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.claude-permission-hook"
CLAUDE_SETTINGS="$HOME/.claude/settings.json"

info()  { echo "[INFO]  $*"; }
warn()  { echo "[WARN]  $*" >&2; }
error() { echo "[ERROR] $*" >&2; exit 1; }

# ============================================================================
# Download abstraction (curl / wget)
# ============================================================================

DOWNLOADER=""
detect_downloader() {
  if command -v curl >/dev/null 2>&1; then
    DOWNLOADER="curl"
  elif command -v wget >/dev/null 2>&1; then
    DOWNLOADER="wget"
  else
    error "Either curl or wget is required but neither is installed."
  fi
}

# Download a URL. Usage: download_file <url> [output_file]
# If output_file is omitted, prints to stdout.
download_file() {
  local url="$1"
  local output="${2:-}"

  if [ "$DOWNLOADER" = "curl" ]; then
    if [ -n "$output" ]; then
      curl -fsSL -o "$output" "$url"
    else
      curl -fsSL "$url"
    fi
  elif [ "$DOWNLOADER" = "wget" ]; then
    if [ -n "$output" ]; then
      wget -q -O "$output" "$url"
    else
      wget -q -O - "$url"
    fi
  else
    return 1
  fi
}

# ============================================================================
# Platform detection
# ============================================================================

detect_platform() {
  # Detect OS
  case "$(uname -s)" in
    Linux*)  OS="linux" ;;
    Darwin*) OS="darwin" ;;
    *)       error "Unsupported OS: $(uname -s). Windows users: download the .exe from GitHub Releases." ;;
  esac

  # Detect architecture
  case "$(uname -m)" in
    x86_64|amd64)  ARCH="x64" ;;
    arm64|aarch64) ARCH="arm64" ;;
    *)             error "Unsupported architecture: $(uname -m)" ;;
  esac

  # Rosetta 2 detection: if running x64 shell under Rosetta on an ARM Mac,
  # prefer the native arm64 binary
  if [ "$OS" = "darwin" ] && [ "$ARCH" = "x64" ]; then
    if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = "1" ]; then
      info "Rosetta 2 detected — using native arm64 binary."
      ARCH="arm64"
    fi
  fi

  PLATFORM="${OS}-${ARCH}"
  info "Detected platform: $PLATFORM"
}

# ============================================================================
# Checksum verification
# ============================================================================

verify_checksum() {
  local file="$1"
  local expected="$2"

  local actual
  if command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$file" | cut -d' ' -f1)
  elif command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$file" | cut -d' ' -f1)
  else
    warn "Neither shasum nor sha256sum found — skipping checksum verification."
    return 0
  fi

  if [ "$actual" != "$expected" ]; then
    error "Checksum verification failed. Expected: $expected  Got: $actual"
  fi

  info "Checksum verified."
}

# ============================================================================
# Binary download
# ============================================================================

download_binary() {
  local base_url="https://github.com/$REPO/releases/latest/download"
  local asset_name="${BINARY_NAME}-${PLATFORM}"
  local asset_url="${base_url}/${asset_name}"

  TMP_FILE="$(mktemp)"
  # Clean up temp file on any exit
  trap 'rm -f "$TMP_FILE"' EXIT

  info "Downloading $asset_name ..."
  if ! download_file "$asset_url" "$TMP_FILE"; then
    warn "Download failed for $asset_name."
    return 1
  fi

  # Verify it's an actual binary, not an HTML error page
  if command -v file >/dev/null 2>&1; then
    if ! file "$TMP_FILE" | grep -qiE "ELF|Mach-O"; then
      warn "Downloaded file is not a valid executable."
      return 1
    fi
  fi

  # Checksum verification (best-effort: skip if checksums.txt is unavailable)
  local checksums
  if checksums=$(download_file "${base_url}/checksums.txt" "" 2>/dev/null); then
    local expected
    expected=$(echo "$checksums" | grep "$asset_name" | awk '{print $1}')
    if [ -n "$expected" ]; then
      verify_checksum "$TMP_FILE" "$expected"
    else
      warn "Asset not found in checksums.txt — skipping verification."
    fi
  else
    warn "checksums.txt not available — skipping verification."
  fi

  mkdir -p "$INSTALL_DIR"
  chmod +x "$TMP_FILE"
  mv "$TMP_FILE" "$INSTALL_DIR/$BINARY_NAME"
  # Disarm the trap since the file was moved successfully
  trap - EXIT
  info "Installed binary to $INSTALL_DIR/$BINARY_NAME"
  return 0
}

# ============================================================================
# Build from source (fallback)
# ============================================================================

build_from_source() {
  info "No pre-built binary available for $PLATFORM. Building from source..."

  if ! command -v cargo >/dev/null 2>&1; then
    info "Rust not found. Installing via rustup..."
    if [ "$DOWNLOADER" = "curl" ]; then
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    else
      wget -qO - https://sh.rustup.rs | sh -s -- -y
    fi
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
  fi

  local tmp_dir
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT

  info "Cloning repository..."
  git clone --depth 1 "https://github.com/$REPO.git" "$tmp_dir"

  info "Building release binary (this may take a few minutes)..."
  cargo build --release --manifest-path "$tmp_dir/Cargo.toml"

  mkdir -p "$INSTALL_DIR"
  cp "$tmp_dir/target/release/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
  chmod +x "$INSTALL_DIR/$BINARY_NAME"
  trap - EXIT
  rm -rf "$tmp_dir"
  info "Built and installed to $INSTALL_DIR/$BINARY_NAME"
}

# ============================================================================
# PATH check
# ============================================================================

check_path() {
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
      warn "$INSTALL_DIR is not in your PATH."
      info "Add it by appending this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
      info "  export PATH=\"\$HOME/.local/bin:\$PATH\""
      ;;
  esac
}

# ============================================================================
# Default config
# ============================================================================

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
    "notifications": true,
    "trust_mode": true
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

# ============================================================================
# Claude Code hooks
# ============================================================================

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

# ============================================================================
# Main
# ============================================================================

main() {
  info "Installing Claude Permission Hook..."

  detect_downloader
  detect_platform

  # Try pre-built binary first, fall back to building from source
  if ! download_binary; then
    build_from_source
  fi

  install_config
  configure_hooks
  check_path

  info ""
  info "Installation complete!"
  info "  Binary:  $INSTALL_DIR/$BINARY_NAME"
  info "  Config:  $CONFIG_DIR/config.json"
  info "  Hooks:   $CLAUDE_SETTINGS"
  info ""
  info "Restart Claude Code to activate."
}

main "$@"
