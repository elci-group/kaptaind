# 🔍 Commit Validation: Fast vs. Consensus Modes Tutorial

**Version:** `1.0.0` | **Status:** ✅ Stable | **Last Updated:** April 2026

---

## 📋 Table of Contents

1. [Overview](#overview)
2. [The Two Modes](#the-two-modes)
3. [Configuration](#configuration)
4. [Fast Mode (Single Provider)](#fast-mode-single-provider)
5. [Consensus Mode (Multi-Model)](#consensus-mode-multi-model)
6. [Choosing Between Modes](#choosing-between-modes)
7. [Advanced Usage](#advanced-usage)
8. [Troubleshooting](#troubleshooting)

---

## Overview

🤖 **Problem:** Single-model inference carries hallucination risk. One model might misinterpret code changes or generate vague commit messages.

💡 **Solution:** Kaptaind now offers two strategies for generating commit messages:

- **Fast Mode** — Single cloud provider (Anthropic → OpenAI → Ollama). Lowest latency, acceptable risk for teams prioritizing speed.
- **Consensus Mode** — Multiple local models polled in parallel; semantic cross-comparison elects the best candidate. Higher latency, lower hallucination risk for teams prioritizing correctness.

🎯 **Key Insight:** Both modes are **opt-in** (inference is disabled by default) and **developer-selectable** via a single config option.

---

## The Two Modes

### ⚡ Fast Mode

```
Single inference call → One model's response → Trust it → Commit
```

**When to use:**
- ✅ Continuous integration with fast feedback needed
- ✅ Team prefers speed over perfect commit messages
- ✅ Running kaptaind in a time-sensitive environment
- ✅ Limited compute resources

**Characteristics:**
- Single HTTP call (cloud or local)
- ~500ms–2s latency (cloud) or ~100–500ms (Ollama)
- 1 chance for hallucination
- Lower cost (one API call)

---

### 🧠 Consensus Mode

```
Spawn N models in parallel → Collect N responses → Semantic comparison → Elect best → Commit
```

**When to use:**
- ✅ Accuracy is paramount
- ✅ Regulatory or compliance requirements
- ✅ High-stakes projects (public APIs, core libraries)
- ✅ Team prefers robustness over speed
- ✅ Local Ollama available with multiple models

**Characteristics:**
- `consensus_models.len()` parallel HTTP calls
- All complete before semantic scoring begins
- ~1–3s latency (parallel overhead minimal)
- `min_agreement` quorum required
- `consensus_threshold` similarity check required
- Graceful fallback to deterministic message if consensus fails

---

## Configuration

### Quick Setup

#### Enable Fast Mode (Default)

```toml
[inference]
enabled = true
validation_mode = "fast"          # Single provider, priority order
provider = "auto"                 # Anthropic → OpenAI → Ollama
```

#### Enable Consensus Mode

```toml
[inference]
enabled = true
validation_mode = "consensus"
consensus_models = ["llama3.2", "mistral", "codellama"]
consensus_threshold = 0.6         # Min mean similarity to elect
consensus_min_agreement = 2       # Min responding models
```

### Full Config Reference

```toml
[inference]
enabled = false                                # true to enable AI-generated commits

validation_mode = "fast"                       # "fast" | "consensus"

# Fast mode settings
provider = "auto"                              # "auto" | "anthropic" | "openai" | "ollama"
model = "auto"                                 # "auto" or explicit model name

# Common settings
timeout_secs = 15                              # HTTP timeout for all calls
ollama_base_url = "http://localhost:11434"   # For Ollama (fast or consensus)

# Consensus mode settings
consensus_models = ["llama3.2"]               # Ollama models to poll
consensus_threshold = 0.6                      # Min mean Jaccard similarity [0.0–1.0]
consensus_min_agreement = 2                   # Min responding models to proceed
```

---

## Fast Mode (Single Provider)

### How It Works

1. Check `provider` setting:
   - If not `"auto"` → use specified provider (e.g., `"anthropic"`)
   - If `"auto"` → detect from env vars: `ANTHROPIC_API_KEY` → `OPENAI_API_KEY` → Ollama

2. Resolve model:
   - If not `"auto"` → use specified model
   - If `"auto"` → use provider default:
     - Anthropic: `claude-haiku-4-5-20251001`
     - OpenAI: `gpt-4o-mini`
     - Ollama: `llama3.2`

3. Single HTTP call with commit context

4. Post-process: first line, truncate to 72 chars

5. On success: use result; on failure: fall back to deterministic message

### Examples

**Example 1: Auto-Detect from Environment**

```toml
[inference]
enabled = true
validation_mode = "fast"
provider = "auto"    # Will detect ANTHROPIC_API_KEY or OPENAI_API_KEY
```

Running with:
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
kaptaind --daemon
# → Uses Anthropic automatically
```

**Example 2: Force OpenAI (Ignore Anthropic)**

```toml
[inference]
enabled = true
validation_mode = "fast"
provider = "openai"  # Explicit override
model = "gpt-4"      # More capable than gpt-4o-mini
```

**Example 3: Local Ollama Only**

```toml
[inference]
enabled = true
validation_mode = "fast"
provider = "ollama"
ollama_base_url = "http://localhost:11434"
```

---

## Consensus Mode (Multi-Model)

### How It Works

1. Check `consensus_models` list (must not be empty)

2. Pre-build the commit context prompt once (reused for all models)

3. Spawn one `tokio` task per model in parallel
   - Each task calls Ollama with a different model
   - Tasks run concurrently; wait for all to complete

4. Collect successful responses (filter out timeouts, errors, empty)

5. Quorum check: `responding >= consensus_min_agreement`
   - If fewer models respond than required → **fallback to deterministic message**

6. Tokenize each candidate (lowercase, remove stop words, split on punctuation)

7. Score each candidate by **mean Jaccard similarity** to all others
   - Candidates that are similar to most others score high
   - Outliers (unique responses) score low

8. Threshold check: `best_score >= consensus_threshold`
   - If best score is too low → **fallback to deterministic message**

9. Elect highest-scoring candidate and commit

### Semantic Similarity: Jaccard Explained

**Jaccard Similarity** measures overlap between token sets:

```
candidates:
  A: "feat: add OAuth2 provider interface"
  B: "feat: implement OAuth2 authentication"
  C: "fix: typo in error message"

tokens (after removing stop words):
  A: {feat, add, oauth2, provider, interface}
  B: {feat, implement, oauth2, authentication}
  C: {fix, typo, error, message}

Jaccard(A, B):
  intersection = {feat, oauth2}             (2 words)
  union = {feat, add, oauth2, provider, interface, implement, authentication}  (7 words)
  similarity = 2/7 ≈ 0.286

Jaccard(A, C):
  intersection = {}                         (0 words)
  union = {feat, add, oauth2, provider, interface, fix, typo, error, message}  (9 words)
  similarity = 0/9 = 0.0

mean_similarity(A) = (Jaccard(A,B) + Jaccard(A,C)) / 2 = (0.286 + 0.0) / 2 ≈ 0.143
mean_similarity(B) = (Jaccard(B,A) + Jaccard(B,C)) / 2 = (0.286 + 0.0) / 2 ≈ 0.143
mean_similarity(C) = (Jaccard(C,A) + Jaccard(C,B)) / 2 = (0.0 + 0.0) / 2 = 0.0

→ A or B elected (roughly equal); C is the clear outlier
```

### Examples

**Example 1: Standard Setup (3 Local Models)**

```toml
[inference]
enabled = true
validation_mode = "consensus"
consensus_models = ["llama3.2", "mistral", "codellama"]
consensus_threshold = 0.6
consensus_min_agreement = 2
```

Behavior:
- Spawns 3 parallel Ollama calls
- Requires at least 2 to succeed (if 1 fails, consensus still proceeds)
- Requires best candidate to score ≥ 0.6 similarity

**Example 2: Strict Consensus (All Must Agree)**

```toml
[inference]
enabled = true
validation_mode = "consensus"
consensus_models = ["llama3.2", "mistral", "codellama"]
consensus_threshold = 0.8      # Require high similarity
consensus_min_agreement = 3    # All must respond
```

Behavior:
- Spawns 3 parallel calls
- If any fails → fallback to deterministic (need all 3)
- If all succeed but best score < 0.8 → fallback

**Example 3: Lenient Consensus (Fast Fail-Safe)**

```toml
[inference]
enabled = true
validation_mode = "consensus"
consensus_models = ["llama3.2", "mistral"]
consensus_threshold = 0.3      # Low threshold; almost always elects
consensus_min_agreement = 1    # Even 1 response is OK
```

Behavior:
- Spawn 2 parallel calls
- If at least 1 succeeds → proceed to scoring
- Almost any agreement (even with just 1 model) passes the threshold
- Effectively "consensus with soft fallback"

---

## Choosing Between Modes

### Decision Matrix

| Factor | Fast Mode | Consensus Mode |
|--------|-----------|-----------------|
| **Latency** | 500ms–2s | 1–3s |
| **Reliability** | Single point of failure | Distributed agreement |
| **Cost** | Lowest (1 call) | Medium (N calls) |
| **Setup Complexity** | Simple | Complex (need N models) |
| **Hallucination Risk** | Moderate | Low |
| **Best For** | Tight loops, CI/CD | Production, compliance |

### Recommended Defaults

**Startup / Development:**
```toml
[inference]
enabled = false                    # Disable until you're confident
```

**Prototyping (Anthropic Available):**
```toml
[inference]
enabled = true
validation_mode = "fast"
provider = "auto"
# ANTHROPIC_API_KEY will auto-detect
```

**Production (High Confidence):**
```toml
[inference]
enabled = true
validation_mode = "consensus"
consensus_models = ["llama3.2", "mistral", "codellama"]
consensus_threshold = 0.6
consensus_min_agreement = 2
```

**Offline / No Keys:**
```toml
[inference]
enabled = true
validation_mode = "consensus"
consensus_models = ["llama3.2"]    # Single local model
consensus_threshold = 0.0           # Always elect (no fallback needed)
```

---

## Advanced Usage

### 🔧 Hybrid: Start with Consensus, Fall Back to Deterministic

The system automatically falls back to deterministic messages if:
- Consensus quorum isn't reached
- Best score is below threshold

This is always safe—kaptaind commits regardless. The deterministic message looks like:

```
kaptaind: Patch -> v0.1.2 [api-stable; paths=3; api_touches=0; deps=0; runtime=0; score=0.15]
```

### 🎯 Model Selection

**For Consensus:**
- **llama3.2** — 2B params, fast, good quality (default)
- **mistral** — 7B params, more capable, slower
- **codellama** — Code-optimized, best for code understanding
- **neural-chat** — Good balance, lightweight
- **orca-mini** — Smaller, very fast

Mix and match based on your hardware:

```toml
consensus_models = ["llama3.2", "neural-chat", "orca-mini"]  # lightweight
consensus_models = ["mistral", "codellama"]                    # more capable
```

### ⏱️ Tuning Thresholds

**Stricter** (fewer fallbacks, fewer commits):
```toml
consensus_threshold = 0.8         # Require near-identical responses
consensus_min_agreement = 3       # All models must succeed
```

**Lenient** (more fallbacks, tries consensus first):
```toml
consensus_threshold = 0.3         # Accept any agreement better than nothing
consensus_min_agreement = 1       # Even single response is OK
```

---

## Troubleshooting

### ❌ "Fast mode: wrong provider selected"

**Symptom:** Expected Anthropic, but got OpenAI or Ollama.

**Debug:**
```bash
echo "ANTHROPIC_API_KEY: $ANTHROPIC_API_KEY"
echo "OPENAI_API_KEY: $OPENAI_API_KEY"
```

**Fix:**
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
kaptaind-cli stop && kaptaind --daemon
```

Or override in config:
```toml
provider = "anthropic"
```

---

### ❌ "Consensus: not enough responses"

**Symptom:** Logs show `insufficient responses; falling back to deterministic`.

**Likely Cause:** Not all models are running or some timed out.

**Debug:**
```bash
# Verify Ollama is running
curl http://localhost:11434/api/tags

# Test one model manually
ollama run llama3.2 "Hello"
```

**Fix:**
```bash
# Lower min_agreement requirement
consensus_min_agreement = 1

# Or ensure models are downloaded
ollama pull mistral
ollama pull codellama
```

---

### ❌ "Consensus: best score below threshold"

**Symptom:** Logs show models returned responses, but no consensus reached.

**Cause:** Models disagreed (generated very different messages), so best score < threshold.

**Examples:**
```
Model A: "feat: add OAuth2 provider"
Model B: "fix: broken imports"
Model C: "refactor: simplify auth logic"

→ All three are different topics
→ Jaccard similarity between any two is low
→ Even best score might be 0.15
→ Threshold 0.6 not met → fallback
```

**Fix (Option 1): Lower threshold**
```toml
consensus_threshold = 0.3         # More lenient
```

**Fix (Option 2): Use better models**
```toml
consensus_models = ["mistral", "codellama"]  # More capable, more agreement
```

**Fix (Option 3): Switch to fast mode**
```toml
validation_mode = "fast"
```

---

### ❌ "Consensus is too slow"

**Symptom:** Each cluster waits 3+ seconds for models to respond.

**Options:**

**1. Reduce number of models:**
```toml
consensus_models = ["llama3.2"]   # Single model (defeats consensus purpose)
```

**2. Use fast mode:**
```toml
validation_mode = "fast"
```

**3. Reduce timeout:**
```toml
timeout_secs = 5                  # Was 15; may cause more failures
```

**4. Switch to local-only Ollama (fast mode with provider="ollama"):**
```toml
validation_mode = "fast"
provider = "ollama"
timeout_secs = 5
```

---

## 🎓 Real-World Scenario

### Team: High-Velocity Startup

**Challenge:** Ship fast, but commits shouldn't be nonsense.

**Setup:**

```toml
[inference]
enabled = true
validation_mode = "fast"
provider = "auto"    # Anthropic if available; otherwise OpenAI; fallback Ollama
```

Environment:
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

**Result:**
- Every commit gets a meaningful subject line
- No latency penalty (< 2 seconds per cluster)
- Low cost (Anthropic Haiku is cheap)
- Fallback to deterministic if API down

---

### Team: Regulated Finance

**Challenge:** Commit messages are audit trail. Must be accurate and trustworthy.

**Setup:**

```toml
[inference]
enabled = true
validation_mode = "consensus"
consensus_models = ["mistral", "codellama", "neural-chat"]
consensus_threshold = 0.7
consensus_min_agreement = 2
```

**Result:**
- If 2+ models agree with high similarity → use consensus message
- If disagreement → deterministic fallback (still auditable)
- No external API dependency (all local, offline-capable)
- Logs all intermediate scores for compliance review

---

## 🎓 Next Steps

- 📖 Read the [main README](./README.md) for `[inference]` config details
- 🔧 Enable either `"fast"` or `"consensus"` mode
- 📊 Monitor logs: `tracing::info!` messages show mode selection and scores
- 🧪 Test with `kaptaind-cli analyze` first (dry-run before daemon)

---

**Made with ❤️ by the Kaptaind team**

*Last updated: April 2026 | Version 1.0.0*
