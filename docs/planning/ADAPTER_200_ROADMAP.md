# Adapter 200 — Enterprise Plan for Language Coverage at Scale

Status: plan (not implementation). Grounded in the audited adapter architecture
(`src/diff/lang/**`) and the `CLAIMS_AUDIT.md` findings (notably F6: 28 adapter files
existed, only 12 were wired). This plan is built so that failure mode cannot recur.

## 0. Thesis — do not hand-write 200 adapters

There are not 200 *popular* languages by any serious measure. The indices (§1) carry a
meaningful signal for ~50 languages; beyond that, per-language share drops below ~0.1%
and the tail is esoteric, historic, or domain-specific. Hand-authoring and maintaining
200 bespoke AST adapters would be low-quality, unmaintainable, and the wrong frame.

The enterprise answer is a **tiered coverage model** with a low marginal cost per
additional language, on top of two capabilities kaptaind already has:

- A universal **fallback line scanner** (confidence 0.0) that gives *baseline* coverage
  for any text file (`src/diff/lang/mod.rs`, `adapter.rs`).
- An external **plugin protocol** (`stdin {"file"}` → `stdout {"symbols":[...]}`,
  `src/diff/lang/plugin.rs`, `[plugins]`) that lets the long tail be covered by
  community/maintainer scripts without core changes.

So "200 languages" means: **guaranteed baseline coverage for all 200 via fallback +
plugin, with first-class adapters tiered by market share**, and a CI gate that proves
every shipped adapter is declared, registered, confidence-tabled, fixture-tested,
benchmarked, and documented.

---

## 1. Source of truth for "top 200 by market share"

No single index is authoritative. Reconcile four, weighting for *developer activity*
(what kaptaind actually analyzes: source files in repos):

| Index | Signal | Bias |
|-------|--------|------|
| **GitHub Octoverse / Linguist** | Active repos, PRs, bytes of code | Open-source heavy — best proxy for "files kaptaind will see" |
| **Stack Overflow Developer Survey** | Working-with / want-to-work-with | Self-selecting, web/dev skew |
| **TIOBE** | Search-engine query volume | Legacy/enterprise skew, lags |
| **PYPL** | Tutorial search volume | Learning-demand skew |

**Method (Phase 0 deliverable):** normalize each index to a 0–1 score, compute a
weighted blend (`0.45 GitHub + 0.25 SO + 0.20 TIOBE + 0.10 PYPL`), rank, and publish the
reconciled list as `docs/planning/adapter-200/SOURCES.md` with raw data under
`docs/planning/adapter-200/data/`. **Refresh quarterly**; ranks drift, the *method* is
the durable artifact. Flag any language whose indices disagree by >30 ranks for manual
review.

**Tier cutoffs (by blended rank):**

| Tier | Ranks | Adapter depth | Confidence band | Target coverage mechanism |
|------|-------|---------------|-----------------|---------------------------|
| **T1 — Core** | 1–20 | Full AST / structured parser (syn-class where available) | 0.85–1.0 | Built-in adapter |
| **T2 — Mainstream** | 21–60 | Structured regex/scanner with version awareness | 0.7–0.9 | Built-in adapter |
| **T3 — Long tail** | 61–120 | Regex-lite + manifest/shebang detection | 0.5–0.75 | Built-in adapter *or* bundled plugin |
| **T4 — Tail** | 121–200 | Fallback scanner (0.0) + community plugin | 0.0–0.5 | Plugin / fallback only |

The 12 currently-shipped adapters already cover most of T1 and part of T2; the first
work is extending T1/T2, not the tail.

---

## 2. Adapter contract (architecture as-is)

Every adapter implements `LanguageAdapter` (`src/diff/lang/adapter.rs:113`):
`name`, `language`, `detect_files`, `parse_ast`, `parse_ast_versioned`, `extract_api`,
`diff_ast`, `detect_breaking_changes`. Detection is path/extension/shebang/filename based
(`detect_files`); parsing is version-aware via LV-SCL (`parse_ast_versioned`,
`version_tag`, `VersionMatch`) with a 1-hour manifest cache. Each adapter emits
`FileParseMetadata { lang, version, parser_used, confidence, version_match }` into every
analysis artifact (`adapter.rs:73`).

