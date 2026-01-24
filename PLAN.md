# Notification System Merge Plan

This document outlines the phased implementation plan for merging `claude-notifications-go` features into `permission-hook`.

## Goal

Combine permission handling and smart notifications into a single Rust binary, allowing users to:
1. Auto-approve/deny commands (existing feature)
2. Receive desktop notifications when Claude needs attention
3. Optionally send webhook notifications (Slack, Discord, Telegram)

## Current State

**permission-hook** (Rust):
- PreToolUse hook for permission decisions
- Auto-approve/deny based on patterns
- Decision logging

**claude-notifications-go** (Go):
- Multiple hook types (PreToolUse, Stop, SubagentStop, Notification)
- Status detection state machine (6 statuses)
- Desktop notifications with sound
- Webhook support with retry/circuit-breaker
- Deduplication and cooldown management

---

## Phase 1: Core Infrastructure ✅ COMPLETE

**Goal**: Add hook event handling and status detection without notifications yet.

### Tasks

- [x] **1.1** Add new hook event types to input parsing
  - `Stop` - session/task stopped
  - `SubagentStop` - subagent completed
  - `Notification` - permission_prompt event

- [x] **1.2** Implement JSONL transcript parser (`jsonl.rs`)
  - Parse Claude Code transcript files
  - Extract messages, tools, text content
  - Get last N assistant messages

- [x] **1.3** Implement status analyzer (`analyzer.rs`)
  - State machine for 6 statuses:
    - `task_complete` - active tools used
    - `review_complete` - read-only tools with long analysis
    - `question` - AskUserQuestion or permission prompt
    - `plan_ready` - ExitPlanMode
    - `session_limit_reached` - session limit text detected
    - `api_error` - 401 error with login prompt
  - Tool categorization (active vs passive vs read-like)

- [x] **1.4** Implement session state manager (`state.rs`)
  - Per-session JSON state files in temp directory
  - Track last interactive tool, timestamps
  - Cooldown management

- [x] **1.5** Implement deduplication manager (`dedup.rs`)
  - Two-phase locking (early check + atomic acquisition)
  - Content-based deduplication
  - Lock file management with TTL

### Testing Phase 1
- [x] Unit tests for JSONL parsing (5 tests)
- [x] Unit tests for status analyzer (15 tests)
- [x] Unit tests for state manager (8 tests)
- [x] Unit tests for dedup (6 tests)
- [x] Integration test: hook event routing (4 tests in main.rs)

**Total: 96 tests passing**

### Deliverable
Hook correctly detects status from transcripts, manages state, prevents duplicates.
No notifications sent yet - just logging.

---

## Phase 2: Desktop Notifications ✅ COMPLETE

**Goal**: Send native desktop notifications when Claude needs attention.

### Tasks

- [x] **2.1** Add `notify-rust` dependency for cross-platform notifications

- [x] **2.2** Implement notifier module (`notifier.rs`)
  - Send desktop notifications
  - Platform-specific handling (Windows/macOS/Linux)
  - Notification title with emoji + status

- [x] **2.3** Implement summary generator (`summary.rs`)
  - Generate concise notification messages
  - Markdown cleanup (remove code blocks, links, etc.)
  - Status-specific message generation
  - 150 character limit with truncation

- [x] **2.4** Implement session name generator
  - Generate friendly session names from session ID
  - Include git branch and folder name

- [x] **2.5** Add notification config section
  ```json
  {
    "notifications": {
      "desktop": {
        "enabled": true
      },
      "suppressQuestionAfterTaskCompleteSeconds": 12,
      "suppressQuestionAfterAnyNotificationSeconds": 12,
      "notifyOnSubagentStop": false
    }
  }
  ```

- [x] **2.6** Wire up notifications in hook handler
  - Send notification after status detection
  - Respect cooldown settings
  - Check per-status enabled flags

### Testing Phase 2
- [x] Unit tests for summary generation (11 tests)
- [x] Unit tests for markdown cleanup (4 tests)
- [x] Unit tests for notifier (2 tests)
- [ ] Manual test: verify notifications appear on Windows
- [ ] Manual test: cooldown suppression works

**Total: 120 tests passing**

### Deliverable
Desktop notifications appear when:
- Task completes
- Plan is ready for review
- Claude has a question
- Session limit reached

---

## Phase 3: Sound Playback (Optional) ✅ COMPLETE

**Goal**: Play notification sounds.

### Tasks

- [x] **3.1** Add `rodio` dependency for audio playback (optional feature)

- [x] **3.2** Implement audio module (`audio.rs`)
  - Load and play MP3/WAV files (with `sound` feature)
  - Volume control
  - System notification sound fallback (Windows/macOS/Linux)

- [x] **3.3** Default to system sounds (no bundled files needed)
  - Uses Windows System.Media.SystemSounds
  - Uses macOS afplay with system sounds
  - Uses Linux paplay/aplay with freedesktop sounds

- [x] **3.4** Add sound config
  ```json
  {
    "notifications": {
      "desktop": {
        "sound": true,
        "volume": 1.0
      }
    }
  }
  ```

### Testing Phase 3
- [x] Unit tests for audio module (3 tests)
- [ ] Manual test: sounds play on notification
- [ ] Manual test: custom sound files work (with `sound` feature)

**Total: 126 tests passing**

### Deliverable
Notifications play system sounds by default. Custom sounds supported via optional `sound` feature.

---

## Phase 4: Webhook Notifications ✅ COMPLETE

