# Adapter 200 — Reconciled Source List (T1–T3)

Phase-0 research deliverable for `docs/planning/ADAPTER_200_ROADMAP.md` §1.
Ranks are a **durable-consensus snapshot**, not a frozen order; recompute quarterly.

## Method

Blend four indices, normalized to 0–1 each, weighted toward developer activity
(the best proxy for "files kaptaind actually analyzes"):

| Index | Weight | Why |
|-------|--------|-----|
| GitHub Octoverse / Linguist | 0.45 | Active repos/PRs/bytes — closest to kaptaind's input |
| Stack Overflow Developer Survey | 0.25 | Working-with adoption |
| TIOBE | 0.20 | Enterprise/legacy signal |
| PYPL | 0.10 | Learning-demand signal |

Blended rank → tier. Disagreement > 30 ranks across indices → manual review.

## Coverage mechanism by tier

| Tier | Ranks | Mechanism (this increment) |
|------|-------|----------------------------|
| **T1 Core** | 1–20 | Built-in adapter (AST where a real parser exists, else structured scanner) |
| **T2 Mainstream** | 21–60 | Built-in structured-regex/scanner adapter |
| **T3 Long tail** | 61–120 | Built-in regex-lite **or** bundled plugin |
| **T4 Tail** | 121–200 | Fallback scanner (0.0) + community plugin — **out of scope here** |

## This increment (T1–T3 wiring)

Wired **16 previously-orphaned adapters** into `register_builtin_adapters`
(`src/diff/lang/adapters/mod.rs`), each with an explicit confidence arm in
`src/diff/lang/mod.rs` and a `LANGUAGE_MATRIX.md` row. Active set: **12 → 28**.

These map onto the blended ranking as follows (confidence = initial, reasoning-based;
pending corpus calibration per ROADMAP §7):

| Language | Blended tier | Confidence | Detection basis |
|----------|--------------|------------|-----------------|
| Java | T1 | 0.85 | `public`/`protected`/`private` modifiers (regex) |
| C# | T1 | 0.85 | `public`/`internal`/`protected`/`private` (regex) |
| C++ | T1 | 0.7 | classes/templates/access specifiers (regex; macro/template-limited) |
| C | T1 | 0.7 | non-`static` functions = external linkage (regex; macro/decl-limited) |
| PHP | T1 | 0.8 | `public`/`private`/`protected function` (regex) |
| Ruby | T1 | 0.75 | `def` public-by-default + `private`/`protected` (regex) |
| Scala | T1/T2 | 0.8 | `def`/`class`/`trait`/`object`, default-public (regex) |
| Dart | T1/T2 | 0.8 | `_leading` private, else public (regex) |
| Perl | T2 | 0.7 | `sub` public-by-default (regex) |
| Elixir | T2 | 0.8 | `def` public / `defp` private (regex) |
| Haskell | T2 | 0.7 | module export list + top-level bindings (regex) |
| Clojure | T2 | 0.75 | `defn` public / `defn-` private (regex) |
| F# | T2 | 0.7 | `let` bindings, default-public in module (regex) |
| OCaml | T2 | 0.7 | `let` bindings; `.mli` interface defines public (regex) |
| Erlang | T2 | 0.8 | explicit `-export([...])` (regex) |
| Lua | T2/T3 | 0.7 | `function` public / `local function` private (regex) |

> Confidence values are **reasoning-calibrated**, not yet corpus-measured. ROADMAP §7
> replaces them with held-out F1 bands as each adapter's fixture corpus lands
> (Phase 1). Until then they are intentionally conservative.

## Behavior change (flag for CHANGELOG)

Wiring these adapters changes analysis for repositories containing these languages:
files that previously hit the fallback line scanner (confidence 0.0, then normalized
by the `_ => 0.75` arm) now resolve to a real adapter with the confidences above. Net
effect is *more accurate* confidence (up for Java/C#/PHP/Scala/Elixir/Erlang/Dart;
down for C/C++/Haskell/Lua/OCaml/Perl/F#). This is intended and matches the breadth
already claimed in `README.md`; it must ship in a **minor** release with a behavior-change
note, never silently in a patch (ROADMAP §10).

## Remaining T1–T3 backlog (not wired this increment)

- **T1 not yet present as even an orphan:** none outstanding — every T1 language now
  has at least a wired scanner. Depth work (AST-grade for C/C++/Java/C#/PHP) is Phase 1.
- **T2/T3 with no adapter file yet** (fallback-only today, candidates for plugin or new
  adapters): ~~SQL~~ (**wired rev 30** — adapter + gold seeds + calibration corpus), GraphQL (schema-level exists via API-surface rules), ~~Terraform/HCL~~ (**wired rev 31** — adapter + gold seeds + calibration corpus),
  ~~Solidity~~ (**wired rev 32** — adapter + gold seeds + calibration corpus), ~~Groovy~~ (**wired rev 33** — adapter + gold seeds + calibration corpus), ~~Julia~~ (**wired rev 34** — adapter + gold seeds + calibration corpus), ~~R~~ (**wired rev 35** — adapter + gold seeds + calibration corpus), ~~Objective-C~~ (**wired rev 36** — adapter + gold seeds + calibration corpus), ~~Zig~~ (**wired rev 37** — adapter + gold seeds + calibration corpus), Nim, Crystal, PowerShell, Visual Basic,
  V, Bicep, Puppet, Fortran, COBOL, Ada, Delphi — tracked in the ROADMAP §9 promotion
  queue; promote by telemetry or demand.

## Refresh

Re-run the blend quarterly; update this file and the ROADMAP §15 snapshot together.
