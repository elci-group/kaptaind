# 📦 Bundle Size Scoring Tutorial

**Version:** `1.0.0` | **Status:** ✅ Stable | **Last Updated:** April 2026

---

## 📋 Table of Contents

1. [Overview](#overview)
2. [Core Concepts](#core-concepts)
3. [Setup Guide](#setup-guide)
4. [Configuration](#configuration)
5. [Monitoring & Analysis](#monitoring--analysis)
6. [Best Practices](#best-practices)
7. [Troubleshooting](#troubleshooting)

---

## Overview

📦 **What is Bundle Size Scoring?** Kaptaind can automatically measure your build artifacts and score version bumps based on size changes. Perfect for teams that care about:

- 🌍 **Bandwidth constraints** (mobile apps, edge computing)
- 📊 **Performance metrics** (bundle size = Core Web Vitals)
- ⚡ **Release quality** (prevent accidental bloat)
- 💰 **Cost optimization** (smaller bundles = faster deploys)

🎯 **Example:** A change adds 50KB to your JavaScript bundle → triggers a `Minor` version bump even if no APIs changed.

---

## Core Concepts

### How Scoring Works

Bundle size scoring works in five steps:

```
1. Initial Build (Baseline)
   └─ Run: npm run build
   └─ Measure: dist/ folder size
   └─ Store: .kaptaind/bundle.json

2. Make Changes
   └─ Edit: src/components/Modal.tsx
   └─ Commit: git add . && git commit

3. Analyze Cluster
   └─ Kaptaind detects changes
   └─ Runs: npm run build (again)
   └─ Measures: new dist/ size

4. Compute Score
   └─ Delta = |new_size - old_size| / old_size
   └─ Score = delta, clamped to [0, 1]
   └─ Example: +50KB on 500KB bundle = 0.10 score

5. Weight & Bump
   └─ Include in composite score with other dimensions
   └─ If score > 0.6 → Minor bump
   └─ If score ≤ 0.15 → Patch bump
```

### Scoring Formula

```
bundle_score = |current_size - previous_size| / previous_size

Score → Interpretation
-----    ---------------
0.00–0.05    ✅ Negligible (patch territory)
0.05–0.15    🟡 Minor regression/improvement (minor bump)
0.15–0.30    🔴 Significant bloat (major concern)
0.30+        🚨 Severe regression (investigate!)
```

### Weight in Overall Version Bump

By default, bundle size is **disabled** (`b = 0.0`). When enabled:

```
Composite Score = (s × structural) + (a × api) + (d × deps) + (r × runtime) + (b × bundle)

Where:
  s = 0.35 (structural weight)
  a = 0.30 (API weight)
  d = 0.20 (dependency weight)
  r = 0.15 (runtime weight)
  b = 0.0–1.0 (bundle weight, you choose)
```

**Example:**
```
Structural: 0.20
API:        0.30
Deps:       0.10
Runtime:    0.15
Bundle:     0.10   ← +50KB increase

With b = 0.0 (disabled):
  Composite = 0.35×0.20 + 0.30×0.30 + 0.20×0.10 + 0.15×0.15 + 0.0×0.10
            = 0.07 + 0.09 + 0.02 + 0.02 + 0.00
            = 0.20 → PATCH bump

With b = 0.1 (enabled):
  Composite = 0.35×0.20 + 0.30×0.30 + 0.20×0.10 + 0.15×0.15 + 0.1×0.10
            = 0.07 + 0.09 + 0.02 + 0.02 + 0.01
            = 0.21 → PATCH bump

With b = 0.3 (high priority):
  Composite = 0.35×0.20 + 0.30×0.30 + 0.20×0.10 + 0.15×0.15 + 0.3×0.10
            = 0.07 + 0.09 + 0.02 + 0.02 + 0.03
            = 0.23 → PATCH bump (but closer to minor)
```

---

## Setup Guide

### Step 1️⃣: Identify Your Build Command

What command builds your project?

```bash
# JavaScript/TypeScript
npm run build          # or yarn build, pnpm build, bun build
npm run build:prod
next build
vite build

# Python
python setup.py bdist_wheel
pyinstaller main.py

# Go
go build -o bin/app

# Rust
cargo build --release
```

### Step 2️⃣: Identify Your Output Directory

Where does the build place artifacts?

```bash
# JavaScript frameworks
dist/                  # Vite, Svelte
build/                 # Create React App
.next/                 # Next.js
out/                   # Static export
dist-ssr/              # SPA with SSR

# Python
dist/
build/

# Go/Rust
bin/
target/release/
```

**Kaptaind auto-detects:** `dist/`, `build/`, `.next/`, `out/`. If yours is different, specify it.

### Step 3️⃣: Enable in Configuration

Edit **`kaptaind.toml`**:

```toml
[weights]
s = 0.35  # Structural
a = 0.3   # API
d = 0.2   # Dependencies
r = 0.15  # Runtime
b = 0.1   # Bundle (enable with 0.05–0.3)

[bundle]
command = "npm run build"      # Your exact build command
output_dir = "dist"            # Where artifacts land
```

### Step 4️⃣: Verify Setup

Run an initial dry-run:

```bash
kaptaind-cli analyze

# Output:
# ✓ Structural score: 0.12
# ✓ API score: 0.25
# ✓ Dependency score: 0.05
# ✓ Runtime score: 0.02
# ✓ Bundle score: 0.15
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Composite score: 0.27 → PATCH bump
```

Check the bundle score. If it's reasonable, you're good!

### Step 5️⃣: Monitor First Commit

After enabling, watch your first bundled commit:

```bash
# Make a small change
echo "// test" >> src/app.js

# Kaptaind will:
# 1. Detect the change
# 2. Run: npm run build
# 3. Measure: dist/ size
# 4. Create: .kaptaind/bundle.json (baseline)
# 5. Commit: "Patch -> v0.1.1 [...]"

# Check the artifact
cat .kaptaind/analysis/*.json | jq '.bundle'

# Output:
# {
#   "previous_size_bytes": 125432,
#   "current_size_bytes": 125432,
#   "delta_bytes": 0,
#   "score": 0.0
# }
```

---

## Configuration

### Minimal Setup

For a Next.js project:

```toml
[weights]
b = 0.1

[bundle]
command = "npm run build"
# Auto-detects .next/ ✓
```

### Full Setup

For a custom monorepo:

```toml
[weights]
s = 0.35
a = 0.3
d = 0.2
r = 0.15
b = 0.15  # Bundle is important

[bundle]
command = "npm run build:prod"
output_dir = "packages/web/.dist"
```

### Advanced: Multiple Outputs

If you have multiple artifacts (web + mobile), measure both:

```toml
# Kaptaind currently measures one output directory.
# For multiple outputs, create a wrapper script:

# scripts/measure-bundle.sh
#!/bin/bash
set -e

# Build all targets
npm run build:web
npm run build:mobile

# Measure web bundle
WEB_SIZE=$(du -sb dist/web | cut -f1)

# Measure mobile bundle
MOBILE_SIZE=$(du -sb dist/mobile | cut -f1)

# Report total
echo "Bundle size: $((WEB_SIZE + MOBILE_SIZE)) bytes"

# Use in config:
# [bundle]
# command = "bash scripts/measure-bundle.sh"
# output_dir = "dist"  # Total size
```

### Conditional Bundling

Skip bundling in certain environments:

```toml
[bundle]
command = "npm run build"
output_dir = "dist"

# Coming soon: enable = "!CI"
# For now, disable in CI via env var:
# $ KAPTAIND_BUNDLE_DISABLED=1 kaptaind --daemon
```

---

## Monitoring & Analysis

### 📊 View Bundle History

```bash
cat .kaptaind/bundle.json
```

**Output:**
```json
{
  "measured_at": "2026-04-05T14:22:00Z",
  "size_bytes": 127540,
  "previous_size_bytes": 125432,
  "delta_bytes": 2108,
  "delta_percent": 1.68,
  "output_dir": "dist",
  "artifacts": {
    "dist/app.js": 65200,
    "dist/vendor.js": 42340,
    "dist/app.css": 20000
  }
}
```

### 📈 Track Trends

Create a script to monitor bundle growth over time:

```bash
#!/bin/bash
# scripts/bundle-history.sh

echo "Date,Size (KB),Delta (%)"

for manifest in .kaptaind/analysis/*.json; do
  if jq -e '.bundle' "$manifest" > /dev/null 2>&1; then
    DATE=$(jq -r '.timestamp' "$manifest")
    SIZE=$(jq -r '.bundle.size_bytes' "$manifest")
    DELTA=$(jq -r '.bundle.delta_percent' "$manifest")
    
    echo "$DATE,$((SIZE / 1024)),$DELTA"
  fi
done | sort
```

### 🔍 Analyze Changes by File

```bash
# Which files contribute most to bundle?
cat .kaptaind/bundle.json | jq -r '.artifacts | to_entries | sort_by(-.value) | .[] | "\(.value / 1024 | round)KB\t\(.key)"'

# Output:
# 65KB    dist/app.js
# 42KB    dist/vendor.js
# 20KB    dist/app.css
# 0.5KB   dist/index.html
```

### ⚠️ Regression Detection

Identify commits that increased bundle size:

```bash
# Find all commits with positive bundle score
for manifest in .kaptaind/analysis/*.json; do
  DELTA=$(jq -r '.bundle.delta_bytes' "$manifest")
  if [ "$DELTA" -gt 0 ]; then
    SUBJECT=$(jq -r '.message' "$manifest")
    echo "❌ +$DELTA bytes: $SUBJECT"
  fi
done
```

---

## Best Practices

### ✅ DOs

- ✅ **Enable bundling for user-facing products** (web, mobile, CLI)
- ✅ **Disable bundling for backend/API-only services** (no shipped artifacts)
- ✅ **Monitor trends month-over-month** (early detection of bloat)
- ✅ **Use realistic build commands** (minified, tree-shaken, production-ready)
- ✅ **Set reasonable weight** (`b = 0.05–0.15`; avoid `b > 0.5`)
- ✅ **Test bundling locally before enabling daemon** (verify build succeeds)

### ❌ DON'Ts

- ❌ **Don't use dev builds** (`npm run dev`, unminified)
- ❌ **Don't measure unrelated directories** (vendored code, node_modules)
- ❌ **Don't set b = 1.0** (bundle > all other factors)
- ❌ **Don't ignore large increases** (2–3x jumps often indicate real problems)
- ❌ **Don't disable when bad PR happens** (investigate instead)

### 🎯 Setting the Right Weight

| Project Type | Recommended b | Reason |
|---|---|---|
| **Web SPA** | 0.10–0.15 | Bundle size critical for LCP |
| **Next.js** | 0.05–0.10 | Less critical (streaming, code-splitting) |
| **Mobile SDK** | 0.15–0.25 | Very sensitive to size (download time) |
| **CLI Tool** | 0.05 | Matters, but not critical |
| **Backend API** | 0.0 | Disable (no shipped bundle) |

---

## Troubleshooting

### ❌ "Build command failed; bundling skipped"

**Symptom:** Logs show "Bundle scoring skipped (build failed)"

**Debug:**
```bash
# Run your build command manually
npm run build

# Does it fail?
echo $?
```

**Fix:**
- Debug the build error (missing deps, syntax errors, etc.)
- Once the build succeeds, kaptaind will capture it automatically

---

### ❌ "Output directory not found"

**Symptom:** Error: "Bundle output_dir not found: dist"

**Check:**
```bash
# Run your build and verify output exists
npm run build
ls -la dist/

# Does the directory exist? Is it non-empty?
du -sh dist/
```

**Fix:**
```toml
[bundle]
command = "npm run build"
output_dir = "dist"  # Check this path exists after build
```

Or use a different output directory:
```toml
output_dir = ".next"  # For Next.js
output_dir = "build"  # For CRA
```

---

### ❌ "Bundle size is always 0 or unrealistic"

**Symptom:** `.kaptaind/bundle.json` shows `size_bytes: 0`

**Causes:**
1. Build command doesn't generate output
2. Build command succeeds but with no files
3. Wrong output_dir specified

**Debug:**
```bash
# Step 1: Run build manually
npm run build

# Step 2: Check output directory
ls -la dist/
du -sh dist/

# Step 3: Verify output_dir matches
grep output_dir kaptaind.toml
```

---

### ❌ "Build is too slow; bundling blocks commits"

**Symptom:** Each commit waits for `npm run build` to finish (2–5 min)

**Options:**

**Option 1: Use a faster build target**
```toml
[bundle]
command = "npm run build:fast"  # Faster, less optimized
```

**Option 2: Measure pre-built artifacts**
```toml
[bundle]
command = "true"  # No-op; measure dist/ as-is
# (Run a separate CI build, this just measures)
```

**Option 3: Disable bundling, use CI instead**
```toml
[bundle]
# Disable by removing this section
```

Then measure bundle size in your CI pipeline separately.

---

### ❌ "Bundle baseline is wrong; want to reset"

**Symptom:** `.kaptaind/bundle.json` has stale size from when setup was broken

**Fix:**
```bash
# Delete the baseline
rm .kaptaind/bundle.json

# Next commit will re-baseline
# (First measurement becomes the new baseline)
```

---

## 🎓 Real-World Example

### Scenario: React App Optimization

You're optimizing a Create React App for mobile:

**Initial state:**
```
.cra-app/
├── kaptaind.toml
└── public/
    └── dist/
        ├── app.js        (150KB)
        ├── vendor.js     (280KB)
        └── index.css     (15KB)
```

**Configuration:**
```toml
[weights]
b = 0.15  # Bundle is critical for mobile

[bundle]
command = "npm run build"
output_dir = "build"
```

**Optimization 1: Code splitting**
```bash
# You split large routes with React.lazy()
# Build size: 120KB (was 150KB) → -20% improvement

# Kaptaind measures:
# delta_percent = -20%
# score = 0.20
# Composite with other changes = 0.35 → PATCH bump ✓
```

**Optimization 2: Dependency replacement**
```bash
# You replace moment.js (67KB) with date-fns (12KB)
# Build size: 85KB (was 120KB) → -29% improvement

# Kaptaind measures:
# delta_percent = -29%
# score = 0.29
# Composite > 0.6 → MINOR bump ✓ (celebrated!)
```

**Final state:**
```bash
cat .kaptaind/bundle.json | jq '.'

{
  "measured_at": "2026-04-06T11:30:00Z",
  "size_bytes": 85000,
  "previous_size_bytes": 445000,
  "delta_bytes": -360000,
  "delta_percent": -80.9,
  "output_dir": "build"
}
```

---

## 🎓 Next Steps

- 📖 Read the [main README](./README.md) for bundle scoring details
- 🏗️ Enable bundling in your `kaptaind.toml`
- 📊 Monitor your first week of bundle measurements
- 🎯 Adjust weights based on what matters for your project

---

**Made with ❤️ by the Kaptaind team**

*Last updated: April 2026 | Version 1.0.0*