**Goal**: Send notifications to external services (Slack, Discord, Telegram).

### Tasks

- [x] **4.1** Implement webhook sender (`webhook.rs`)
  - HTTP POST with configurable payload
  - Blocking send (hook process lifetime)

- [x] **4.2** Implement retry logic
  - Exponential backoff (1s, 2s, 4s, max 10s)
  - Configurable max attempts (default 3)

- [x] **4.3** Implement circuit breaker
  - Opens after 5 failures
  - 30s recovery timeout

- [x] **4.4** Implement rate limiting
  - Token bucket algorithm
  - Default 10 requests/minute

- [x] **4.5** Add webhook presets
  - Slack (attachments with color)
  - Discord (embeds with color)
  - Telegram (HTML format with chat_id)
  - Custom (JSON with status/title/message/session)

- [x] **4.6** Add webhook config
  ```json
  {
    "notifications": {
      "webhook": {
        "enabled": false,
        "preset": "slack",
        "url": "https://hooks.slack.com/...",
        "telegram_chat_id": "123456",
        "retry_enabled": true,
        "retry_max_attempts": 3
      }
    }
  }
  ```

### Testing Phase 4
- [x] Unit tests for payload formatting (4 tests)
- [x] Unit tests for circuit breaker (3 tests)
- [x] Unit tests for rate limiter (2 tests)
- [x] Unit tests for preset parsing (1 test)
- [x] Unit tests for status colors (1 test)

**Total: 148 tests passing**

### Deliverable
Webhooks send to Slack/Discord/Telegram with retry, circuit breaker, and rate limiting.

---

## Phase 5: Plugin Integration

**Goal**: Make it work as a proper Claude Code plugin.

### Tasks

- [ ] **5.1** Create `.claude-plugin/plugin.json` manifest

- [ ] **5.2** Create `hooks/hooks.json` with all event types
  ```json
  {
    "hooks": {
      "PreToolUse": [{"matcher": ".*", ...}],
      "Stop": [{"hooks": [...]}],
      "SubagentStop": [{"hooks": [...]}],
      "Notification": [{"matcher": "permission_prompt", ...}]
    }
  }
  ```

- [ ] **5.3** Add slash commands (optional)
  - `/notifications-settings` - configure notifications

- [ ] **5.4** Update README with full documentation

- [ ] **5.5** Create GitHub release workflow

### Deliverable
Installable as a Claude Code plugin via marketplace or git URL.

---

## File Structure (Final)

```
permission-hook/
├── src/
│   ├── main.rs              # Entry point, CLI handling
│   ├── config.rs            # Configuration (extended)
│   ├── permission.rs        # Permission logic (existing, refactored)
│   ├── analyzer.rs          # Status state machine
│   ├── jsonl.rs             # JSONL parser
│   ├── state.rs             # Session state manager
│   ├── dedup.rs             # Deduplication manager
│   ├── notifier.rs          # Desktop notifications
│   ├── audio.rs             # Sound playback
│   ├── webhook.rs           # HTTP webhooks
│   ├── summary.rs           # Message generation
│   └── platform.rs          # Cross-platform utilities
├── sounds/                   # Notification sounds
│   ├── task-complete.mp3
│   ├── question.mp3
│   └── plan-ready.mp3
├── .claude-plugin/
│   └── plugin.json
├── hooks/
│   └── hooks.json
├── tests/
│   ├── analyzer_test.rs
│   ├── dedup_test.rs
│   ├── state_test.rs
│   └── ...
├── Cargo.toml
├── README.md
└── PLAN.md
```

---

## Dependencies to Add

```toml
[dependencies]
# Existing
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
regex = "1.10"
chrono = "0.4"
dirs = "5.0"
reqwest = { version = "0.11", features = ["blocking", "json"] }

# New for notifications
notify-rust = "4"              # Desktop notifications
rodio = "0.17"                 # Audio playback (optional)
tokio = { version = "1", features = ["rt-multi-thread", "time"] }  # Async runtime
```

---

## Testing Strategy

### Unit Tests
Each module has comprehensive unit tests mirroring the Go implementation:
- `analyzer_test.rs` - State machine logic (25+ tests)
- `dedup_test.rs` - Lock acquisition, concurrent access (10+ tests)
- `state_test.rs` - State CRUD, cooldown (20+ tests)
- `summary_test.rs` - Message generation (10+ tests)
- `webhook_test.rs` - Payload formatting, retry (15+ tests)

### Integration Tests
- End-to-end hook handling with mock stdin
- Transcript analysis with fixture files

### Manual Tests
- Desktop notifications appear correctly
- Sounds play at correct volume
- Webhook delivery to test endpoints

---

## Migration Path

Users of both plugins:
1. Uninstall `claude-notifications-go`
2. Update `permission-hook` config to enable notifications
3. Restart Claude Code

The config file will be backward compatible - existing permission settings preserved.

---

## Timeline Estimate

| Phase | Scope | Status |
|-------|-------|--------|
| Phase 1 | Core Infrastructure | ✅ Complete |
| Phase 2 | Desktop Notifications | ✅ Complete |
| Phase 3 | Sound Playback | ✅ Complete |
| Phase 4 | Webhooks | ✅ Complete |
| Phase 5 | Plugin Integration | Not Started |

---

## Notes

- Each phase is independently testable
- Permission functionality remains intact throughout
- Config is backward compatible
- Can stop at any phase and have a working product
