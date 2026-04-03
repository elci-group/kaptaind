# Kaptaind Notification & i3-Status Integration Plan

## Objective
To provide users with real-time feedback on Kaptaind's background operations, including successful commits, version bumps, test failures, and current daemon state, integrating smoothly with desktop notification systems (`notify-send`) and window manager status bars (`i3status`, `polybar`).

## 1. State Persistence for Status Bars (i3status / polybar)

Status bars need a fast, non-blocking way to read the current daemon state.

**Approach: Status File Export**
- **Mechanism:** Kaptaind will write its current state to a lightweight file (e.g., `.kaptaind/status.json` or `/tmp/kaptaind-<repo_name>-status.json`).
- **Data Points:**
  - `status`: `Idle`, `Clustering`, `Testing`, `Committing`, `Failed`
  - `last_version`: e.g., `0.1.28`
  - `last_action_time`: Unix timestamp
  - `last_error`: Any test or git push error
- **Update Triggers:** Written at the start and end of `daemon::scheduler::run()` loops.
- **i3status/polybar script:** A simple bash or python script can `cat` and parse this JSON file using `jq` to output a formatted string (e.g., `🚢 v0.1.28 [Idle]` or `🛑 Test Failed`).

## 2. Desktop Notifications (DBUS / libnotify)

Users need transient popups when significant events happen (commits, errors).

**Approach: Configurable Hooks & Native DBUS**

### Option A: Shell Command Hook (Recommended for Flexibility)
- Extend `kaptaind.toml` with a `[notify]` section:
  ```toml
  [notify]
  on_commit = 'notify-send "Kaptaind Bump" "Version $KAPTAIND_VERSION\nScore: $KAPTAIND_SCORE"'
  on_error = 'notify-send -u critical "Kaptaind Error" "Tests failed!"'
  ```
- **Implementation:** In `src/daemon/scheduler.rs`, invoke the shell commands using `std::process::Command::new("sh").arg("-c")` while injecting environment variables containing event data.

### Option B: Native Rust Integration (`notify-rust`)
- Add `notify-rust` to `Cargo.toml`.
- Send notifications directly via DBUS.
- **Implementation:**
  - On commit: Send standard notification with the commit summary.
  - On test hook failure: Send a critical notification.
- *Pros:* No external shell processes, native icon support.
- *Cons:* Harder to customize formatting for the user without complex config templates.

## 3. Implementation Phases

### Phase 1: Status File Logging
1. Define a `State` enum and a `StatusReport` struct (deriving `serde::Serialize`).
2. Add a helper function in `src/daemon/` to safely atomically write the status to `.kaptaind/state.json`.
3. Update state at key steps: cluster start, test start, test result, commit result.

### Phase 2: Notification Hooks
1. Add `NotifyConfig` to `src/config/loader.rs`.
2. Update `src/daemon/scheduler.rs` to parse the notification commands or invoke `notify-rust` based on configuration.
3. Inject the analysis metrics (score, version, touched files) into the notification context.

## Summary
The combination of an atomic state file `.kaptaind/state.json` provides an easy ingestion point for `i3status` scripts, while configurable shell hooks for notifications offer the flexibility needed for users to integrate `notify-send`, Telegram bots, or other custom alerting systems without bloating the core Rust binary.