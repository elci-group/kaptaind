# 🚀 Multi-Provider Inference Routing Tutorial

**Version:** `1.0.0` | **Status:** ✅ Stable | **Last Updated:** April 2026

---

## 📋 Table of Contents

1. [Overview](#overview)
2. [What Is Multi-Provider Routing?](#what-is-multi-provider-routing)
3. [Setup Guide](#setup-guide)
4. [Provider Priority & Auto-Detection](#provider-priority--auto-detection)
5. [Configuration](#configuration)
6. [Web Dashboard Integration](#web-dashboard-integration)
7. [Advanced Usage](#advanced-usage)
8. [Troubleshooting](#troubleshooting)
9. [Performance & Costs](#performance--costs)

---

## Overview

✨ **Problem:** Managing multiple LLM providers requires complex fallback logic and environment-specific configuration.

💡 **Solution:** Kaptaind automatically detects which inference providers you have API access to and intelligently routes requests to the best available option—no manual wiring needed.

🎯 **Result:** Better commit messages from Claude or GPT-4o when available, graceful fallback to local Ollama when offline.

---

## What Is Multi-Provider Routing?

### The Challenge

In a typical setup, you might have:
- **Anthropic API** for high-quality inference (when available)
- **OpenAI API** as a backup (if Anthropic is down or rate-limited)
- **Local Ollama** running on your dev machine (always available, zero latency)

Without a unified router, you'd need to:
1. Check which API keys are set
2. Hardcode fallback logic
3. Handle each API's unique request/response format
4. Manage model names across providers

### The Kaptaind Solution

Kaptaind's **Multi-Provider Inference Router** does all of this for you:

```
Environment Variables
        ↓
  Detect Available Providers
        ↓
  Select Best Provider (in priority order)
        ↓
  Resolve Provider-Specific Model
        ↓
  Execute Inference (unified request/response)
        ↓
  Gracefully Fall Back on Error
```

---

## Setup Guide

### Step 1️⃣: Check Your Current Setup

```bash
# See what's currently available
echo "ANTHROPIC_API_KEY: $ANTHROPIC_API_KEY"
echo "OPENAI_API_KEY: $OPENAI_API_KEY"
```

### Step 2️⃣: (Optional) Add Anthropic

Get an API key from [console.anthropic.com](https://console.anthropic.com):

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

### Step 3️⃣: (Optional) Add OpenAI

Get an API key from [platform.openai.com](https://platform.openai.com):

```bash
export OPENAI_API_KEY="sk-..."
```

### Step 4️⃣: (Recommended) Run Ollama Locally

Download [ollama.ai](https://ollama.ai) and run:

```bash
# Terminal 1: Start Ollama
ollama serve

# Terminal 2: Download the default model (one-time)
ollama pull llama3.2
```

### Step 5️⃣: Verify Detection

Start `kaptaind` and check the logs:

```bash
# In daemon mode, check status
kaptaind-cli status

# Look for output like:
# inference provider selected: anthropic, model=claude-haiku-4-5-20251001
```

---

## Provider Priority & Auto-Detection

### Default Priority (from Highest to Lowest)

| # | Provider | Model | When Used |
|---|----------|-------|-----------|
| 1️⃣ | **Anthropic** | `claude-haiku-4-5-20251001` | `ANTHROPIC_API_KEY` set |
| 2️⃣ | **OpenAI** | `gpt-4o-mini` | `OPENAI_API_KEY` set (no Anthropic) |
| 3️⃣ | **Ollama** | `llama3.2` | Always available (local fallback) |

### How Auto-Detection Works

```rust
// Pseudocode
fn resolve_provider() -> Provider {
    if env::var("ANTHROPIC_API_KEY").is_ok() {
        return Anthropic;
    }
    if env::var("OPENAI_API_KEY").is_ok() {
        return OpenAI;
    }
    return Ollama;  // Always succeeds (local)
}
```

### Example Scenarios

#### 🟢 Scenario A: All Three Available
```
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
Ollama: Running on localhost:11434

→ Uses: Anthropic (highest priority)
```

#### 🟡 Scenario B: Only OpenAI & Ollama
```
OPENAI_API_KEY=sk-...
Ollama: Running

→ Uses: OpenAI (Anthropic unavailable)
```

#### 🔴 Scenario C: Only Ollama
```
Ollama: Running on localhost:11434

→ Uses: Ollama (default fallback)
```

#### ⚫ Scenario D: Override Priority
```
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...

kaptaind.toml:
  provider = "openai"  # Force OpenAI, ignore Anthropic

→ Uses: OpenAI (explicit override)
```

---

## Configuration

### Kaptaind Daemon (Rust Backend)

In your **`kaptaind.toml`**:

```toml
[inference]
# Enable/disable AI-generated commit messages
enabled = true

# Provider selection: "auto" | "anthropic" | "openai" | "ollama"
provider = "auto"

# Model selection: "auto" (uses provider-specific default) | explicit model name
model = "auto"

# HTTP timeout for cloud provider requests (seconds)
timeout_secs = 15

# Ollama connection (only used when provider="ollama")
ollama_base_url = "http://localhost:11434"
```

#### 📝 Common Configurations

**Production (Cloud-First)**
```toml
[inference]
enabled = true
provider = "auto"       # Try Anthropic, fall back to OpenAI, then Ollama
model = "auto"
timeout_secs = 30       # More generous timeout for cloud
```

**Development (Offline-First)**
```toml
[inference]
enabled = true
provider = "ollama"     # Always use local Ollama
model = "llama3.2"
timeout_secs = 5        # Fast local response
ollama_base_url = "http://localhost:11434"
```

**Testing (Deterministic)**
```toml
[inference]
enabled = false         # Skip AI inference, use deterministic messages
```

### Web Dashboard (Next.js Frontend)

In **`web/.env.local`**:

```bash
# These are auto-detected from environment, but you can override:

# Anthropic (optional, auto-detected)
ANTHROPIC_API_KEY=sk-ant-...
ANTHROPIC_MODEL=claude-haiku-4-5-20251001

# OpenAI (optional, auto-detected)
OPENAI_API_KEY=sk-...
OPENAI_MODEL=gpt-4o-mini

# Ollama (optional, defaults to localhost)
OLLAMA_BASE_URL=http://localhost:11434
OLLAMA_MODEL=llama3.2
```

The web dashboard uses the same auto-detection logic:

```typescript
// Automatic detection happens at request time
function resolveProvider(): "anthropic" | "openai" | "ollama" {
  if (process.env.ANTHROPIC_API_KEY) return "anthropic";
  if (process.env.OPENAI_API_KEY) return "openai";
  return "ollama";
}
```

---

## Web Dashboard Integration

### 🎨 AI-Generated Features in the Dashboard

#### 1. **Commit Message Generation** (`/dashboard/ai-commits`)
Generates a single-line commit subject (max 72 chars) for a cluster:

```
GET /api/ai/commit-message
{
  "projectId": "my-project",
  "clusterId": "abc123"
}

→ Returns:
{
  "narrative": "feat: add multi-provider inference routing"
}
```

#### 2. **Bump Reasoning** (`/dashboard/bump-reasoning`)
Explains *why* a cluster received a specific version bump:

```
GET /api/ai/bump-reasoning
{
  "projectId": "my-project",
  "clusterId": "abc123"
}

→ Returns:
{
  "reasoning": "Added new API surface for provider selection,
    triggering a Minor version bump. Composite score 0.72 reflects
    significant structural changes and new public exports."
}
```

#### 3. **Changelog Generation** (`/dashboard/changelog`)
Generates a release notes entry for an entire AoC session:

```
GET /api/ai/changelog
{
  "projectId": "my-project",
  "aocId": "release-v2.0"
}

→ Returns:
{
  "changelog": "## What's Changed\n- Inference routing: auto-detects
    Anthropic/OpenAI/Ollama providers..."
}
```

### 🔄 Request Flow

```
Browser Request
  ↓
Next.js API Route (/api/ai/*)
  ↓
Detect Available Provider
  ↓
Call Appropriate Cloud/Local Provider
  ↓
Format Response
  ↓
Return to Dashboard
```

---

## Advanced Usage

### 🎯 Custom Model Selection

Override the provider-specific defaults:

**For Anthropic:**
```toml
[inference]
provider = "auto"
model = "claude-opus-4-6"    # Use a different Claude model
```

**For OpenAI:**
```toml
[inference]
provider = "auto"
model = "gpt-4-turbo"        # Use a more capable model
```

**For Ollama:**
```toml
[inference]
provider = "ollama"
model = "llama2"             # Use a different local model
ollama_base_url = "http://ollama.internal:11434"
```

### 📊 Cost Optimization

**Scenario:** You have both Anthropic and OpenAI, but want to minimize costs.

```toml
[inference]
provider = "openai"          # Use GPT-4o mini (cheaper than Claude Haiku)
```

**Cost Comparison (rough estimates):**
- Claude Haiku: ~$0.80 per million input tokens
- GPT-4o mini: ~$0.15 per million input tokens
- Ollama: $0.00 (runs locally)

### 🌍 Self-Hosted Inference

If you run your own inference server (e.g., vLLM, LocalAI):

```toml
[inference]
provider = "ollama"
ollama_base_url = "http://my-inference-server:8000"
model = "mistral-7b"

# Or override via env:
export OLLAMA_BASE_URL="http://my-inference-server:8000"
```

### 🔐 Using Provider-Specific Features

Each provider has unique capabilities. You can leverage them in custom workflows:

**Anthropic's Extended Thinking (coming soon):**
```rust
// Hypothetical future extension
#[toml]
[inference]
anthropic_extended_thinking = true  # Use extended thinking for complex diffs
```

**OpenAI's Function Calling:**
```rust
// Can be extended to generate structured JSON for analysis
```

### 🚦 Rate Limiting & Retry Logic

Kaptaind automatically handles provider errors:

```rust
// Pseudocode
match call_provider(anthropic) {
    Ok(result) => result,
    Err(_) => {
        // Log error, try next provider
        match call_provider(openai) {
            Ok(result) => result,
            Err(_) => {
                // Fall back to Ollama (always succeeds)
                call_provider(ollama)
            }
        }
    }
}
```

If a cloud provider times out or fails:
- Logs the error
- Falls through to the next available provider
- If all cloud providers fail, Ollama acts as a safety net

---

## Troubleshooting

### ❌ "Inference is disabled"

**Symptom:** No AI-generated commit messages, commits use fallback format.

**Check:**
```bash
# Verify inference is enabled
grep "enabled" kaptaind.toml | grep -i inference
```

**Fix:**
```toml
[inference]
enabled = true
```

---

### ❌ "Wrong provider is being used"

**Symptom:** Expected Anthropic, but got OpenAI or Ollama.

**Debug:**
```bash
# Check which keys are set
env | grep -E "ANTHROPIC_API_KEY|OPENAI_API_KEY"

# Check kaptaind logs
kaptaind-cli status  # Look for "inference provider selected: ..."
```

**Fix (Option 1: Set the keys):**
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

**Fix (Option 2: Override in config):**
```toml
[inference]
provider = "anthropic"  # Force a specific provider
```

---

### ❌ "Anthropic key is set but using OpenAI"

**Likely Cause:** Key is set in a different shell session, not inherited by daemon.

**Debug:**
```bash
# Check what the daemon sees
ps aux | grep kaptaind
# If it's running in a tmux/screen, check that session's env
```

**Fix:**
```bash
# Kill daemon and restart with correct env
kaptaind-cli stop
export ANTHROPIC_API_KEY="sk-ant-..."
kaptaind --daemon
```

---

### ❌ "Ollama timeout: Connection refused"

**Symptom:** Inference times out; logs show "Connection refused" on `localhost:11434`.

**Fix:**
```bash
# Verify Ollama is running
curl http://localhost:11434/api/tags

# If not running, start it:
ollama serve
```

---

### ❌ "Model not found"

**Symptom:** Error: "model `llama3.2` not found"

**Fix (for Ollama):**
```bash
ollama pull llama3.2
```

**Fix (for cloud providers):**
```toml
[inference]
model = "claude-haiku-4-5-20251001"  # Check valid model names on provider docs
```

---

### ❌ "Web dashboard returns 500 error"

**Symptom:** `/api/ai/commit-message`, `/api/ai/bump-reasoning`, or `/api/ai/changelog` return 500.

**Debug:**
```bash
# Check web server logs
tail -50 web/.next/build.log

# Or run in development mode
cd web && npm run dev
# Then trigger the request and see real-time errors
```

**Common Causes:**
- API key is incorrect or expired → Error logged, falls back to next provider
- Ollama is not running → Falls back to deterministic message
- Network timeout → Check `[inference].timeout_secs` in Rust, response timeout in web routes

---

## Performance & Costs

### 🚀 Response Times

| Provider | Latency | Notes |
|----------|---------|-------|
| **Anthropic** | 500ms–2s | Cloud API; varies by load |
| **OpenAI** | 500ms–2s | Cloud API; varies by load |
| **Ollama** | 100–500ms | Local; very fast |

### 💰 Estimated Monthly Cost (at 10 commits/day)

Assuming 500 tokens per inference request:

| Provider | Tokens/Month | Cost |
|----------|--------------|------|
| **Claude Haiku** | 1.5M | ~$1.20 |
| **GPT-4o mini** | 1.5M | ~$0.23 |
| **Ollama** | — | $0.00 |

**Recommendation:** Use Ollama for development, Anthropic for production.

### ✅ Best Practices

- ✅ Set up local Ollama as a safety net
- ✅ Use `timeout_secs = 30` for cloud providers (slower on peak hours)
- ✅ Monitor API usage in provider dashboards
- ✅ Use `provider = "ollama"` in CI/CD (no key management)
- ✅ Cache inference results in `.kaptaind/analysis/` (no re-generation)

---

## 🎓 Next Steps

- 📖 Read the [main README](./README.md) for configuration details
- 🛠️ Set up your preferred provider above
- 📊 Monitor provider costs and response times
- 💬 Share feedback on GitHub Issues

---

**Made with ❤️ by the Kaptaind team**

*Last updated: April 2026 | Version 1.0.0*