Confidence is a single table — `normalize()` in `src/diff/lang/mod.rs:9` — and is the
**only** place a language's reliability weight is set. Adding a language means touching
exactly four places, enforced by the no-orphan gate (§4):

1. `src/diff/lang/adapters/<lang>.rs` — the adapter.
2. `src/diff/lang/adapters/mod.rs` — `pub mod <lang>;` + `registry.register(...)` in
   `register_builtin_adapters`.
3. `src/diff/lang/mod.rs` — a `normalize()` arm with a justified confidence.
4. `LANGUAGE_MATRIX.md` — a row documenting detection, misses, breaking rules.

(Plugins bypass 1–3 by design and are tracked separately under `[plugins]`.)

---

## 3. Per-adapter lifecycle (stage gates)

Each adapter graduates through gates; no gate is skippable.

| Stage | Output | Gate (must pass to advance) |
|-------|--------|------------------------------|
| **Research** | `SOURCES` entry, grammar/visibility rules, manifest/version signals, sample corpus | Visibility model documented; ≥20 representative source files collected (incl. generated/minified/multi-lang) |
| **Conceptualize** | "Public API = X" definition; breaking-change definition; confidence justification | Signed off in design note; confidence band proposed |
| **Design** | Detection rules (ext/shebang/filename), parse strategy (AST vs regex), LV-SCL version detection, edge-case list (§8) | Strategy reviewed against §8 catalog; fallback behavior defined |
| **Implement** | Adapter + `mod.rs` + `normalize()` + matrix row (all four, atomically) | No-orphan gate green (§4); `cargo clippy -D warnings` clean |
| **Test** | Fixture corpus + golden snapshots + property + edge + version-variant tests (§5) | ≥95% fixture recall on the language's corpus; 0 regressions |
| **Benchmark** | Throughput (files/s), p95 parse latency, memory (§6) | Meets tier budget (§6); no >2× regression vs. comparable adapter |
| **Edge-case** | §8 cases for the paradigm encoded as tests | Every applicable §8 class has a passing or documented-xfail test |
| **Evaluate** | Precision/recall/F1 on held-out corpus (§7) | F1 ≥ tier target; confidence calibrated to measured F1 (§9) |
| **Refine** | Fixes from eval; confidence re-table | Re-eval green; matrix updated |
| **Integrate** | Feature-flagged rollout, telemetry, docs, CHANGELOG (§10) | Canary metrics within tolerance; rollback verified |

**Definition of Done (per adapter):** all four integration points in one commit; fixture
corpus + golden snapshots committed; bench recorded; §8 cases addressed; measured F1
recorded next to the confidence in `normalize()`; `LANGUAGE_MATRIX.md` row; `CHANGELOG`
entry; behind a capability flag until canary passes.

---

## 4. No-orphan discipline (closes audit F6)

A single CI lint — `tests/adapter_registry_lint.rs` (or an `xtask`) — asserts, for every
`src/diff/lang/adapters/*.rs` file (excluding `common.rs`/`mod.rs`), that it is:

- declared in `adapters/mod.rs` (`pub mod`), **and**
- registered in `register_builtin_adapters` **or** explicitly allow-listed as
  `#[adapter_lint::orphan_ok]` with a tracking issue, **and**
- present in `normalize()` with a confidence arm, **and**
- present in `LANGUAGE_MATRIX.md`, **and**
- backed by a fixture directory `tests/fixtures/adapters/<lang>/`.

Any new adapter that fails the lint fails CI. This turns the F6 failure (dead files,
doc/code mismatch) into a hard, permanent guard. It also generalizes
`claim_active_adapter_count_regression` from a floor into an exact-set check.

---

## 5. Testing standard

Per language, a fixture corpus `tests/fixtures/adapters/<lang>/` with:

