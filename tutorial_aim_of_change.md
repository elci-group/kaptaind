# 🎯 Aim of Change (AoC) Sessions Tutorial

**Version:** `1.0.0` | **Status:** ✅ Stable | **Last Updated:** April 2026

---

## 📋 Table of Contents

1. [Overview](#overview)
2. [Core Concepts](#core-concepts)
3. [Getting Started](#getting-started)
4. [Managing Sessions](#managing-sessions)
5. [Agent Interception](#agent-interception)
6. [Advanced Workflows](#advanced-workflows)
7. [Troubleshooting](#troubleshooting)

---

## Overview

🎯 **What is AoC?** Aim of Change sessions group related code changes into intent-driven clusters with full traceability. They solve the problem of "scattered commits that should be one feature."

💼 **Use Cases:**
- 🔐 Feature development ("add OAuth2 authentication")
- ♻️ Refactoring ("modernize auth middleware")
- 🐛 Bug fixes ("fix memory leak in cache layer")
- 🚀 Release coordination ("v2.0 stable")

✨ **Key Benefit:** Track *why* changes were made, not just *what* changed.

---

## Core Concepts

### What Gets Grouped?

An AoC session captures:

```
┌─────────────────────────────────────────┐
│     Aim of Change Session: "OAuth2"     │
├─────────────────────────────────────────┤
│                                         │
│  Commit 1: Add OAuth2 provider class    │
│  Commit 2: Wire up redirect logic       │
│  Commit 3: Add token validation tests   │
│  Commit 4: Update CONFIG with key docs  │
│                                         │
│  Total: 4 commits, 12 files changed     │
│  Duration: 2 hours                      │
│  Status: SHIPPED (manifested)           │
│                                         │
└─────────────────────────────────────────┘
```

### Session Lifecycle

```
START ("feature: oauth2")
  ↓
ACTIVE (automatic tagging of commits)
  ↓
SHIP (finalize & export manifest)
  ↓
MANIFESTED (.kaptaind/aoc/manifests/<id>.json)
```

---

## Getting Started

### Step 1️⃣: Start a Session

Begin work on a feature with a clear intent:

```bash
kaptaind-cli aoc start "feature: oauth2 authentication"
```

**Output:**
```
✓ Session started: feature: oauth2 authentication
  Session ID: 8c4e2d19-a0b3-4c2e-9d7e-1a5f3b8c2d0e
  Active: .kaptaind/aoc/active.json
```

### Step 2️⃣: Work Normally

Make your changes and commit as usual. Kaptaind automatically tags each commit:

```bash
# Your work...
git add .
git commit -m "Add OAuth2 provider class"

# In .kaptaind/aoc/active.json:
# {
#   "id": "8c4e2d19-a0b3-4c2e-9d7e-1a5f3b8c2d0e",
#   "label": "feature: oauth2 authentication",
#   "started_at": "2026-04-05T10:30:00Z",
#   "commits": [
#     {
#       "hash": "abc1234",
#       "kaptaind_version": "v0.2.1",
#       "timestamp": "2026-04-05T10:35:22Z"
#     }
#   ]
# }
```

### Step 3️⃣: Check Progress

See how many commits have been grouped:

```bash
kaptaind-cli aoc status
```

**Output:**
```
✓ Active Session: feature: oauth2 authentication
  Session ID: 8c4e2d19-a0b3-4c2e-9d7e-1a5f3b8c2d0e
  Commits grouped: 4
  Duration: 2h 15m
  Files touched: 12
  Lines added: 340 | Lines removed: 28
```

### Step 4️⃣: Ship the Session

When your feature is complete, finalize the session:

```bash
kaptaind-cli aoc ship
```

**Output:**
```
✓ Session shipped
  Manifest ID: 8c4e2d19-a0b3-4c2e-9d7e-1a5f3b8c2d0e
  Manifest path: .kaptaind/aoc/manifests/8c4e2d19-a0b3-4c2e-9d7e-1a5f3b8c2d0e.json
  
  Ready for:
  - Release notes generation
  - Deployment tracking
  - Audit logging
```

---

## Managing Sessions

### 📂 File Structure

After starting and shipping a session, you'll have:

```
.kaptaind/
├── aoc/
│   ├── active.json                    # Current active session (if any)
│   └── manifests/
│       ├── 8c4e2d19-...json          # Shipped session manifest
│       ├── a1b2c3d4-...json
│       └── ...
└── traces/
    ├── 8c4e2d19-...json              # Trace records (linked to AoC)
    └── ...
```

### Understanding the Manifest

A shipped manifest looks like:

```json
{
  "id": "8c4e2d19-a0b3-4c2e-9d7e-1a5f3b8c2d0e",
  "label": "feature: oauth2 authentication",
  "cluster": "oauth2-cluster-a7f3",
  "trace_ids": [
    "abc1234",
    "def5678",
    "ghi9012"
  ],
  "initial_version": "0.1.2",
  "final_version": "0.2.0",
  "commit_count": 4,
  "test_failures": 0,
  "cluster_count": 3,
  "started_at": "2026-04-05T10:30:00Z",
  "shipped_at": "2026-04-05T12:45:30Z",
  "commits": [
    {
      "hash": "abc1234def5678",
      "subject": "feat: add oauth2 provider interface",
      "kaptaind_version": "v0.1.3"
    },
    {
      "hash": "ghi9012jkl3456",
      "subject": "feat: wire up redirect and token validation",
      "kaptaind_version": "v0.2.0"
    }
  ]
}
```

### Query Shipped Sessions

Generate release notes from any shipped manifest:

```bash
# Using the web dashboard
# GET /api/kaptaind/aoc?id=8c4e2d19-a0b3-4c2e-9d7e-1a5f3b8c2d0e

# Or read the JSON directly
cat .kaptaind/aoc/manifests/8c4e2d19-a0b3-4c2e-9d7e-1a5f3b8c2d0e.json | jq .
```

### Cancel a Session

If you want to discard an active session without shipping:

```bash
kaptaind-cli aoc cancel
```

**Effect:**
- Clears `.kaptaind/aoc/active.json`
- Commits remain in git history (not reverted)
- No manifest created

---

## Agent Interception

### What Is Interception?

Agent Interception captures structured observability data (test results, build logs, etc.) alongside an AoC session for audit trails and compliance.

### Basic Usage

Run a command and capture its output linked to the current AoC session:

```bash
kaptaind-cli aoc intercept -- npm test
```

**What Happens:**
1. Captures the command's stdout/stderr
2. Records exit code
3. Stores result in `.kaptaind/traces/<uuid>.json`
4. Links trace to current active AoC session

### Advanced: With Model & Intent

Analyze the captured output with an AI model:

```bash
kaptaind-cli aoc intercept \
  --model claude-3-5-sonnet \
  --intent "refactor auth middleware" \
  -- npm test
```

**Output:**
```json
{
  "trace_id": "abc1234def5678",
  "aoc_session_id": "8c4e2d19-a0b3-4c2e-9d7e-1a5f3b8c2d0e",
  "intent": "refactor auth middleware",
  "command": "npm test",
  "exit_code": 0,
  "stdout": "✓ 42 tests passed",
  "stderr": "",
  "analysis": {
    "model": "claude-3-5-sonnet",
    "summary": "All tests pass. The refactored middleware maintains backward compatibility while improving performance by ~15%.",
    "confidence": 0.92
  }
}
```

### Use Cases

| Scenario | Command | Intent |
|----------|---------|--------|
| Feature + tests | `npm test` | "add OAuth2 provider" |
| Refactor + linting | `cargo clippy` | "modernize error handling" |
| Performance work | `npm run benchmark` | "optimize cache layer" |
| Security audit | `cargo audit` | "update vulnerable deps" |

---

## Advanced Workflows

### 🏢 Release Coordination

Coordinate a multi-team release across services:

**Service A (Backend):**
```bash
kaptaind-cli aoc start "release: v2.0.0 backend"
# ... make changes ...
kaptaind-cli aoc ship
```

**Service B (Frontend):**
```bash
kaptaind-cli aoc start "release: v2.0.0 frontend"
# ... make changes ...
kaptaind-cli aoc ship
```

**Release Manifest:**
```bash
# Query both manifests to track release across repos
cat .kaptaind/aoc/manifests/*.json | jq 'select(.label | contains("v2.0.0"))'
```

### 🔍 Audit & Compliance

Build an audit trail for regulated environments:

```bash
# Start session with explicit audit intent
kaptaind-cli aoc start "audit: HIPAA compliance for patient data handling"

# Intercept all changes with evidence
kaptaind-cli aoc intercept --model claude-opus-4-6 --intent "HIPAA compliance" -- npm test
kaptaind-cli aoc intercept --intent "security review" -- cargo audit

# Ship and archive
kaptaind-cli aoc ship

# Exported manifest is immutable proof of work
cat .kaptaind/aoc/manifests/*.json | jq '.label, .commits, .analysis'
```

### 🚀 Continuous Deployment

Integrate AoC with CI/CD:

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    branches: [main]

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Check active AoC session
        run: |
          SESSION=$(kaptaind-cli aoc status --json)
          if [ -z "$SESSION" ]; then
            echo "No active AoC session; skipping release"
            exit 0
          fi

      - name: Run tests (captured in AoC trace)
        run: kaptaind-cli aoc intercept -- npm test

      - name: Deploy
        run: npm run deploy

      - name: Ship AoC session
        run: kaptaind-cli aoc ship

      - name: Generate release notes
        run: |
          MANIFEST=$(kaptaind-cli aoc manifest --latest)
          kaptaind-cli changelog --manifest "$MANIFEST" > RELEASE_NOTES.md

      - name: Create GitHub Release
        uses: actions/create-release@v1
        with:
          body_path: RELEASE_NOTES.md
```

### 📊 Multi-Session Aggregation

Analyze multiple shipped sessions for trend data:

```bash
# Get all shipped sessions
ls .kaptaind/aoc/manifests/*.json | xargs -I {} jq '{
  label: .label,
  commits: (.commits | length),
  duration_hours: ((.shipped_at - .started_at) / 3600),
  version_bump: (.final_version - .initial_version)
}' {} | jq -s 'group_by(.label) | map({
  feature: .[0].label,
  sessions: (. | length),
  avg_commits: (map(.commits) | add / length),
  avg_duration: (map(.duration_hours) | add / length)
})'
```

---

## Troubleshooting

### ❌ "No active session"

**Symptom:** Running `kaptaind-cli aoc status` returns error.

**Fix:**
```bash
kaptaind-cli aoc start "your feature name"
```

---

### ❌ "Session not recording commits"

**Symptom:** Made commits, but `aoc status` shows 0 commits.

**Cause:** Commits were made before starting the session, or daemon is not running.

**Fix:**
```bash
# Verify daemon is running
ps aux | grep kaptaind | grep -v grep

# Restart if needed
kaptaind --daemon

# Check active session file
cat .kaptaind/aoc/active.json
```

---

### ❌ "Intercept command failed"

**Symptom:** `kaptaind-cli aoc intercept -- npm test` returns error.

**Debug:**
```bash
# Try running the command directly first
npm test

# If that succeeds, check kaptaind logs
kaptaind-cli status
```

---

### ❌ "Cannot ship, session is corrupted"

**Symptom:** `kaptaind-cli aoc ship` fails with parse error.

**Fix:**
```bash
# Inspect the active session file
cat .kaptaind/aoc/active.json | jq .

# If corrupted, manually repair or cancel
kaptaind-cli aoc cancel

# Restart session
kaptaind-cli aoc start "your feature"
```

---

## 🎓 Next Steps

- 📖 Read the [main README](./README.md) for AoC details
- 🎯 Start your first AoC session
- 🔍 Integrate with your CI/CD pipeline
- 📊 Generate automated release notes

---

**Made with ❤️ by the Kaptaind team**

*Last updated: April 2026 | Version 1.0.0*
