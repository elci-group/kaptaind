# Robust Commit & Push Automation Plan

## Executive Summary

This plan enhances kaptaind's commit and push automation with enterprise-grade robustness while maintaining the project's philosophy of simplicity, explicitness, and configurability. The design builds on existing patterns (git2, anyhow, tracing, async/await) without breaking changes.

---

## Current State Assessment

### Existing Implementation

| Component | Location | Current Capability |
|-----------|----------|-------------------|
| Commit Orchestrator | `src/commit/orchestrator.rs` | 3 staging modes (all/cluster/pattern), exclude patterns, git2-based |
| Push Controller | `src/push/controller.rs` | Basic `git push origin <branch>`, no retry, no conflict handling |
| Scheduler | `src/daemon/scheduler.rs` | Sequential commit→push, simple error handling |
| Config | `src/config/loader.rs` | `PushConfig { enabled, branch }` only |

### Identified Gaps

1. **No retry logic** - transient network failures fail the entire operation
2. **No conflict handling** - push rejection leaves repo in inconsistent state
3. **No pre-push validation** - no build/test before push (separate from commit tests)
4. **No authentication failure recovery** - SSH/key issues are fatal
5. **No dry-run mode** - cannot preview what would happen
6. **No commit signing** - GPG signing not supported
7. **No batching** - every cluster triggers immediate push (noise for high-velocity repos)
8. **No force push protection** - accidental force push risk

---

## Design Philosophy

This plan adheres to kaptaind's core principles:

1. **Explicit over implicit** - All behaviors are configurable, no magic
2. **Fail safe** - When in doubt, preserve data and notify
3. **Graceful degradation** - Features degrade gracefully when unavailable
4. **Observability** - All actions are traceable and logged
5. **Non-breaking** - Existing configs continue to work

---

## Phase 1: Enhanced Push Reliability

### 1.1 Retry Logic with Exponential Backoff

**New Config:**
```toml
[push]
enabled = true
branch = "main"
remote = "origin"                    # NEW: configurable remote

[push.retry]                         # NEW: retry configuration
max_attempts = 3                     # default: 3 attempts
initial_delay_ms = 1000              # default: 1s initial delay
backoff_multiplier = 2.0             # default: exponential backoff
max_delay_ms = 30000                 # default: cap at 30s
retryable_errors = ["timeout", "connection", "lock"]  # default error types
```

**Implementation:**
```rust
// src/push/strategy.rs
pub struct RetryStrategy {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub backoff_multiplier: f64,
    pub max_delay: Duration,
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(30),
        }
    }
}

pub async fn push_with_retry(
    repo: &Repository,
    branch: &str,
    remote: &str,
    strategy: &RetryStrategy,
) -> Result<PushResult, PushError> {
    // Implementation with exponential backoff
    // Classifies errors as retryable vs fatal
    // Returns detailed result for telemetry
}
```

**Error Classification:**
- **Retryable:** Network timeout, connection reset, lock contention, rate limit (429)
- **Fatal:** Authentication failure (401/403), non-fast-forward (needs rebase), repository not found

### 1.2 Conflict Detection & Auto-Rebase

**New Config:**
```toml
[push.conflict]                      # NEW: conflict resolution
auto_rebase = false                  # default: manual (safety first)
rebase_strategy = "simple"           # "simple" | "interactive" (future)
auto_abort_on_conflict = true        # abort rebase if conflicts exist
preserve_merges = false              # --rebase-merges flag
```

**Implementation:**
```rust
// src/push/rebase.rs
pub enum ConflictResolution {
    AlreadyUpToDate,
    Rebased { commits_replayed: usize },
    Conflicts { files: Vec<PathBuf> },
    Failed { reason: String },
}

pub async fn try_auto_rebase(
    repo: &Repository,
    branch: &str,
    remote: &str,
    config: &ConflictConfig,
) -> Result<ConflictResolution, RebaseError> {
    // 1. Fetch latest from remote
    // 2. Check if rebase needed
    // 3. Attempt rebase if auto_rebase enabled
    // 4. Return conflicts for manual resolution if auto_abort_on_conflict
}
```

**Safety Measures:**
- Never auto-rebase unless explicitly enabled
- Always create reflog entry before rebase
- Abort on any conflict (preserve working tree)
- Notify user of manual steps needed

