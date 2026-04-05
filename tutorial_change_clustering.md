# ⏱️ Change Clustering Tutorial

**Version:** `1.0.0` | **Status:** ✅ Stable | **Last Updated:** April 2026

---

## 📋 Table of Contents

1. [Overview](#overview)
2. [How Clustering Works](#how-clustering-works)
3. [Configuration](#configuration)
4. [Tuning for Your Workflow](#tuning-for-your-workflow)
5. [Advanced Scenarios](#advanced-scenarios)
6. [Troubleshooting](#troubleshooting)

---

## Overview

⏱️ **What is Change Clustering?** Kaptaind automatically batches rapid file changes into logical groups. This solves a common problem:

**Without clustering:**
```
You save a file → Kaptaind commits immediately
You save again 2 seconds later → Another commit!
IDE format-on-save triggers → ANOTHER commit!

Result: 10 commits for what should be 1 logical change
```

**With clustering (default 5 seconds):**
```
You save files rapidly (format-on-save, imports, etc.)
Kaptaind waits 5 seconds for changes to settle
All rapid changes grouped into ONE logical cluster
ONE smart commit that represents the whole change

Result: 1 commit that "just works"
```

🎯 **Key Benefit:** Keeps your git history clean and semantic without manual batching.

---

## How Clustering Works

### The Timeline

Imagine you're editing a TypeScript file with auto-save enabled:

```
Time: 0ms   → You hit Save
             └─ File write detected
             └─ Clustering engine starts timer: [5s window]

Time: 100ms → Prettier auto-formats
             └─ File write detected
             └─ (Clustering timer continues)

Time: 200ms → Auto-import resolves
             └─ File write detected
             └─ (Clustering timer continues)

Time: 5000ms → Clustering window closes!
              └─ All 3 changes grouped into 1 cluster
              └─ Diff analysis begins
              └─ Version bump calculated
              └─ Git commit created

Time: 5100ms → Ready for next cluster
```

### Architecture

```
┌─────────────────────────────────────────────┐
│    Filesystem Watcher (notify crate)        │
│  Monitors: All file changes in repo         │
└────────────┬────────────────────────────────┘
             ↓
┌─────────────────────────────────────────────┐
│    Temporal Clustering Engine               │
│  Groups events within [cluster.window] secs │
│  (default: 5 seconds)                       │
└────────────┬────────────────────────────────┘
             ↓
┌─────────────────────────────────────────────┐
│    Diff Analysis Pipeline                   │
│  Scores: structural, api, deps, runtime...  │
└────────────┬────────────────────────────────┘
             ↓
┌─────────────────────────────────────────────┐
│    Version Bump & Commit                    │
│  Creates rich git commit with metadata      │
└─────────────────────────────────────────────┘
```

### Event Batching Example

```
Cluster 1 (5s window)
├─ src/auth.ts changed (line 10–20)
├─ src/auth.ts changed (line 50–60, auto-format)
├─ src/index.ts changed (re-import fix)
└─ Result: 1 cluster → 1 commit

Cluster 2 (5s window)
├─ tests/auth.test.ts changed
├─ tests/auth.test.ts changed (auto-format)
└─ Result: 1 cluster → 1 commit
```

---

## Configuration

### Basic Settings

In **`kaptaind.toml`**:

```toml
[cluster]
window = 5  # Seconds between file changes before a cluster "closes"
```

That's it! One setting controls the entire clustering behavior.

### Understanding the Window

```
window = 2
└─ Aggressive: Commits very frequently
   └─ Good for: Ultra-fast feedback, test-driven workflows
   └─ Bad for: Too many commits, noisy history

window = 5 (default)
└─ Balanced: Good for most workflows
   └─ Good for: Most developer workflows, standard save patterns
   └─ Works with: Format-on-save, auto-import

window = 10
└─ Conservative: Waits longer before committing
   └─ Good for: Batch workers, slower editors, script-based saves
   └─ Bad for: Long feedback loops, harder to correlate commits

window = 30
└─ Very conservative: Batches widely disparate changes
   └─ Good for: CI/CD pipelines (rare in production)
   └─ Bad for: Debugging (too much per commit)
```

### Quick Reference

| Window | Best For | Example Scenario |
|--------|----------|------------------|
| **2s** | Test-driven dev, tight feedback | TDD: write test → save → commit → repeat |
| **5s** (default) | Standard workflows | Format-on-save + imports | auto-complete |
| **10s** | Slower machines | Large repo, slow disk, limited RAM |
| **15s** | Mobile dev workflows | Xcode/Android Studio with slower syncs |
| **30s** | Batch processing | CI job that makes 10 unrelated changes |

---

## Tuning for Your Workflow

### 🏃 Fast Typing + Format-on-Save

**Symptom:** Many "quick" edits, auto-format triggers multiple saves.

**Solution:** Increase window to let formatting settle.

```toml
[cluster]
window = 8  # Give auto-format time to finish
```

### 🐢 Slow Disk / High Latency

**Symptom:** File events arrive in bursts, not individually.

**Solution:** Increase window; filesystem is already batching for you.

```toml
[cluster]
window = 10  # Disk writes are inherently slow
```

### 🤖 Automated Scripts

**Symptom:** A script makes 50 file changes instantly, you want ONE commit.

**Solution:** Increase window to collect all changes.

```toml
[cluster]
window = 15  # Scripts often dump many files at once
```

### ✅ CI/CD Pipelines

**Symptom:** Running in a container, want minimal commits.

**Solution:** Disable daemon, use `kaptaind analyze` directly.

```bash
# In your CI script:
kaptaind-cli analyze | tee analysis.json
git add VERSION .kaptaind/analysis/
git commit -m "chore: auto-version"
```

### ⚡ Real-Time Feedback

**Symptom:** Want to see commits as soon as you save.

**Solution:** Decrease window (trade off: more commits).

```toml
[cluster]
window = 2  # Snappy feedback
```

---

## Advanced Scenarios

### 🔧 Scenario 1: IDE with Aggressive Auto-Save

**Your IDE:** VS Code with `files.autoSave = "afterDelay"` and `files.autoSaveDelay = 200`

**Problem:** Every keystroke triggers a save → many clusters.

**Solution:**
```toml
[cluster]
window = 5  # Waits for multiple auto-saves to batch
```

Or configure your IDE to save less frequently:
```json
// .vscode/settings.json
{
  "files.autoSave": "onFocusChange",  // Save on blur, not keystroke
  "files.autoSaveDelay": 2000
}
```

### 🔄 Scenario 2: Multi-Tool Editing

**Your workflow:** Edit in IDE + run scripts to generate files

```
Time 0s:    You edit src/app.ts (IDE save)
Time 1s:    You run: npm run build   (generates dist/*)
Time 2s:    You run: npm run format  (reformats src/*)
Time 5s:    Cluster closes
            └─ Includes IDE changes + generated files + formatted files
```

**Config:**
```toml
[cluster]
window = 5  # Default handles this well
```

### 📱 Scenario 3: Mobile Development (Xcode)

**Problem:** Xcode builds slowly; file syncs happen in batches.

**Solution:**
```toml
[cluster]
window = 10  # Xcode build + sync is inherently slower
```

### 🌍 Scenario 4: Monorepo with Many Packages

**Problem:** Changes to package A, B, and C happen at slightly different times.

```
package-a/src/index.ts changed  (time: 0ms)
package-b/src/hooks.ts changed  (time: 200ms)
package-c/src/util.ts changed   (time: 400ms)

With window=5:
└─ All grouped into 1 cluster (good!)

With window=2:
└─ package-a gets its own cluster
└─ package-b gets its own cluster
└─ package-c gets its own cluster
   (more commits, but clearer intent)
```

**Choose based on your preference:**
- **Cohesive commits:** `window = 5` (group all changes)
- **Granular commits:** `window = 2` (separate by timing)

### 🎯 Scenario 5: Release Coordination

**Want:** Each feature gets its own commit, unrelated fixes grouped separately.

**Option A: Long window for features, short for hotfixes**
```bash
# During feature development:
kaptaind --daemon

# When ready to release, temporarily increase window:
# Edit kaptaind.toml:
#   window = 10
# This encourages batching of final polish changes

# Commit the changes:
# One rich commit that says "Feature: XYZ" covers everything
```

**Option B: Use Aim of Change (AoC) Sessions**
```bash
kaptaind-cli aoc start "feature: authentication redesign"
# ...make changes...
kaptaind-cli aoc ship

# Each AoC cluster is its own logical unit, regardless of timing
```

---

## Troubleshooting

### ❌ "Kaptaind is committing too frequently"

**Symptom:** A new commit appears every few seconds.

**Cause:** `window` is too small, or your editor is saving very rapidly.

**Debug:**
```bash
# Check current window
grep window kaptaind.toml

# Check how often daemon wakes up
tail -20 .kaptaind/daemon.err | grep "cluster"
```

**Fix Option 1: Increase window**
```toml
[cluster]
window = 10  # Increase from 5 to 10 seconds
```

**Fix Option 2: Configure editor to save less frequently**
```json
// VS Code
{
  "files.autoSave": "onFocusChange",  // Not on keystroke
  "files.autoSaveDelay": 3000         // 3 seconds
}
```

**Fix Option 3: Ignore rapid changes**
```bash
# Temporarily disable daemon during heavy editing
kaptaind-cli stop
# ...work...
kaptaind --daemon  # Restart when done
```

---

### ❌ "Kaptaind is never committing"

**Symptom:** You make changes, but no commits appear.

**Cause:** Clustering is working fine, but something else blocks commits:
- Tests are failing
- Watcher is not active
- Daemon crashed

**Debug:**
```bash
# Check daemon status
kaptaind-cli status

# Check if watcher is active
ps aux | grep -i kaptaind | grep -v grep

# Check error logs
tail -50 .kaptaind/daemon.err

# Try a dry-run analysis
kaptaind-cli analyze
```

**Fix:**
- If daemon isn't running: `kaptaind --daemon`
- If tests are failing: `cargo test` (or your test command) and fix errors
- If watcher crashed: check logs and restart

---

### ❌ "Commits are being grouped that shouldn't be"

**Symptom:** Two unrelated changes (different features) ended up in the same commit.

**Cause:** They both happened within the 5-second window (by coincidence).

**Fix Option 1: Use AoC Sessions**
```bash
kaptaind-cli aoc start "feature: authentication"
# ... make feature changes ...

# Separate feature automatically tagged, even if timing overlaps
```

**Fix Option 2: Use longer window (reduces collisions)**
```toml
[cluster]
window = 15  # Longer gaps between unrelated features
```

**Fix Option 3: Manually group (advanced)**
```bash
# Temporarily increase window during specific changes:
# Edit kaptaind.toml: window = 30
# Make your changes (all batched)
# Restore: window = 5
```

---

### ❌ "Clustering is adding latency to my workflow"

**Symptom:** I save a file, want to test immediately, but Kaptaind blocks for 5 seconds.

**Cause:** Waiting for clustering window to close.

**Fix Option 1: Use faster window**
```toml
[cluster]
window = 2  # Closes faster, more commits, but snappier feedback
```

**Fix Option 2: Use separate terminal**
```bash
# Terminal 1: Run Kaptaind daemon
kaptaind --daemon

# Terminal 2: Do your work
# Make changes, run tests immediately (don't wait for commits)
```

**Fix Option 3: Disable daemon during intensive testing**
```bash
kaptaind-cli stop

# ... run tests, develop ...

kaptaind --daemon  # Restart when done
```

---

## 🎓 Real-World Example

### TypeScript Project with Auto-Format

**Setup:**
```json
// .vscode/settings.json
{
  "editor.formatOnSave": true,
  "editor.autoSave": "afterDelay",
  "files.autoSaveDelay": 500
}
```

**kaptaind.toml:**
```toml
[cluster]
window = 5  # Default; handles format delays well
```

**Workflow:**

```
You type: const name = "alice"  and hit save

Time: 0ms
  → .ts file written to disk
  → Clustering timer starts: [====== 5s ======]

Time: 500ms
  → VS Code auto-format triggers
  → Prettier reformats the file (no logic change)
  → Another write event
  → Clustering timer restarts: [====== 5s ======]

Time: 5500ms
  → Clustering window closes
  → Kaptaind analyzes: 1 logical change (the line you typed)
  → Detects: no API changes, small structural change
  → Bump: PATCH
  → Commit: "chore: update name variable"

Result: 1 clean commit despite multiple saves
```

---

## 🎓 Next Steps

- 📖 Read the [main README](./README.md) for clustering details
- ⚙️ Choose a `window` value for your workflow
- 🔍 Monitor clustering behavior: `kaptaind-cli log`
- 📊 Adjust based on your commit frequency

---

**Made with ❤️ by the Kaptaind team**

*Last updated: April 2026 | Version 1.0.0*