- **positive/** — symbols that *must* be detected (public funcs/types/exports/routes).
- **negative/** — symbols that *must not* be flagged as public (private, internal,
  `_leading`, block-scoped, comments, strings).
- **breaking/** — before/after pairs that are true breaking removals/signature changes.
- **nonbreaking/** — before/after pairs that must NOT be marked breaking (body edits,
  added comments, compatible widened types where the language defines it so).
- **version/** — LV-SCL variants (e.g. Py 3.9 vs 3.10 `match`, Go pre/post-1.18
  generics, TS 3.8 `export type`, Svelte 4 vs 5 runes).
- **edge/** — the applicable §8 classes.

Test types: golden-symbol snapshot tests (deterministic `extract_api` output),
before/after diff tests (added/removed/modified sets), property tests (parsing never
panics on arbitrary input via `proptest`/`cargo-fuzz`), and version-variant tests.
Targets: **panic-free on 100% of fuzz input**, **≥95% recall** on `positive/`,
**0 false-public** on `negative/` (precision gate).

---

## 6. Benchmarking standard

Extend the existing `divan` benches (`tests/benches/bench_adapters.rs`,
`bench_diff.rs`) — do **not** add per-language `[[bench]]` manifest entries; parametrize
by language inside the existing harness to avoid manifest churn.

Per-tier budgets (host-class baseline T1; record absolute numbers + ratios):

| Tier | Parse throughput | p95 parse latency/file | Memory | Detection F1 |
|------|------------------|------------------------|--------|--------------|
| T1 | ≥ 5,000 files/s | ≤ 200 µs | ≤ 2× file size | ≥ 0.95 |
| T2 | ≥ 10,000 files/s | ≤ 100 µs | ≤ 2× file size | ≥ 0.90 |
| T3 | ≥ 20,000 files/s | ≤ 50 µs | ≤ 1.5× file size | ≥ 0.80 |
| T4 | fallback budget | fallback budget | fallback | n/a (0.0) |

Benchmarks run in CI on a pinned runner; regressions >10% throughput or >2× latency
block merge. Corpus-driven F1 is the metric that finally makes audit items **U1** and
**U5** measurable instead of marketing.

---

## 7. Evaluation & confidence calibration (closes audit U1/U5)

Confidence is a *claim about reliability*; it must be earned by measurement. For each
adapter, compute precision/recall/F1 on a held-out corpus disjoint from the training
fixtures. Map measured F1 → confidence band (e.g. F1≥0.97→1.0, 0.93–0.97→0.9,
0.88–0.93→0.85, …) and record the mapping in code next to `normalize()`.
Re-calibrate quarterly against a growing corpus. This keeps the
**confidence-aware stability penalty** (`Sₙ … − w₅·(1−C)`) honest: an over-confident
adapter that mis-detects will inflate stability; calibration prevents that.

---

## 8. Edge-case catalog (paradigm-driven)

Every design stage maps its language to the applicable classes below; each becomes a
test or a documented `xfail` with rationale.

- **Macros & metaprogramming** — Rust `macro_rules!`/proc-macros, C/C++ preprocessor,
  Lisp macros, Template Haskell: define whether expansions are API (usually: not).
- **Generics / templates** — constraints, conditional members, SFINAE/concepts.
- **Visibility subtleties** — `pub(crate)`/`pub(super)`, `internal`/`protected`,
  package-private, `friend`, Python `_leading`/`__mangle`, Ruby `private`/`protected`.
- **Re-exports / barrels** — `pub use`, `export * from`, `__all__`, index barrels;
  decide pass-through vs. counted (current TS counts re-exports as new — document).
- **Multi-language single files** — `.vue`/`.svelte`/`.astro` (script+style+template),
  `.mdx`, JSX/TSX, embedded SQL/GraphQL: route to the right sub-parser.
- **Generated / minified / bundled** — `*.generated.*`, `.min.js`, `dist/`, protobuf/
  OpenAPI codegen: detect and down-weight or skip; never treat generated churn as API.
- **Dynamic / string-based APIs** — `module.exports` computed, Ruby `method_missing`,
  Python `__getattr__`, JS proxies: flag as low-confidence, not false public.
- **Conditional compilation** — `#ifdef`, Cargo features, `//go:build`, `@Conditional`.
- **DSL & config-as-code** — Gradle/Kotlin DSL, Terraform HCL, Helm/YAML, Nix, Bazel.
- **Encodings & comments** — non-UTF8, BOM, mixed line endings, huge single-line files.

---

## 9. Coverage strategy for the long tail (ranks 61–200)

Do **not** author T4 adapters by hand. For 61–200:

1. Ship the **fallback scanner** as the guaranteed baseline (already done).
2. Provide **bundled plugin shims** for the highest-value T3 languages (Lua, Scala,
   Clojure, Haskell, Erlang/Elixir, OCaml, F#, Perl already have draft files — promote
   the viable ones through §3 rather than leaving them orphaned).
3. Publish a **plugin author guide** + a `kaptaind-cli adapter scaffold <lang>` generator
   that emits the four integration points + fixture skeleton + bench stub, so community
   contributions land conformant by construction.
4. Accept community plugins via a conformance suite (the §5 corpus for that language
   must pass) before listing them as "supported".

This is how "200" is honestly reached: ~40 first-class adapters (T1+T2+top T3), the rest
covered by fallback + conformant plugins, with telemetry (§10) surfacing which tail
languages actually appear in the wild to justify promotion.

---

## 10. Integration, rollout, and comms

- **Flagged:** new adapters ship behind `[capabilities]`/`[plugins]` or a per-language
  enable flag until canary telemetry shows detection accuracy within tolerance.
- **Telemetry:** emit per-language detection counts and confidence in `status.json`/
  telemetry; sample real repos to discover which tail languages matter.
- **Semver impact:** adding an adapter can *change* version bumps (more detected API).
  Communicate in CHANGELOG as a behavior change; gate major-rollout behind a minor
  release with migration notes, never silently in a patch.
- **Rollback:** per-language disable flag; registry built from config so a bad adapter
  can be excluded without a rebuild.
- **Docs:** update `LANGUAGE_MATRIX.md`, `README.md` (the §Features list — replacing the
  audited-overstated "19"), `AGENTS.md`, and the man pages in the same PR.

---

## 11. Resourcing & timeline envelope (honest math)

Throughput assumptions (one engineer, full-time, including tests+bench+eval):

| Depth | Per-adapter effort |
|-------|--------------------|
| Full AST (T1) | 3–6 eng-days (depends on parser availability) |
| Structured regex (T2) | 1–3 eng-days |
| Regex-lite / plugin (T3) | 0.5–1.5 eng-days |

Implication: ~40 first-class adapters (T1+T2+top T3) ≈ 60–120 eng-days ≈ **3–6 eng-months**
for a small team, parallelizable by language. The remaining ~160 are covered by fallback
+ conformant plugins on a rolling basis, not on the critical path. Scope honestly:
**do not put 200 hand-written adapters on the roadmap** — put 40 first-class + universal
coverage + a plugin pipeline, with data-driven promotion.

---

## 12. Phase plan with hard gates

- **Phase 0 — Foundation (1–2 wks).** Reconciled source list (§1); no-orphan lint (§4)
  green on the existing tree (retroactively wires/removes the 13 orphans); scaffold
  generator (§9); fixture+bench harness parametrized by language (§5/§6).
  *Gate:* lint fails CI on any undeclared/unregistered/undocumented adapter; U1/U5
  become measurable.
- **Phase 1 — T1 completion (top 20).** Bring every T1 language to AST-grade with
  measured F1≥0.95 and calibrated confidence. *Gate:* all T1 adapters pass §3 DoD;
  coverage of the top-20 by share ≥ 95%.
- **Phase 2 — T2 (21–60).** Structured-regex adapters, F1≥0.90. *Gate:* cumulative
  market-share coverage ≥ 80% of active open-source code.
- **Phase 3 — T3 + tail automation (61–200).** Regex-lite/plugins for high-value T3;
  fallback guarantees the rest; telemetry-driven promotion queue. *Gate:* every ranked
  language has at least fallback/plugin coverage; promotion decisions data-driven.
- **Phase 4 — Calibration & hardening (ongoing).** Quarterly re-rank + confidence
  recalibration + fuzz campaigns + corpus growth. *Gate:* no confidence > measured F1
  band; fuzz panics = 0.

---

## 13. Risk register

| Risk | Mitigation |
|------|------------|
| Rank indices drift / disagree | Method over list; quarterly refresh; disagreement flag |
| Orphan/dead adapters (F6 recurrence) | §4 lint as a hard CI gate |
| Over-confident adapters inflate stability | §7 calibration ties confidence to measured F1 |
| 200-hand-written scope blow-up | Tiered model + fallback/plugin coverage (§0/§9) |
| Adapters change user version bumps | Flagged rollout, CHANGELOG behavior-change notes (§10) |
| Parser availability varies (AST not always possible) | T1 only where a real parser exists; else T2 regex |
| Generated/minified code false-positives | §8 detection + down-weight/skip |
| Bench/flaky CI on shared runners | Pinned runner; ratio-based regression gates |

---

## 14. Success criteria (KPIs), tied to the A+/S rubric

- Cumulative blended-market-share coverage: ≥95% (top-20) → ≥99% (through T3).
- Every shipped adapter: DoD complete, no-orphan lint green, fuzz panic count = 0.
- Detection quality: T1 F1≥0.95, T2≥0.90, T3≥0.80, all measured on held-out corpora.
- Performance: tier budgets (§6) met; no >10% throughput / >2× latency regressions.
- Honesty: U1/U5 from the claims audit become measured artifacts; `README.md`
  adapter-breadth claim matches `register_builtin_adapters` exactly (closes F1/F2/F4).
- Confidence ≤ measured F1 band for 100% of adapters (calibration integrity).

---

## 15. Appendix A — illustrative tier snapshot (reconcile live at Phase 0)

Ranks are an *illustrative durable-consensus* snapshot, not a frozen order; the §1
reconciliation produces the authoritative list. Languages shift between tiers on each
quarterly refresh.

**T1 (top 20, AST-grade):** Python, JavaScript, TypeScript, Java, C#, C++, C, PHP,
Go, Rust, Ruby, Swift, Kotlin, Dart, Scala, R, Objective-C, Shell, Lua, MATLAB.

**T2 (21–60, structured-regex):** Perl, Elixir, Haskell, Clojure, F#, OCaml, Erlang,
Groovy, Julia, Visual Basic, PowerShell, Terraform/HCL, Solidity, Zig, Nim, Crystal,
V, Bicep, Puppet, Fortran, COBOL, Ada, Delphi, ABAP, SAS, Apex, plus markup/query
languages kaptaind already treats as API surface — SQL, GraphQL, Protobuf, OpenAPI,
HTML/CSS, SCSS/Sass/Less, Vue, Svelte, Astro.

**T3 (61–120, regex-lite/plugin):** Nimble, Reason/ReScript, Elm, PureScript, Idris,
Racket, Common Lisp, Scheme, ClojureScript, CoffeeScript, LiveScript, F* , D, Chapel,
Pony, Red, Raku, Tcl, Forth, Factor, J, APL, Smalltalk/Pharo, Standard ML, Agda, Coq,
Lean, VHDL/Verilog, GLSL/HLSL, WGSL, MQL, ABAP CDS, Q/KDB, Jinja, Nunjucks, EJS, etc.

**T4 (121–200, fallback/plugin):** the long tail of esoteric, legacy, educational, and
domain-specific languages (Awk, SED, bc, dc, Logo, Prolog, Mercury, Eiffel, Simula,
Modula-2/3, Oberon, PL/I, Rexx, A+ , B, BCPL, JOSS, MAD, SNOBOL, INTERCAL, Brainfuck,
Whitespace, Befunge, Malbolge — included for completeness of the *ranked 200* deliverable,
covered by fallback; none justify a first-class adapter).

Note: several T3/T4 entries are present to make the "200" enumeration concrete; the
*strategy* deliberately does not build adapters for them. The first Phase-0 research task
replaces this illustrative list with the reconciled, sourced ranking.