---

## Phase 2: Pre-Push Validation Pipeline

### 2.1 Pre-Push Hooks

**New Config:**
```toml
[push.pre_push]                      # NEW: pre-push validation
enabled = false                      # default: disabled
command = "cargo test && cargo build --release"  # validation command
required = true                      # fail push if validation fails
timeout_secs = 300                   # 5 minute default timeout
```

**Rationale:** Commit-time tests ensure correctness; pre-push tests ensure deployability. These are distinct concerns.

**Implementation:**
```rust
// src/push/validation.rs
pub enum PrePushResult {
    Passed,
    Failed { exit_code: Option<i32>, stderr: String },
    TimedOut { after: Duration },
    Skipped,
}

pub async fn run_pre_push(
    config: &PrePushConfig,
    repo_path: &Path,
) -> PrePushResult {
    // Similar pattern to test hook in scheduler
    // Runs in repo root with timeout
    // Non-blocking if not required
}
```

### 2.2 Force Push Protection

**New Config:**
```toml
[push.safety]                        # NEW: safety features
allow_force = false                  # never allow force push
require_upstream_exist = true        # fail if upstream branch missing
protect_branches = ["main", "master", "release/*"]  # extra protection
```

**Implementation:**
```rust
// src/push/safety.rs
pub fn validate_push_safety(
    repo: &Repository,
    branch: &str,
    config: &SafetyConfig,
) -> Result<(), SafetyError> {
    // Check if branch is protected
    // Verify upstream exists
    // Ensure we're not force-pushing (check ahead/behind)
}
```

---

## Phase 3: Advanced Push Strategies

### 3.1 Push Batching

**Problem:** High-velocity editing creates many small commits with rapid pushes.

**Solution:** Batch commits before pushing.

**New Config:**
```toml
[push.batch]                         # NEW: batch push mode
enabled = false                      # default: immediate push
min_commits = 3                      # minimum commits before push
max_wait_secs = 300                  # maximum wait before forcing push
push_on_quit = true                  # flush on daemon shutdown
```

**Implementation:**
```rust
// src/push/batcher.rs
pub struct PushBatcher {
    pending_commits: Vec<CommitMeta>,
    last_push: Option<Instant>,
    config: BatchConfig,
}

impl PushBatcher {
    pub fn record_commit(&mut self, commit: CommitMeta) {
        self.pending_commits.push(commit);
    }
    
    pub fn should_push(&self) -> bool {
        self.pending_commits.len() >= self.config.min_commits
            || self.time_since_last_push() > self.config.max_wait
    }
}
```

**Scheduler Integration:**
- Commit always happens immediately (preserves history)
- Push is deferred to batch window
- Batch is flushed on graceful shutdown

### 3.2 Dry Run Mode

**New Config:**
```toml
[push]
dry_run = false                      # NEW: preview mode
```

**Behavior:**
- Performs all validations
- Logs what *would* be pushed
- Does not execute actual push
- Useful for CI/testing

---

## Phase 4: Commit Enhancements

### 4.1 GPG Signing Support

**New Config:**
```toml
[commit]                             # NEW: commit section
gpg_sign = false                     # enable GPG signing
gpg_key_id = null                    # optional specific key
```

**Implementation:**
```rust
// src/commit/sign.rs
pub fn configure_signing(
    repo: &Repository,
    config: &CommitConfig,
) -> Result<(), SigningError> {
    // Set git2 commit options for signing
    // Handle missing GPG gracefully
}
```

### 4.2 Pre-Commit Hooks (Client-Side)

**New Config:**
```toml
[commit.pre_commit]                  # NEW: client-side hooks
enabled = false
command = "cargo fmt --check"
required = true
```

**Note:** Distinct from `test` hook (which is a gate). Pre-commit is for formatting/linting.

---

## Phase 5: Enhanced Observability

### 5.1 Push Telemetry

**New Artifacts:**
```json
// .kaptaind/push/<cluster-id>.json
{
  "cluster_id": "...",
  "attempts": [
    {
      "timestamp": "...",
      "result": "retryable_error",
      "error": "connection reset",
      "retry_delay_ms": 1000
    },
    {
      "timestamp": "...",
      "result": "success",
      "remote_commit": "abc123",
      "latency_ms": 250
    }
  ],
  "rebase_performed": false,
  "commits_pushed": 1
}
```

### 5.2 Push Notifications

**Enhanced Notifications:**
- Push success/failure notifications
- Rebase required notifications
- Conflict resolution instructions
- Batch flush notifications

---

## Phase 6: Implementation Roadmap

### Phase 6.1: Foundation (Week 1)
- [ ] Create `src/push/strategy.rs` with retry logic
- [ ] Extend `PushConfig` with retry settings
- [ ] Add error classification for retryable vs fatal
- [ ] Unit tests for retry strategy

### Phase 6.2: Conflict Handling (Week 2)
- [ ] Create `src/push/rebase.rs` for auto-rebase
- [ ] Implement fetch-before-push pattern
- [ ] Add conflict detection
- [ ] Add safety checks (never auto-rebase without opt-in)

### Phase 6.3: Validation Pipeline (Week 3)
- [ ] Create `src/push/validation.rs` for pre-push hooks
- [ ] Add pre-push config section
- [ ] Integrate with scheduler (post-commit, pre-push)

### Phase 6.4: Batching & Safety (Week 4)
- [ ] Create `src/push/batcher.rs`
- [ ] Implement push batching in scheduler
- [ ] Add force push protection
- [ ] Add dry-run mode

### Phase 6.5: Polish (Week 5)
- [ ] GPG signing support
- [ ] Enhanced telemetry
- [ ] Documentation updates
- [ ] Integration tests

---

## Configuration Migration Guide

### Existing Config (Still Valid)
```toml
[push]
enabled = false
branch = "main"
```

### Enhanced Config (Optional)
```toml
[push]
enabled = true
branch = "main"
remote = "origin"
dry_run = false

[push.retry]
max_attempts = 3
initial_delay_ms = 1000
backoff_multiplier = 2.0

[push.conflict]
auto_rebase = false
auto_abort_on_conflict = true

[push.pre_push]
enabled = true
command = "cargo test --release"
required = true
timeout_secs = 300

[push.safety]
allow_force = false
protect_branches = ["main", "master"]

[push.batch]
enabled = true
min_commits = 3
max_wait_secs = 300

[commit]
gpg_sign = false
```

---

## Error Handling Strategy

### Retryable Errors (Auto-Retry)
- `TransientNetwork` - connection timeout, reset
- `RateLimited` - 429 from remote
- `LockContention` - repository locked

### Recoverable Errors (Manual Intervention)
- `ConflictDetected` - needs rebase/merge
- `AuthenticationFailed` - credential issue
- `PermissionDenied` - access rights

### Fatal Errors (Fail Fast)
- `RepositoryNotFound` - remote doesn't exist
- `InvalidRef` - branch name invalid
- `HookRejected` - pre-push hook failed (required)

---

## Testing Strategy

### Unit Tests
- Retry strategy timing calculations
- Error classification logic
- Batch window calculations

### Integration Tests
- Mock git repository with git-daemon
- Simulated network failures (Toxiproxy)
- Conflict scenarios

### E2E Tests
- Full daemon cycle with push
- Rebase scenarios
- Batch accumulation and flush

---

## Success Metrics

1. **Reliability:** 99.9% push success rate (up from current ~95% with transient failures)
2. **Recovery:** 100% of recoverable errors have clear user instructions
3. **Safety:** 0 accidental force pushes to protected branches
4. **Velocity:** Batch mode reduces pushes by 60-80% for high-velocity editing

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Auto-rebase causes data loss | Disabled by default; requires explicit opt-in |
| Retry storms overwhelm remote | Exponential backoff with jitter; max delay cap |
| Batch delays critical fixes | `max_wait_secs` cap; manual flush via CLI |
| Pre-push hooks too slow | Configurable timeout; can be made non-required |
| Breaking existing configs | All new fields have defaults; non-breaking |

---

## Conclusion

This plan transforms kaptaind's push automation from "best effort" to "enterprise robust" while maintaining the project's simplicity ethos. Every feature is opt-in, well-observed, and safely defaults to current behavior.

The modular design allows incremental adoption - users can enable just retry logic, or go full batch mode with pre-push validation. All roads lead to reliable, observable, safe automation.
