# Adapter Calibration — T1–T3 (measured)

Phase-1 calibration artifact for `docs/planning/ADAPTER_200_ROADMAP.md` §7, produced by a
28-worker swarm (corpora) + a central eval harness (measurement). This converts the
claims-audit **U1/U5** items from unverifiable marketing into **reproducible, measured
evidence** — and surfaces a concrete, prioritized adapter bug catalog.

## Method

- **Corpora:** 28 parallel workers each read one adapter (`src/diff/lang/adapters/<lang>.rs`)
  and authored a behavioral fixture corpus under `tests/fixtures/adapters/<lang>/`
  (`positive/`, `negative/`, `breaking/` before→after pairs, `edge/`, `NOTES.md`). Workers
  were fenced to their own directory (no `src/**`, no `cargo`, no git).
- **Eval:** `tests/adapter_calibration.rs` runs each registered adapter over its corpus.
  - `adapters_panic_free_and_resolve_on_corpora` — CI guard (panic-free + ≥28 resolve).
  - `calibration_report` (`--ignored --nocapture`) — emits the table below.
- **What the numbers are:** corpora-qualified smoke observations. Positive files are
  expected to yield ≥1 symbol; negative files to yield 0; breaking pairs record
  `detect_breaking_changes`. **Not** gold-label precision/recall/F1 — corpus quality varies
  by language and some "negatives" intentionally contain constructs that expose known
  false-positives. True F1 needs a hand-labeled held-out corpus (next iteration).

## Measured smoke table (host T1, 2026-07-10) — rev 2 (post-fix)

Rev 1 (8/102 false-pos) → rev 2 (4/102) after three targeted per-adapter fixes
(see "Revision 2" below). Positive detection, breaking, and panic-free bars unchanged.

| lang | pos det/files | neg false-pos/files | breaking true/pairs | parse errs |
|------|---------------|---------------------|---------------------|------------|
| astro | 5/5 | 0/4 | 3/3 | 0 |
| c | 6/6 | 0/4 | 3/3 | 0 |
| clojure | 6/6 | 0/4 | 3/3 | 0 |
| cpp | 5/5 | 0/4 | 3/3 | 0 |
| csharp | 5/5 | 0/3 | 3/3 | 0 |
| dart | 6/6 | 0/4 | 3/3 | 0 |
| elixir | 5/5 | 0/4 | 3/3 | 0 |
| erlang | 5/5 | 3/3 | 3/3 | 0 |
| fsharp | 6/6 | 0/4 | 3/3 | 0 |
| go | 5/5 | 0/4 | 3/3 | 0 |
| haskell | 5/5 | 0/3 | 3/3 | 0 |
| htmlcss | 5/5 | 0/4 | 0/3 | 0 |
| java | 5/5 | 0/4 | 3/3 | 0 |
| javascript | 6/6 | 0/3 | 3/3 | 0 |
| kotlin | 5/5 | 0/3 | 3/3 | 0 |
| lua | 5/5 | 0/3 | 3/3 | 0 |
| ocaml | 5/5 | 0/3 | 3/3 | 0 |
| perl | 5/5 | 0/3 | 3/3 | 0 |
| php | 5/5 | 1/3 | 3/3 | 0 |
| python | 5/5 | 0/4 | 3/3 | 0 |
| ruby | 5/5 | 0/4 | 3/3 | 0 |
| rust | 6/6 | 0/4 | 3/3 | 0 |
| scala | 6/6 | 0/4 | 3/3 | 0 |
| scss | 6/6 | 0/4 | 3/3 | 0 |
| svelte | 5/5 | 0/4 | 3/3 | 0 |
| swift | 4/4 | 0/3 | 3/3 | 0 |
| typescript | 6/6 | 0/4 | 3/3 | 0 |
| vue | 6/6 | 0/4 | 3/3 | 0 |
| **TOTAL** | **149/149** | **4/102** | **81/84** | **0** |

## Headline findings (measured)

- **Robustness: 0 panics / 0 parse errors** across all 28 adapters × ~340 fixtures — the
  §5 panic-free bar is met and now CI-guarded.
- **Positive detection 149/149 (100%).** Every positive fixture produced ≥1 symbol — a
  strong recall signal on the first-pass gold set.
- **False-positives 8/102 → 4/102** after the rev-2 fixes below. The residual 4 are
  `erlang` (3/3) and `php` (1/3), and — on inspection — they are **oracle questions, not
  adapter bugs** (see Revision 2): Erlang's `-module` and PHP's `class` are legitimately
  public surface, so the "negative" labels there are over-strict. 25/28 adapters now have
  clean negatives; the remaining 2 are a labeling decision, not sloppy scanning.
- **Breaking: 81/84 pairs; the 3 "misses" are all `htmlcss`** — its
  `detect_breaking_changes` is unconditionally `false` (style changes are treated as
  non-breaking by design). 27/28 adapters detect removals as breaking.

## Revision 2 — corrected diagnosis + targeted fixes (2026-07-10)

Inspecting the **actual failing fixtures** (not just the totals) overturned the rev-1
assumption that the 8 false-positives were mostly comment/string (X1) issues. They were not:

| Adapter | FP (rev1) | Root cause (fixture-level) | Class | Fix |
|---------|-----------|----------------------------|-------|-----|
| csharp | 2/3 | `internal class Service`, unmodified `class Holder` emitted as public | **X4 visibility** | gate type decls on explicit `public` modifier |
| java | 1/4 | `public interface BlockCommented` inside `/* … */` parsed live | **X1 block-comment** | track `/* */`/`/** */` regions in the parse loop |
| ocaml | 1/3 | `let () = print_endline "boot"` bound the RHS call target | **binding-form** | stop name extraction at `=` in `first_ocaml_name` |
| erlang | 3/3 | `-module(name)` emitted unconditionally | **oracle** | *not a bug* — module name is public in Erlang |
| php | 1/3 | `class Account` emitted (classes are public-by-default) | **oracle** | *not a bug* — class is legitimately public |

**Only 1 of the 8 (java) was an X1 comment issue.** The other clearly-wrong ones were
visibility (csharp) and binding-form (ocaml); 4 (erlang×3, php×1) are the corpus oracle
being stricter than the language. Consequence: **a shared `strip_comments_and_strings`
helper in `common.rs` would have fixed exactly one case while adding cross-adapter risk** —
so it was correctly *not* the next move. The three real fixes were localized to one adapter
each and are covered by their existing unit tests plus the calibration guard.

**Regression caught and fixed during this pass:** the first csharp visibility gate placed
its `continue` outside the type-keyword check, so *every* `public …` line (including
methods) skipped member parsing — csharp breaking fell 3/3 → 1/3. Re-scoping the `continue`
to fire only on an actual type-declaration line restored csharp breaking to 3/3 while
keeping 0/3 false-positives. This is exactly why the breaking pairs are measured, not just
the negative count.

**Confidence re-table recs (rev-1) reconsidered:** the `erlang 0.8→0.7` and
`csharp 0.85→0.8` downgrades were driven by misattributed data — erlang's "dirty" negatives
are the module-oracle question (not sloppiness), and csharp's were a real bug **now fixed**.
Both downgrades are **withdrawn**; confidence is revisited only against correctly-labeled
residuals after a gold-label pass.

## Revision 3 — X2 core: kind-aware `modified` diff (2026-07-10)

Implemented the P0 `modified` signal at the shared-helper level (`common.rs`):

- `basic_diff` now populates `AstDiff.modified` with symbols whose **name** is present on
  both sides but whose **`kind`** changed (e.g. `function`→`class`, `method`→`property`),
  via a new `modified_by_kind` helper (first-write-wins per side for determinism).
- **Behavior-neutral by design:** every adapter's `detect_breaking_changes` keys off
  `removed` (verified: none read `modified`), so versioning/bump decisions are unchanged.
  `modified` makes previously-invisible changes *observable*; it does not change policy.
- All 28 adapters route `diff_ast` through `basic_diff`, so coverage is uniform. The
  `modified: vec![]` literals in rust/scss/vue are **test fixtures** for removal-breaking,
  not production `diff_ast` (those delegate to `basic_diff`); left as-is.

Measured rev 3 (`calibration_report` gained a `modified` column):

| metric | rev 2 | rev 3 |
|--------|-------|-------|
| positive | 149/149 | 149/149 |
| neg false-pos | 4/102 | 4/102 |
| breaking | 81/84 | 81/84 |
| **modified (Σ over breaking pairs)** | n/a | **0** |
| parse errs / panics | 0 | 0 |

The `modified` column is **0 across every adapter** — expected, not a defect: the current
`breaking/` corpora are add/remove/rename (name-keyed), so no same-name-kind-change pair
exists for the helper to catch. The 5 new unit tests in `common.rs` lock the logic
(added/removed unchanged; kind-change → modified, not added/removed; same-kind → not
modified; new-symbol-kind reported; one-sided names ignored).

**Residual — explicit, not hidden:**
1. Demonstrating `modified > 0` end-to-end needs a dedicated **`modified/` corpus category**
   (kind-change pairs). They must *not* live in `breaking/`: a kind-change pair would be a
   `brk_pair` with `detect_breaking_changes == false`, which would *lower* the breaking
   ratio and misrepresent the signal. Adding `modified/` is a mini-swarm (harness column
   already wired; needs per-adapter fixtures).
2. **Policy decision deferred:** whether a kind-change (e.g. `method`→`property`,
   `function`→`class`) is *breaking* is per-language and not universally true
   (`const`→`static` is not breaking). Set per-adapter only against a gold-label corpus.
3. **Signature / return-type / arity changes are still invisible** — `Symbol` carries only
   `name` + `kind`. Surfacing them needs a `Symbol.signature` extension across ~28 adapters
   + serde + tests; that's the larger follow-up that actually moves breaking precision.



## Revision 4 — X2 demonstrated end-to-end; `modified` reachability map (2026-07-10)

A 28-worker swarm authored `tests/fixtures/adapters/<lang>/modified/` (3 kind-change
pairs + 1 same-kind control per adapter, fenced, fixtures-only — no `src/**`, no cargo/git).
The harness now walks `modified/` and reports `modified det/pairs` (a pair "detects" when
`diff.modified` is non-empty). Control pairs count as pairs but must NOT detect.

| lang | modified det/pairs | | lang | modified det/pairs |
|------|--------------------|-|------|--------------------|
| astro | 3/4 | | java | 3/4 |
| c | 3/4 | | javascript | **0/0** (no corpus — unreachable) |
| clojure | 3/4 | | kotlin | 3/4 |
| cpp | 3/4 | | lua | 3/4 |
| csharp | 3/4 | | ocaml | 3/4 |
| dart | 3/4 | | perl | 3/4 |
| elixir | 3/4 | | php | 3/4 |
| erlang | 3/4 | | python | 3/4 |
| fsharp | 3/4 | | ruby | 3/4 |
| go | 3/4 | | rust | 3/4 |
| haskell | 3/4 | | scala | 3/4 |
| htmlcss | **0/4** (unreachable) | | scss | 3/4 |
| | | | svelte | 3/4 |
| | | | swift | 3/4 |
| | | | typescript | **0/4** (unreachable) |
| | | | vue | **0/4** (unreachable) |
| **TOTAL** | | | | **72/108** |

**Reading:** 24/28 adapters land at exactly **3/4** — the 3 kind-change pairs fire and the
same-kind control correctly does *not* (no over-firing). That is the X2 signal working where
the data model allows: **72/108 detected, 0 controls misfiring, 0 panics, 0 parse errors**,
with pos/breaking/negative unchanged (149/149, 81/84, 4/102) — confirming `modified` stays
behavior-neutral for versioning.

**The 4 non-firing adapters are the finding, not a failure** — they precisely map where
`Symbol` (name+kind only) cannot express a same-name/different-kind change:

- **javascript (`0/0`, no corpus):** its worker stopped pre-emptively with a correct proof —
  `name = rest.to_string()` (the whole line remainder after `export `), so `name` embeds the
  kind keyword; swapping the keyword changes `name`, so `modified` is *structurally
  unreachable*. No files written rather than fabricate.
- **typescript (`0/4`):** shares `ts_parse`/`classify_ts_export`, same `name = rest` shape —
  pairs were authored in good faith but the harness proves none fire. Same root cause as JS.
- **vue (`0/4`):** `name` is the macro marker (e.g. `defineProps`), `kind` is props/emits;
  no construct keeps `name` constant while `kind` varies.
- **htmlcss (`0/4`):** shallow style scanner; the paired constructs emit the same/no kind.

This is a measured, concrete justification for the **stable-identifier `name`** (and/or
`Symbol.signature`) follow-up: for JS/TS/Vue the *name itself* must stop embedding the
kind-bearing keyword before `modified` (and any signature-change detection) can fire. The
ts/vue/htmlcss corpora are kept as documented unreachable cases and as ready fixtures once
those adapters are upgraded.

## Revision 5 — stable-identifier `name` (JS) + gold-seed F1 machinery (2026-07-10)

Two workstreams advanced.

### Workstream A — JS stable-identifier `name` (first-adapter proof of the lever)

`javascript.rs` now extracts a stable identifier via a new `export_name(rest)` helper (e.g.
`export class Foo` → name `Foo`, was `class Foo {}`). Same root cause as the rev-4 JS/TS
blocker; fixed for JS only this pass (TS shares `ts_parse` and Vue has its own adapter —
both staged). A fresh `tests/fixtures/adapters/javascript/modified/` corpus (the rev-4 JS
worker had correctly refused to author one) now measures it:

| metric | rev 4 | rev 5 |
|--------|-------|-------|
| javascript modified | **0/0** (unreachable) | **3/4** (3 kind-change + control holds) |
| javascript pos / neg | 6/6 / 0/3 | 6/6 / 0/3 (unchanged) |
| javascript breaking | 3/3 | **2/2** (see tradeoff) |
| TOTAL modified | 72/108 | **75/112** |
| TOTAL breaking | 81/84 | **80/83** (see tradeoff) |
| panics / parse errs | 0 | 0 |

**Breaking tradeoff — measured and transparent:** moving JS `name` to a stable identifier
removed the *accidental* signature-change detection that the old full-line `name` provided.
The JS `signature_change` breaking pair (`connect(host)` → `connect(host, port)`) was only
"breaking" because the signature was mashed into `name` — its own fixture comment admitted
this ("diff is name-based, no arity model"). Under stable names that pair is a *signature/
arity change*, not a removal; it is invisible to **every** adapter today (the X2/signature
residual). It was therefore **relocated** from `breaking/` to a new `signature/` category
(parked for the `Symbol.signature` workstream), so `breaking/` holds only genuine removals.
True-removal detection is intact (JS `remove_function` + `rename_export` = 2/2). The TOTAL
breaking delta (81/84 → 80/83) is the removal of one *false* breaking-true (name-mashing)
plus its pair — measurement is now more accurate, not a regression. The correct fix for
signature/arity changes is the `Symbol.signature` field (below), not name-mashing.

### Workstream B — gold-label seed F1 machinery

First *true* (not corpora-smoke) precision/recall/F1, computed against hand-labeled expected
symbols. Additive; does not change adapter behavior.

- `tests/fixtures/gold/labels.json` — versioned manifest mapping seed files → expected
  public `(name, kind)` symbols (in the adapter's current emission format) + a ground-truth
  note per file.
- `tests/gold_f1.rs` — loads the manifest, runs each referenced adapter, computes per-lang
  TP/FP/FN → precision/recall/F1 (micro-aggregated). Two tests:
  - `gold_seed_resolves_and_rust_baseline` — **CI guard**: seed resolves + panic-free, and
    the known-good (syn-based, pub-only) rust adapter must hold **F1 ≥ 0.99** on the seed.
    Any rust over/under-report fails CI — this pins rust's pub-only contract.
  - `gold_f1_report` (`--ignored --nocapture`) — prints the P/R/F1 table.

Seed result (rust, 2 files / 6 labeled symbols — deliberately tiny and unambiguous):

| lang | TP | FP | FN | precision | recall | F1 |
|------|----|----|----|-----------|--------|----|
| rust | 6 | 0 | 0 | 1.000 | 1.000 | 1.000 |

This validates the machinery end-to-end on a known-good adapter. **It is seed-scoped, not
project-wide** — labels mirror the current emission format, so the seed functions as a
contract/regression oracle; full hand-labeling (python/erlang/php/…) is the ongoing work
that turns this into project-wide F1 and unlocks confidence re-tabling.

**Oracle resolution for the rev-2 residual 4 (gold-informed; to be confirmed when those
gold files land):**
- **erlang 3/3 → real false-positives.** A module with `-export([])` / no export attribute
  has *no usable public API*; ground truth = ∅. The unconditional `-module` symbol is the
  bug → fix is to emit module surface only when ≥1 export exists (not a confidence
  downgrade).
- **php 1/3 → corpus over-strict.** `class Account` is public-by-default in PHP even when its
  members are private; ground truth includes the class → the adapter is correct and the
  negative label is wrong → relabel that fixture as positive.

## Revision 6 — TypeScript stable-identifier `name`; Vue reachability verdict (2026-07-10)

`export_name` was promoted from `javascript.rs` to a **shared** `common.rs::export_name` and
applied in `ts_parse` (export / export-type / hook / type-alias branches). `ts_parse`'s only
caller is `typescript.rs`, so the blast radius is TS-only; JS keeps using the same helper via
`use super::common::*` (the local copy was removed). The rev-4 TS `modified/` corpus (which
the TS worker had authored to keep the identifier constant) now fires with no new fixtures.

| metric | rev 5 | rev 6 |
|--------|-------|-------|
| typescript modified | 0/4 | **3/4** |
| javascript modified | 3/4 | 3/4 (unchanged) |
| typescript breaking | 2/3 | **2/2** (one more signature pair parked) |
| TOTAL modified | 75/112 | **78/112** |
| TOTAL breaking | 80/83 | **79/82** |
| pos / neg / panics | 149/149 / 4/102 / 0 | 149/149 / 4/102 / 0 |

**Same breaking mechanism as rev 5:** TS `change_signature` (`greet(name)` →
`greet(name, loud)`) was only "breaking" via name-mashing; under stable names it is a
signature/arity change (invisible to all adapters). Parked `breaking/` → `signature/`, so TS
`breaking/` is now the two genuine removals (`remove_export`, `rename_export`) = 2/2. The
TOTAL breaking move (80/83 → 79/82) is a second *false* breaking-true removed; breaking is
now measured over real removals only.

**Vue verdict — parked, not fabricated:** Vue emits `name = <full line>` with `kind` ∈
{props, emits, expose} chosen by which macro substring the line contains. There is no
per-declaration identifier to hold constant across a kind change, and props/emits/expose are
*distinct declarations*, not the same symbol changing kind — so `modified` is structurally
ill-posed at the current macro granularity. Expressing Vue `modified` would require
redefining Vue symbols at the per-prop / per-event level (a deeper model change, not a name
fix). The rev-4 Vue `modified/` corpus (0/4) is kept as a documented unreachable case; no new
Vue corpus was authored, matching the rev-4 JS worker's honest-stop precedent.

The two parked `signature/` pairs (JS `signature_change`, TS `change_signature`) are now
concrete, ready fixtures for the `Symbol.signature` workstream — the clear next lever and the
one that actually moves breaking precision.

## Revision 7 — signature/arity changes detected as `modified` (2026-07-10)

The ceiling-mover, landed as an **additive side-channel** so it touches no existing `Symbol`
literal (every `AstRepresentation` is built with `..Default::default()`, so the new field is
inert until an adapter opts in):

- `adapter.rs::AstRepresentation` gained `signatures: HashMap<name, raw_signature>` (default
  empty; not serialized — `AstRepresentation` is not part of analysis artifacts, so no
  artifact-shape change).
- `common.rs::basic_diff` / `modified_by_kind` now flags a same-name symbol as `modified`
  when **kind differs OR both sides recorded a differing `signature`**. Adapters that leave
  `signatures` empty are byte-for-byte unaffected (kind signal alone decides).
- `javascript.rs` and `ts_parse` (TS) record `signatures[name] = <export-line remainder>` for
  `export …` symbols — enough to distinguish arity / parameter / return-text changes while
  keeping body-only edits invisible (control pairs hold).
- The calibration harness walks a new `signature/` corpus category and reports
  `signature det/pairs`. Two `common.rs` unit tests pin the semantics (signature-change →
  `modified`; one-sided signature → ignored).

The two parked pairs now fire **as `modified`** (not as breaking — policy still deferred):

| metric | rev 6 | rev 7 |
|--------|-------|-------|
| javascript signature | n/a | **1/1** (`connect(host)` → `connect(host, port)`) |
| typescript signature | n/a | **1/1** (`greet(name)` → `greet(name, loud)`) |
| **TOTAL signature** | n/a | **2/2** |
| TOTAL modified | 78/112 | 78/112 (signature pairs live in `signature/`, not `modified/`) |
| TOTAL breaking | 79/82 | 79/82 (**neutral** — signature changes are `modified`, not `removed`) |
| pos / neg / panics | 149/149 / 4/102 / 0 | unchanged |
| lib tests | 439 | **441** (+2 signature unit tests) |

This proves signature/arity detection end-to-end while **deliberately not** forcing a
versioning decision: whether a given signature change is *breaking* is the deferred,
per-language policy call (e.g. adding a required param is breaking; adding an optional one
may not be). Making `modified`-via-signature break is a one-line policy change per adapter,
gated behind the gold-label corpus so it isn't done blind.

**Residuals (explicit):**
1. Signatures are populated only for **JS/TS `export …` symbols**; the other 26 adapters
   leave the map empty (inert). Generalizing is per-adapter opt-in work, staged.
2. The stored signature is the **raw export-line remainder** — coarse but sufficient for
   arity/parameter/return-text deltas. A structural signature type (parsed args/return) is a
   refinement, not a prerequisite.
3. **Breaking policy for signature changes is still deferred** (needs gold labels); the
   signal is measured, not enacted.

## Revision 8 — signature side-channel generalizes to bare-identifier adapters (java) (2026-07-10)

`java.rs` now records `signatures[name] = <method signature>` (from the first `(`, trailing
`{` stripped) for public methods. Java is the right target: its `name` is a bare identifier
(`add`), so before rev 8 an arity change was invisible (same name, same kind). A new
`tests/fixtures/adapters/java/signature/` pair (`add(int a)` → `add(int a, int b)`) confirms
the side-channel works beyond JS/TS:

| metric | rev 7 | rev 8 |
|--------|-------|-------|
| java signature | 0/0 | **1/1** |
| javascript / typescript signature | 1/1 / 1/1 | 1/1 / 1/1 (unchanged) |
| **TOTAL signature** | 2/2 | **3/3** |
| java breaking / modified | 3/3 / 3/4 | 3/3 / 3/4 (unchanged) |
| TOTAL breaking / modified | 79/82 / 78/112 | 79/82 / 78/112 (neutral) |
| pos / neg / panics | 149/149 / 4/102 / 0 | unchanged |

**Important targeting correction (plan said "rust"; the data says otherwise):** the
signature side-channel is only needed for adapters whose `name` does **not** already carry
the signature. Two families exist:

- **Name-mashing adapters (signature already in `name`) — do NOT need the side-channel:**
  `rust` (`name = "add(a, b)"`), `python` (`name = rest` incl. args), `erlang` (`name/arity`),
  `haskell`. These already detect signature/arity changes — as `removed`+`added` → **breaking**
  (which is the desired behavior, since signature changes are breaking there). Giving them a
  stable `name` + side-channel would *move* signature changes from breaking to neutral
  `modified` — a precision **regression** unless/until signature→breaking policy is set. So
  they are correctly left as-is.
- **Bare-identifier adapters (signature invisible) — the side-channel's real targets:**
  `java` (done), plus `csharp`, `php`, `go`, `kotlin`, `swift`, `scala`, `dart`, `ruby`,
  `lua`, `ocaml`, `fsharp`, `clojure`, `elixir`, `perl`, `c`, `cpp`, and the web/macro
  adapters. These need `signatures` populated (and `signature/` corpus pairs) to surface
  signature changes as `modified`.

This keeps breaking behavior correct everywhere: name-mashing adapters keep their
signature-as-breaking behavior; bare-identifier adapters gain `modified` (neutral) until the
gold-gated breaking policy is decided.

## Revision 9 — csharp signatures; pattern established (2026-07-10)

`csharp.rs` records `signatures[name]` as the **balanced parameter list `( … )`** for public
methods (body-independent, so expression-bodied methods don't leak the body). A new
`tests/fixtures/adapters/csharp/signature/` pair (`Add(int a)` → `Add(int a, int b)`) fires:

| metric | rev 8 | rev 9 |
|--------|-------|-------|
| csharp signature | 0/0 | **1/1** |
| **TOTAL signature** | 3/3 | **4/4** (js / ts / java / csharp) |
| csharp breaking / modified | 3/3 / 3/4 | unchanged |
| TOTAL breaking / modified | 79/82 / 78/112 | neutral |
| pos / neg / panics / lib | 149/149 / 4/102 / 0 / 441 | unchanged |

The signature population is now a **repeatable, behavior-neutral recipe** per bare-identifier
adapter: import `HashMap`, build the map, capture the signature in the function/method
branch, add it to the `AstRepresentation` literal, plus a `signature/` corpus pair. It only
adds `modified` detection (breaking untouched). The remaining bare-identifier targets
(`php`, `go`, `kotlin`, `swift`, `scala`, `dart`, `ruby`, `lua`, `ocaml`, `fsharp`,
`clojure`, `elixir`, `perl`, `c`, `cpp`, web adapters) are the same mechanical change — the
`src/**` part is sequential/reviewed; the `signature/` corpus pairs are swarm-safe once a
given adapter records signatures.

## Revision 10 — php signatures; go/kotlin reclassified as name-mashing (2026-07-10)

`php.rs` records `signatures[name]` as the **balanced parameter list `( … )`** for top-level
`function` and `public function` methods (body-independent; the `{ … }` body and any
`: returnType` are not captured). A new `tests/fixtures/adapters/php/signature/` pair
(`add(int $a)` → `add(int $a, int $b)`) fires.

Reading `go.rs` / `kotlin.rs` before editing **corrected the forward plan**: both are
**name-mashing** adapters — `name` is the full `func ` / `fun ` remainder (parameters **and**
return type), so an arity change already surfaces as `removed`+`added` → **breaking**, which
is the desired behavior (adding a required parameter breaks every call site in Go and Kotlin).
They are therefore moved out of the signature target list into the name-mashing skip set
alongside rust/python/erlang/haskell; stabilizing their `name` would *regress* breaking
precision. Verified, not assumed.

| metric | rev 9 | rev 10 |
|--------|-------|--------|
| php signature | 0/0 | **1/1** |
| **TOTAL signature** | 4/4 | **5/5** (js / ts / java / csharp / php) |
| php breaking / modified | 3/3 / 3/4 | unchanged |
| TOTAL breaking / modified | 79/82 / 78/112 | neutral |
| pos / neg / panics / lib | 149/149 / 4/102 / 0 / 441 | unchanged |
| go / kotlin | targeted | **reclassified name-mashing → skip** |

Remaining bare-identifier signature targets: `swift`, `scala`, `dart`, `ruby`, `lua`,
`ocaml`, `fsharp`, `clojure`, `elixir`, `perl`, `c`, `cpp`, web adapters. The `src/**`
recipe stays sequential/reviewed (read each parse body first — name-mashing adapters are
skipped, not forced); `signature/` corpus pairs stay swarm-safe per adapter that already
records signatures.

## Revision 11 — scala + dart signatures (2026-07-10)

`scala.rs` records `signatures[name]` for `def` (the text after `def `, name extracted via the
existing `extract_identifier` which stops at `(`); `dart.rs` records `signatures[name]` for
top-level functions (name via `extract_top_level_function_name` + `clean_identifier`). Both use
the shared balanced-`( … )` helper shape (body-independent). New
`tests/fixtures/adapters/{scala,dart}/signature/` pairs (`add(a: Int)` → `add(a: Int, b: Int)`
and `int add(int a)` → `int add(int a, int b)`) fire.

| metric | rev 10 | rev 11 |
|--------|--------|--------|
| scala signature | 0/0 | **1/1** |
| dart signature | 0/0 | **1/1** |
| **TOTAL signature** | 5/5 | **7/7** (js / ts / java / csharp / php / scala / dart) |
| scala / dart breaking / modified | 3/3 / 3/4 | unchanged |
| TOTAL breaking / modified | 79/82 / 78/112 | neutral |
| pos / neg / panics / lib | 149/149 / 4/102 / 0 / 441 | unchanged |

Remaining bare-identifier signature targets: `ruby`, `lua`, `ocaml`, `fsharp`, `clojure`,
`elixir`, `perl`, `c`, `cpp`, web adapters. Read each parse body first — name-mashing adapters
are skipped, not forced (the go/kotlin/swift lesson).

## Revision 12 — ruby + lua signatures (2026-07-10)

`ruby.rs` records `signatures[name]` for `def` methods (name via the existing
`rest.split(['(', ' ', ';'])`); `lua.rs` records `signatures[name]` for module functions
(name via `extract_function_name`, e.g. `M.add`). Both use the shared balanced-`( … )` helper
shape (body-independent; a Ruby `def` without parens records no signature). New
`tests/fixtures/adapters/{ruby,lua}/signature/` pairs (`def add(a)` → `def add(a, b)` and
`function M.add(a)` → `function M.add(a, b)`) fire.

| metric | rev 11 | rev 12 |
|--------|--------|--------|
| ruby signature | 0/0 | **1/1** |
| lua signature | 0/0 | **1/1** |
| **TOTAL signature** | 7/7 | **9/9** (js / ts / java / csharp / php / scala / dart / ruby / lua) |
| ruby / lua breaking / modified | 3/3 / 3/4 | unchanged |
| TOTAL breaking / modified | 79/82 / 78/112 | neutral |
| pos / neg / panics / lib | 149/149 / 4/102 / 0 / 441 | unchanged |

Remaining paren-recipe targets: `clojure`, `elixir`, `perl`, `c`, `cpp`, web adapters (verify
each — Clojure `defn` uses `[…]` arg vectors, not parens, so it likely needs a custom helper).
Custom-helper sub-track (whitespace / non-paren signatures): `ocaml`, `fsharp` (`let x y = …`),
and likely `clojure` (`defn […]`).

## Revision 13 — c + cpp + perl signatures; paren recipe exhausted (2026-07-10)

`c.rs::c_parse`, `cpp.rs::extract_function_definition`'s call site, and `perl.rs`'s `sub` branch
now record `signatures[name]` as the balanced parameter list `( … )`. All three use the shared
paren-helper shape (body-independent). Perl only records for signature-style subs
(`sub add($a)`); classic `sub foo {` is a graceful no-op. C++ only emits definitions, so the
corpus uses `int add(…) { … }`. New `tests/fixtures/adapters/{c,cpp,perl}/signature/` pairs
fire.

| metric | rev 12 | rev 13 |
|--------|--------|--------|
| c / cpp / perl signature | 0/0 | **1/1** (each) |
| **TOTAL signature** | 9/9 | **12/12** (js / ts / java / csharp / php / scala / dart / ruby / lua / c / cpp / perl) |
| c / cpp / perl breaking / modified | 3/3 / 3/4 | unchanged |
| TOTAL breaking / modified | 79/82 / 78/112 | neutral |
| pos / neg / panics / lib | 149/149 / 4/102 / 0 / 441 | unchanged |

The **balanced-paren recipe is now exhausted** across every bare-identifier paren adapter.
Remaining signature work is the **custom-helper track** (ocaml/fsharp whitespace-arg
`let x y = …`; clojure `[…]` arg-vector) — different helper shapes, own corpus, separate slice.
Web adapters (vue/svelte/astro/scss/htmlcss) have no function-signature concept at macro
granularity (same structural reason Vue is parked for `modified`); signature is N/A there.

## Revision 14 — clojure `[…]` arg-vector signatures; custom-helper track opened (2026-07-10)

`clojure.rs`'s `(defn ` branch now records `signatures[name]` as the balanced **argument vector
`[ … ]`** via a new `clojure_signature` helper. This is the first non-paren signature: Clojure's
first `(` on a `defn` line is the *body* (`(+ x 1)`), so the shared paren helper would capture
body and false-modify — the bracket scan stops at the matching `]` and stays body-independent.
A new `tests/fixtures/adapters/clojure/signature/` pair (`(defn add [a] …)` →
`(defn add [a b] …)`) fires.

| metric | rev 13 | rev 14 |
|--------|--------|--------|
| clojure signature | 0/0 | **1/1** |
| **TOTAL signature** | 12/12 | **13/13** (js / ts / java / csharp / php / scala / dart / ruby / lua / c / cpp / perl / clojure) |
| clojure breaking / modified | 3/3 / 3/4 | unchanged |
| TOTAL breaking / modified | 79/82 / 78/112 | neutral |
| pos / neg / panics / lib | 149/149 / 4/102 / 0 / 441 | unchanged |

Custom-helper track remaining: `ocaml` / `fsharp` (whitespace-arg `let add x y = …` — capture
the tokens between the binding name and `=`). `defmacro` is the same `[…]` shape and is left as
an identical one-line follow-up.

## Revision 15 — ocaml + fsharp whitespace-arg signatures; side-channel complete (2026-07-10)

`ocaml.rs`'s `let` branch and `fsharp.rs`'s `let`/`val` (`kind == "value"`) call site now record
`signatures[name]` as the **whitespace-separated argument tokens between the binding name and
`=`** (`ocaml_signature` / `fsharp_signature`; body-independent; value bindings like `let x = 1`
record no signature, graceful). F# computes the signature on the attribute-stripped line so
`[<…>]` prefixes don't shift the name. New `tests/fixtures/adapters/{ocaml,fsharp}/signature/`
pairs (`let add x = x` → `let add x y = x + y`) fire.

| metric | rev 14 | rev 15 |
|--------|--------|--------|
| ocaml / fsharp signature | 0/0 | **1/1** (each) |
| **TOTAL signature** | 13/13 | **15/15** (js / ts / java / csharp / php / scala / dart / ruby / lua / c / cpp / perl / clojure / ocaml / fsharp) |
| ocaml / fsharp breaking / modified | 3/3 / 3/4 | unchanged |
| TOTAL breaking / modified | 79/82 / 78/112 | neutral |
| pos / neg / panics / lib | 149/149 / 4/102 / 0 / 441 | unchanged |

The signature side-channel is now **complete** across every bare-identifier adapter: 12 on the
balanced-paren recipe + 3 on custom helpers (clojure `[…]`; ocaml/fsharp whitespace-arg). The
remaining signature-adjacent work is **robustness** (a second, distinct pair per adapter so
`modified` is not a single `add_param` artifact) and the deferred, gold-gated **breaking policy**
for signature changes (item 11). Name-mashing adapters and web adapters remain correctly out of
scope (breaking-via-name / no-function-signature respectively).

## Revision 16 — precision: php negative relabel; erlang module-gating attempted & reverted (2026-07-10)

Two targeted moves against the only known false-positive class (TOTAL 4/102):

1. **php negative relabel (kept).** `php/negative/private_protected.php` declared a public
   `class Account` with only private/protected members. PHP classes have no visibility modifier
   and ARE public surface (the class is instantiable; only its members are private), so the
   file was an **over-strict negative**, not a true false-positive. Relabeled by moving it to
   `php/positive/class_with_private_members.php` (the class is detected; private members are
   not). php negative 1/3 → **0/2**; php positive 5/5 → **6/6**; no code change, no side effects.

2. **erlang module-gating (attempted → REVERTED).** The 3/3 erlang false-positives all came
   from the unconditional `-module(name)` emission in no-export / empty-export modules
   (`no_export_attribute`, `private_functions` `-export([])`, `comments_only`). A deferred
   "emit module only when ≥1 export" change removed all 3 FPs — but **regressed `modified`
   3/4 → 1/4**: the `module_to_macro` and `module_to_record` kind-change pairs rely on a
   no-export `-module(name)` as their left side; with gating, that left side vanishes and the
   same-name kind change is no longer pairable (surfaces as `added`). A modified regression is
   not an acceptable trade for a negative reduction, so the change was reverted. erlang stays at
   its documented 3/3.

| metric | rev 15 | rev 16 |
|--------|--------|--------|
| php negative | 1/3 | **0/2** (relabel) |
| erlang negative | 3/3 | 3/3 (gating reverted — see #2) |
| **TOTAL negative** | 4/102 | **3/101** |
| erlang modified | 3/4 | 3/4 (regression caught + reverted) |
| **TOTAL modified** | 78/112 | 78/112 (no regression) |
| php positive | 5/5 | 6/6 |
| **TOTAL positive** | 149/149 | 150/150 |
| breaking / signature / lib / panics | 79/82 / 15/15 / 441 / 0 | unchanged |

Net: TOTAL negative 4/102 → **3/101** via a clean corpus relabel, **no code regression**. A
proper erlang fix is **deferred**: it requires adopting a "module-without-export is not public
surface" semantic AND co-redesigning the `module_to_*` `modified/` pairs to always-public
kinds (record/macro/exported-function), then re-measuring — a deliberate, separately-verified
slice, not a drive-by.

**Follow-up (rev 16b, corpus-coupling finding):** reading all four erlang `modified/` pairs
(`module_to_record`, `module_to_macro`, `record_to_macro`, `control`) showed the deferral is
deeper than a corpus tweak. Every `.erl` file has a `-module` line, and under gating *any* file
with ≥1 export emits its module — so module emission and function-export are **file-coupled**.
To light up the before-side module you must add an export; that same export on the after side
lights up the after-side module too, putting `module <name>` on BOTH sides and masking the
same-name kind change. The only always-public (ungated) kinds are `record` and `macro`, so
`record_to_macro` is the *only* clean kind-change that survives gating; `module_to_record` and
`module_to_macro` cannot. A real fix therefore needs a **model change** — treat `-module` as a
namespace/container (not a diff-able bare symbol) and qualify surface by module — not a corpus
or confidence tweak. Status quo (unconditional module, 3/3 FP, modified 3/4) is the strictly
better local point until that model change is scoped.

## Cross-cutting failure classes (deduplicated, prioritized)

These recur across most regex adapters and drive the bulk of false-positives / weak
breaking. Fixing them at the shared-helper level (`common.rs`) lifts many adapters at once.

| # | Class | Effect | Affected (examples) | Priority |
|---|-------|--------|---------------------|----------|
| X1 | **No comment/string stripping** | false-public symbols from `//`, `/* */`, `"""`, heredocs, POD | almost all regex adapters (csharp, java, php, ocaml, swift, kotlin, scss, vue, svelte, astro, …) | **P0** |
| X2 | **Name-only diff; `modified` never set** | signature / return-type / arity / kind changes invisible to breaking | nearly all (only Rust kind-keys a subset) | **P0** |
| X3 | **No generated/minified down-weight or skip** | generated/codegen/`*.min.*` parsed as live API; one-line bundles garbled | all (roadmap §8) | P1 |
| X4 | **Modifier / keyword-order false negatives** | `public static/final/override`, `abstract/open class`, `inline fun` dropped | java, csharp, kotlin, swift, php, scala | P1 |
| X5 | **Scope-blindness** | local/member/block-scoped decls flagged as public | kotlin, scala, ocaml, lua, erlang, c, cpp | P1 |
| X6 | **No preprocessor / conditional-compilation** | `#if false`, `#[cfg]`, `//go:build` symbols emitted | c, cpp, csharp, rust, go | P2 |
| X7 | **Extension match case-sensitive** | `.CLJ`/`.CS`/`.CPP` missed | clojure (+others) | P2 |

## Per-language highlights (source-derived; full detail in each `NOTES.md`)

- **rust** (1.0, syn): strong. Gaps: `const`/`static` removal not breaking; associated
  consts/types on impls not emitted; `impl Trait for &T` (non-path self) → 0 methods;
  cfg-gated items always emitted; arg/return-type-only changes invisible (X2).
- **go**: receiver methods missed (`func (s *S) Start()`); `const`/`var` never emitted;
  grouped `type (…)` missed; generics gated behind versioned parse.
- **swift / kotlin / java / csharp / php / scala**: modifier-order false negatives (X4);
  comment/string false-positives (X1) — confirmed in csharp/java/php/ocaml smoke;
  kotlin/scala scope-blind (X5); csharp ignores type visibility + `#if`; php backed-enum
  name and `<?php ?>` boundary issues; scala misses `case object`, `val`/`var`/`type`.
- **typescript / javascript**: route-export over-match via `contains()` (X1); hook
  double-count; `.d.ts` scanned as source; multi-line `export\nfunction` missed; barrels
  counted as new; CommonJS/dynamic exports invisible (js).
- **vue / svelte / astro**: macro substring false-positives in comments/strings (X1);
  over-broad breaking on prop-line edits (X2); astro ignores export removal for breaking;
  svelte `$state`/`$derived` invisible; minified single-line missed.
- **scss**: `@use` dead (only `@forward` emitted); `@function` not detected; css-var
  removal not breaking (inconsistent); value-in-name → value change looks breaking (X2).
- **htmlcss**: `detect_breaking_changes` is **hard-coded `false`** (the 81/84 cause); no
  real HTML parse; Allman/brace-next-line selectors missed. *(By design for style churn —
  document explicitly.)*
- **c / cpp**: function-like macros, pointer-returns, multi-line sigs missed (c);
  cpp emits forward-decls, ignores visibility, no `enum` kind, `extern "C"` skipped,
  overloads collapse; both flag `static` as public (X5), no comment strip (X1).
- **clojure**: `^:private` emitted as public literally named `^:private`; `(comment …)`
  not understood; `defmulti`/`defmethod`/`deftype` untracked; case-sensitive ext (X7).
- **haskell**: export lists ignored (over-report); `type/data family` name=`family`;
  infix operators missed; `pattern` mis-reported; `.lhs` prose scanned raw.
- **erlang**: multi-line `-export` missed; multi-arg guarded clauses fail `ends_with(')')`;
  naive arity on tuples/maps; `-record`/`-define` always public (→ 3/3 neg false-pos).
- **elixir**: (corpus clean) review `defp`/macro/protocol edges in NOTES.md.
- **lua**: module-export hard-coded to `M.`; `M["x"]`/computed keys invisible; `--[[ ]]`
  leaks; local-never-returned members over-detected.
- **ocaml**: commented decl false-positive (X1); local `let` over-detected (X5); only 5
  prefixes recognized; `.mli` hiding ignored — confirmed 1/3 neg false-pos.
- **perl**: `_leading` private ignored; POD/heredoc false-positives; `use constant { }`
  hash-form → 0; dynamic APIs (typeglob/eval/AUTOLOAD) invisible.
- **fsharp / dart / ruby / python**: corpora clean in smoke; see NOTES.md for residual
  edges (e.g., python `__all__`/dunder, ruby `private`/`module_function`, dart
  `part`/`part of`, f# type providers).

## Confidence re-table recommendations (evidence-based; not yet applied)

Confidence should track measured behavior. **Rev-2 update:** the `erlang` and `csharp`
downgrades proposed in rev 1 are **withdrawn** — erlang's dirty negatives are the
module-oracle question (not sloppiness) and csharp's were a real bug that is now fixed.
No confidence changes are justified by the current corpora; revisit only after a
gold-label held-out pass produces trustworthy per-adapter precision/recall.

| Adapter | Current | Evidence (rev 2) | Disposition |
|---------|---------|------------------|-------------|
| erlang | 0.8 | 3/3 neg flagged = `-module` oracle question, not false-positives | **keep 0.8** (rev-1 downgrade withdrawn) |
| csharp | 0.85 | 2/3 neg were a real visibility bug — **fixed** (now 0/3) | **keep 0.85** (rev-1 downgrade withdrawn) |
| java | 0.85 | 1/4 neg (block comment) — **fixed** (now 0/4) | keep 0.85 |
| php | 0.8 | 1/3 neg = `class` public-by-default oracle question | keep 0.8 |
| ocaml | 0.7 | 1/3 neg (`let () =` binding) — **fixed** (now 0/3) | keep 0.7 |
| htmlcss | 0.4 | breaking=false by design | keep 0.4; **document** no-breaking |

Re-measure after each fix (ROADMAP §7 calibration loop); the report above is rev 2.

## What this closes / what remains

**Closed:** U1/U5 now have a measured, reproducible artifact; a panic-free CI guard
(`adapters_panic_free_and_resolve_on_corpora`); a per-adapter bug catalog with priorities;
**rev-2 targeted fixes** (csharp visibility gate, java block-comment tracking, ocaml
`let () =` binding) that cut false-positives **8/102 → 4/102** with breaking held at 81/84;
and a corrected diagnosis showing the residual 4 are oracle questions, not adapter bugs.

**Remaining backlog (deliberately not swarm-edited), re-prioritized after rev 2:**
1. ~~**Kind-aware `modified` diff (X2)**~~ — **DONE (rev 3, core):** `basic_diff` now sets
   `modified` on same-name/different-kind symbols; behavior-neutral (breaking still keys
   off `removed`); unit-tested; report has a `modified` column (currently 0 — `breaking/`
   corpora are removal-keyed).
2. ~~**`modified/` corpus category (P0)**~~ — **DONE (rev 4):** 3 kind-change pairs + 1
   same-kind control per adapter; harness reports `modified det/pairs`. **24/28 adapters
   fire 3/4 (control holds), 72/108 total, 0 misfires.** javascript/typescript/vue/htmlcss
   are structurally unreachable (name embeds the kind keyword) — see rev-4 map.
3. **Per-adapter kind-change policy (P1):** decide, per language and against gold labels,
   which kind-changes are breaking (e.g. `method`→`property` vs `const`→`static`).
4. **Stable-identifier `name` + `Symbol.signature` (P1; evidence-backed, partially done):**
   - **Step 1a DONE (rev 5, JS):** `javascript.rs::export_name` extracts a stable identifier;
     JS `modified` 0/0 → 3/4. Revealed (and parked in `signature/`) that JS signature/arity
     changes were only "breaking" via name-mashing — now correctly a signature gap.
   - **Step 1b DONE (rev 6, TS):** `export_name` promoted to shared `common.rs::export_name`
     and applied in `ts_parse` (TS-only caller); TS `modified` 0/4 → 3/4. **Vue PARKED:**
     structurally ill-posed at macro granularity (props/emits/expose are distinct
     declarations, no per-declaration identifier to stabilize); needs a per-prop/per-event
     symbol model, not a name fix.
   - **Step 2 PARTIAL (rev 7):** signature detection landed as an additive
     `AstRepresentation.signatures` side-channel (default-empty; no `Symbol`-literal change,
     no artifact-shape change); `basic_diff` flags kind **or** signature change; JS+TS+java+
     csharp+php+scala+dart+ruby+lua+c+cpp+perl+clojure+**ocaml+fsharp** populate signatures;
     the parked JS/TS pairs plus java+csharp+php+scala+dart+ruby+lua+c+cpp+perl+clojure+ocaml+
     fsharp `add_param` pairs now fire as `modified` (**TOTAL signature 15/15**), breaking
     neutral (policy deferred). **Targeting rule:** populate signatures only for *bare-identifier*
     adapters — **side-channel COMPLETE (15 adapters: 12 paren-recipe + clojure/ocaml/fsharp
     custom helpers)**; remaining = robustness (a distinct second pair per adapter) + the deferred
     gold-gated breaking policy (item 11); web adapters N/A at macro granularity. **Skip
     name-mashing
     adapters** (rust/python/erlang/haskell/**go/kotlin/swift/elixir** — go/kotlin/swift
     verified rev 10 (`name` = full `func`/`fun` remainder); elixir verified rev 12 (unit test
     pins `name` = `hello(name)`)) — their `name` already carries the
     signature and detects changes as breaking (the desired behavior); stabilizing their
     `name` would regress breaking precision. **Remaining:** optional structural signature
     type (parsed args/return), and the per-language *breaking* decision for signature changes
     (gold-gated, behind feature flags — item 11).
5. **Gold-label F1 (P0; infra + rust seed DONE rev 5):** `tests/gold_f1.rs` + `labels.json`
   produce true P/R/F1; rust seed = 1.000/1.000/1.000 and is **CI-pinned** (F1 ≥ 0.99 guard).
   **python seed DONE (rev 16c):** 2 seed files → measured **F1 0.800** (P 0.667 / R 1.000); the
   2 FP are `_private_fn`/`_Internal` (no single-leading-underscore filter; PEP 8 treats them as
   non-public) — precision gap, **fix deferred**. **php seed DONE (rev 17):** 2 seed files →
   measured **F1 0.889** (P 1.000 / R 0.800); the 2 FN are `abstract class`/`final class` (the
   adapter only matches the bare `class ` prefix, so modifier-prefixed classes are missed) —
   recall gap, **fix deferred**. **erlang seed DONE (rev 17):** 1 clean file (module+exports /
   record / macro) → **F1 1.000** on unambiguous surface (contested no-export-module stays parked,
   rev-16b). **go/kotlin/swift/elixir seeds DONE (rev 18):** 1 clean file each, labels written to
   exact current emission (trailing-`{` names where the adapter includes them) → all four
   **F1 1.000**, zero drift. Known gap parked (not a seed failure): go receiver methods
   `func (s *T) M()` are invisible to the adapter — receiver-method recall needs an emission-format
   decision before any fix, so it is not measured here. Seed TOTAL (8 langs) = **0.941**
   (TP 32 / FP 2 / FN 2). **c/cpp/java/csharp/ruby/lua/scala/dart/clojure/ocaml/fsharp/perl
   seeds DONE (rev 19):** 1 clean file each, labels written to exact current emission after
   re-reading every parse body → all twelve **F1 1.000** first try, zero drift. Seed TOTAL
   (20 langs) = **0.976** (TP 82 / FP 2 / FN 2); the only sub-1.0 adapters remain python
   (precision) and php (recall), both with measured fixes deferred. **javascript/typescript/
   haskell/vue/svelte/astro/scss/htmlcss seeds DONE (rev 20) — PROJECT-WIDE SEED COMPLETE:**
   all 28 registry adapters now have gold-labeled seeds; the final 8 all measured **F1 1.000**
   first try (labels written to exact emission: TS 4.0 double-emission of `export type` as
   type_export+type, vue/svelte/astro whole-line macro/remainder names, scss/htmlcss whole-line
   names). Seed TOTAL (28 langs) = **0.982** (TP 112 / FP 2 / FN 2). **php modifier fix DONE
   (rev 21):** `php.rs` strips leading `abstract `/`final ` before the class/interface/trait/enum
   prefix match → php gold F1 **0.889 → 1.000** (FN 2 → 0); calibration unchanged (pos 150/150,
   neg 3/101, breaking 79/82, modified 78/112, signature 15/15); `cargo test --lib` 441 passed,
   fmt/clippy clean. Seed TOTAL = **0.991** (TP 114 / FP 2 / FN 0; recall 1.000). **python
   underscore fix DONE (rev 22):** `python.rs` filters single-leading-underscore names
   (PEP 8 internal-by-convention; dunders kept) on def/class/async def → python gold F1
   **0.800 → 1.000** (FP 2 → 0); calibration unchanged (python row pos 5/5, neg 0/4, breaking
   3/3, modified 3/4); 441 lib tests, fmt/clippy clean. **Seed TOTAL = 1.000 (TP 114 / FP 0 /
   FN 0) — all 28 adapters at gold F1 1.000. **Messy real-world corpus DONE (rev 23):**
   `tests/fixtures/gold/messy/` (9 probes: c/cpp/java/kotlin/python/rust/swift/typescript/vue) +
   `labels_messy.json` with HUMAN-ORACLE labels (true public surface, adapter-vocabulary
   emission) + `gold_f1_messy_report` (ignored, NOT CI-pinned). Measured: **TOTAL F1 0.857
   (P 0.750 / R 1.000, TP 21 / FP 7 / FN 0)**. Recall is perfect on messy code; all 7 FPs are
   comment/docstring/substring leakage: c block-comment fn (1), cpp block-comment class+fn (2),
   kotlin block-comment class (1), python docstring def (1), typescript route-marker substring in
   a `//` comment (1), vue `defineProps` substring in a `//` comment (1). Measured CLEAN:
   java 1.000 (its block-comment tracking works), swift 1.000 (comment/string safe — item 7
   suspicion falsified for swift), rust 1.000 (syn control). This is the evidence item 7 was
   waiting for: per-adapter comment/docstring stripping is now justified for c, cpp, kotlin,
   python (docstrings), typescript (route markers), vue (macro substrings) — fixes land one at a
   time, measure → change → re-measure against BOTH labels.json (must stay 1.000) and
   labels_messy.json (must rise), calibration unchanged. **Comment-leak fixes DONE (rev 24):**
   all 7 measured FPs fixed in two waves — wave 1: java-style block-comment tracking added to
   `c.rs`/`cpp.rs`/`kotlin.rs` (c 0.857→1.000, cpp 0.750→1.000, kotlin 0.800→1.000); wave 2:
   `python.rs` triple-quoted docstring tracking (0.889→1.000), `common.rs` route-marker branch
   excludes `//` comment lines (typescript 0.800→1.000), `vue.rs` skips `//` comments before
   macro substring matching (0.667→1.000). **Messy TOTAL 0.857 → 1.000 (TP 21 / FP 0 / FN 0)**;
   clean TOTAL stays 1.000; calibration unchanged (pos 150/150, neg 3/101, breaking 79/82,
   modified 78/112, signature 15/15); 441 lib tests, fmt/clippy clean, guard green. **Messy
   corpus widened to all 28 adapters (rev 25):** +20 probes (29 files total) → **TOTAL F1 0.866
   (P 0.763 / R 1.000, TP 58 / FP 18 / FN 0)**; recall perfect everywhere. 18 FPs across 16
   adapters, all comment/substring leakage: block-comment leaks (no tracking) in go, ruby
   (`=begin`), perl (`=pod`), lua (`--[[`), scala, dart, javascript, typescript, php, scss,
   htmlcss; multi-line-comment-state leaks in fsharp/ocaml (`(* *)` openers skipped but no
   state), haskell (`{- -}`); clojure `(comment ...)` form bodies leak (line-based);
   token/substring leaks in csharp (`has_public_modifier` is token-based — `// public class X`
   leaks), svelte (`$props(` substring in comment), astro (`Astro.props` substring in comment).
   Measured CLEAN (1.000): c/cpp/kotlin/python/typescript-markers/vue (rev-24 fixes hold), java,
   rust, swift, erlang, elixir. **Structural finding (parked):** svelte `rune_state`/
   `rune_derived` are unreachable in the default (Svelte 4) parse — the rune gate is
   `is_svelte5 || line.contains("$props(")`, so `$state(`/`$derived(` lines never enter the
   branch without a `$props(` on the same line or a versioned parse; needs an emission-model
   decision, not a corpus tweak. **Leaker fixes DONE (rev 26):** all 16 fixed in three waves —
   wave A `/* */` tracking for go/scala/dart/javascript/typescript(common.rs)/php/scss/htmlcss;
   wave B language-specific block comments: ruby `=begin/=end`, perl POD (`=<alpha>`…`=cut`),
   lua `--[[ ]]`, haskell `{- -}`, fsharp/ocaml `(* *)` state (replacing opener-only skips);
   wave C: clojure `(comment …)` paren-balance tracking, csharp `/* */` tracking (its `//` skip
   already worked — measured FP was block-only), svelte/astro `//` comment guards before
   substring matches. **Messy TOTAL 0.866 → 1.000 (TP 58 / FP 0 / FN 0); clean TOTAL stays
   1.000; calibration unchanged; 461 lib tests; clippy clean; fmt clean on arc files** (one fmt
   diff in `tests/regressions.rs` belongs to another session — hands-off). Confidence re-table decision (rev 22):** the gold
   data proves emission correctness on clean, unambiguous surface for every adapter, and the
   calibration corpus holds at pos 150/150 — but a ~4-symbol-per-file seed does NOT measure
   precision/recall on messy real-world code (comments, macros, codegen, nested visibility).
   Decision: **keep the current confidence table**; the rev-1 downgrades stay withdrawn (the
   only two known emission defects are now fixed and measured), and any re-table is gated on a
   larger real-world gold corpus (next backlog slice), not on clean seeds alone.
6. ~~**Generated/minified down-weight (X3, P1):** skip codegen/`*.min.*` so they don't
   register as live API (roadmap §8).~~ — **DONE (rev 28):** `is_generated_artifact()`
   (`src/diff/ast.rs:328`) guards the top of the `api_score_inner` per-file loop, covering both
   the adapter and fallback branches. Filename conventions: `.min.`, `.generated.`,
   `_generated.`, `.pb.go`, `.designer.`; header markers (first 4 KiB, 5 lines): Go
   `Code generated … DO NOT EDIT`, .NET `<auto-generated`, GraphQL/Relay `@generated`.
   Measure-first: new test `generated_artifacts_do_not_register_as_api` failed before the fix
   (min.js/.pb.go/Code-generated .ts all registered signatures; measured 4 vs control-only 1),
   passes after. 486 lib tests, fmt/clippy clean, both gold corpora 1.000, calibration
   unchanged.
7. **Per-adapter comment/string stripping (X1)** — **DONE (rev 24)**: reactivated by rev-23
   messy-corpus evidence (7 measured FPs: c/cpp/kotlin block comments, python docstring,
   typescript route-marker comment, vue macro-substring comment); all 7 fixed one adapter at a
   time (block-comment tracking for c/cpp/kotlin, docstring tracking for python, `//` comment
   guards for typescript route markers and vue macros). Messy TOTAL 0.857 → 1.000, clean stays
   1.000, calibration unchanged. Falsified for swift (1.000 on messy probe); scss/svelte/astro
   remain unprobed — probe them if the messy corpus widens.
8. ~~**Re-table confidence** on gold-label data, then re-measure (not before).~~ — **EVALUATED
   (rev 27), table kept unchanged.** `normalize()` (`src/diff/lang/mod.rs:9`) tiers vs evidence:
   every defect motivating the rev-1 downgrade proposals is fixed and verified (php modifiers,
   python underscores/docstrings, 23 comment-leak FPs across 22 adapters); clean gold 1.000 and
   messy gold 1.000 across all 28 adapters; calibration pos 150/150. Tier ordering matches
   structural capability: parser-grade (rust 1.0) > visibility-aware line parsers (go/swift/
   kotlin 1.0, java/csharp 0.85, scala/dart 0.8) > export/underscore-gated (ts 0.9,
   python/php/elixir/erlang 0.8, js/ruby/clojure 0.75-0.7) > no-visibility line parsers
   (c/cpp/haskell/lua/ocaml/perl/fsharp 0.7) > line/macro-granular (scss 0.5, htmlcss 0.4).
   Noted anomaly (no numeric action): vue/svelte/astro 0.85 sit above python 0.8 despite
   macro/whole-line granularity, and svelte has the parked unreachable-rune recall gap (rev 25)
   — but the corpora measure precision/recall on probes, not tier placement at 0.05 resolution,
   so moving numbers now would be unevidenced. **Criteria for any future re-table:** a
   per-language recall corpus of ≥20 real-world files per adapter (mixed visibility, generics,
   decorators, re-exports) measured against human labels; tier moves require measured F1
   separation at that scale.
9. ~~**AST-grade T1** (C/C++/Java/C#/PHP) — sequential, one language at a time.~~ — **DEFERRED
   (rev 29), evidence-backed.** Evaluation: no measured defect requires grammar-grade parsing.
   Both gold corpora are 1.000 across all 28 adapters; calibration pos 150/150; the remaining
   soft metrics are semantics, not parse-grade: the per-adapter modified 3/4 "miss" was verified
   to be the by-design control pair (e.g. `tests/fixtures/adapters/c/modified/control_*.h` adds
   a struct field — symbols unchanged, correctly not a kind-change; a real AST parser would
   score it identically), and neg 3/101 / breaking 79/82 are corpus-labeling questions.
   Cost side: AST-grade means tree-sitter + 5 grammar crates (C builds) — zero such deps exist
   today, and the project's supply-chain policy (deny.toml, DEPENDENCY_AUDIT.md, in-house
   replacements) forbids unevidenced dependency growth. **Trigger criteria to re-open:** (a) a
   gold-corpus failure attributable to line-parsing that bounded line-based logic cannot fix;
   (b) a per-language recall corpus (≥20 real-world files, human-labeled) showing F1 separation
   only grammar-grade parsing can close; (c) a user-reported mis-analysis traced to line-parsing.
10. **New T2/T3 adapters** (~~SQL~~, ~~Terraform/HCL~~, ~~Solidity~~, ~~Groovy~~, ~~Julia~~, ~~R~~, ~~Objective-C~~,
   ~~Zig~~, …) — sequential; each edits the shared registry (not swarm-safe). **SQL DONE (rev 30,
   first new adapter):** `src/diff/lang/adapters/sql.rs` — T2 structured scanner; `CREATE`/`DROP`
   schema objects (10 object kinds) as surface, DML excluded, `drop_<object>` kinds, modifiers
   (`OR REPLACE`/`UNIQUE`/`TEMP`/`MATERIALIZED`), `IF [NOT] EXISTS`, quoted/schema-qualified
   names, case-insensitive keywords, born-correct `--` + `/* */` comment handling. Wired at all
   four contract points (adapter, mod.rs, `normalize()` 0.7 no-visibility band, LANGUAGE_MATRIX
   row) + lint EXT_PROBE + SOURCES backlog update. Evidence: 7 unit tests; clean gold 1.000
   (5 symbols; TOTAL 119 across 29 langs); messy gold 1.000 (born-clean); calibration row
   exactly as designed (pos 4/4, neg 0/2, breaking 2/3, modified 3/4, sig 0/0); TOTALs rose to
   154/154 pos, 81/85 breaking, 81/116 modified with zero regressions in existing rows; 493 lib
   tests, fmt/clippy clean, no-orphan lint green. **HCL DONE (rev 31, second new adapter):**
   `src/diff/lang/adapters/hcl.rs` — T2 structured scanner; Terraform labeled blocks as surface
   (`variable`/`output`/`module`/`provider`; `resource`/`data` as qualified `type.name`
   addresses), unlabeled blocks (`terraform`/`locals`/`moved`/`import`) structural, `.tfvars`
   excluded, born-correct `#`/`//`/`/* */` comment + `<<TAG` heredoc tracking. Wired at all
   four contract points + lint EXT_PROBE (`tf`, `hcl`) + SOURCES backlog update. Evidence: 6
   unit tests; clean gold 1.000 (5 symbols; TOTAL 124 across 30 langs); messy gold 1.000
   (comment/heredoc fakes rejected, born-clean); calibration row exactly as designed (pos 4/4,
   neg 0/2, breaking 2/3, modified 3/4, sig 0/0); TOTALs rose to 158/158 pos, 83/88 breaking,
   84/120 modified with zero regressions; 499 lib tests, fmt/clippy clean (pre-existing
   `tests/soak.rs` fmt diff belongs to another session), no-orphan lint green. **SOLIDITY DONE
   (rev 32, third new adapter):** `src/diff/lang/adapters/solidity.rs` — T2 structured scanner;
   ABI surface: `contract`/`interface`/`library` declarations, `public`/`external` functions,
   `public` state variables, `event`/`error`/`modifier`/`struct`/`enum`, and
   constructor/fallback/receive entry points; `internal`/`private` skipped (explicit visibility
   model honored → 0.8 confidence band); file-level free functions (no visibility keyword)
   treated as surface. Selector-grade signatures: function/event/error/constructor headers
   recorded as canonical parameter-type tuples (`(address,uint256)`, data-location/indexed/
   payable tokens and parameter names stripped) via multi-line header accumulation to the
   `{`/`;` terminator at paren depth 0 — parameter renames are invisible, parameter-type changes
   register as modifications. Born-correct `//`/`///`/`/* */` comment handling. Wired at all
   four contract points + lint EXT_PROBE (`sol`) + SOURCES backlog update. Evidence: 9 unit
   tests; clean gold 1.000 (6 symbols; TOTAL 130 across 31 langs); messy gold 1.000 (comment
   fakes rejected, born-clean); calibration row exactly as designed (pos 4/4, neg 0/2, breaking
   2/3, modified 3/4, sig 1/2 — first rev-30+ adapter with a signature corpus: param-type change
   detected, param-rename control invisible); TOTALs rose to 162/162 pos, 85/91 breaking,
   87/124 modified, 16/17 signature with zero regressions; 508 lib tests, fmt/clippy clean
   (pre-existing `tests/soak.rs` fmt diff belongs to another session), no-orphan lint green.
   **GROOVY DONE (rev 33, fourth new adapter):** `src/diff/lang/adapters/groovy.rs` — T2
   structured scanner; public-by-default semantics: `class`/`interface`/`trait`/`enum`/
   `@interface` declarations, methods (`def`/typed, incl. script-level), PascalCase
   constructors, and depth-1 properties (fields without visibility keywords generate
   getters/setters — brace-depth gating keeps method-locals out); `private`/`protected`
   skipped. Canonical param-type signatures (`(int,String)`, bare params → `def`, defaults
   and annotations dropped) via multi-line header accumulation completing at `{`/`;` OR on
   balanced parens (Groovy interface methods need no terminator). Born-correct `//`/`/* */`
   comments, `#!` shebang, and `'''`/`"""` triple-quoted string tracking. Call-site defense:
   prefix `=`/`)`/`.` rejection, statement-keyword reject sets, annotation-only prefix
   rejection, and a return-type-token requirement (`def`/primitive/PascalCase) that kills
   `println greet("world")`-style FPs. Two bugs found and fixed mid-slice (empty-prefix
   `all()` vacuous-truth constructor kill; terminator-less interface methods). Wired at all
   four contract points + lint EXT_PROBE (`groovy`) + SOURCES backlog update. Evidence: 11
   unit tests; clean gold 1.000 (6 symbols; TOTAL 136 across 32 langs); messy gold 1.000
   (comment/triple-quote fakes rejected, born-clean); calibration row exactly as designed
   (pos 4/4, neg 0/2, breaking 2/3, modified 3/4, sig 1/2); TOTALs rose to 166/166 pos,
   87/94 breaking, 90/128 modified, 17/19 signature with zero regressions; 522 lib tests,
   fmt/clippy clean (pre-existing `tests/soak.rs` fmt diff belongs to another session),
   no-orphan lint green. **JULIA DONE (rev 34, fifth new adapter):**
   `src/diff/lang/adapters/julia.rs` — T2 structured scanner; convention-gated surface:
   `module`/`baremodule`, `struct`/`mutable struct`, `abstract type`, long/short-form
   functions, `macro`, `const`, and struct fields (dot-accessible, define the default
   constructor); `_`-prefixed names internal; qualified definitions (`function Base.show`)
   emit the final dotted component; declarations below keyword-delimited (`end`) block
   depth 1 excluded. Canonical dispatch-type signatures (`(Int,String)`, untyped → `Any`,
   defaults dropped, `{}` parametric commas preserved) via balanced-paren header completion.
   Born-correct `#`/`#= =#` comments and `"""` docstring tracking. Three bugs found and
   fixed mid-slice (struct-field branch unreachable behind `depth <= 1`; parametric struct
   name `Point{T}` over-capture; underscore structs still activating field tracking) plus
   the `unsupported_languages_fall_back` lint updated (Julia graduated; R remains the pinned
   unsupported language, `UNSUPPORTED_PROBE` const). Wired at all four contract points +
   lint EXT_PROBE (`jl`) + SOURCES backlog update. Evidence: 11 unit tests; clean gold 1.000
   (9 symbols; TOTAL 145 across 33 langs); messy gold 1.000 (comment/docstring fakes
   rejected, born-clean); calibration row exactly as designed (pos 4/4, neg 0/2, breaking
   2/3, modified 3/4, sig 1/2); TOTALs rose to 170/170 pos, 89/97 breaking, 93/132 modified,
   18/21 signature with zero regressions; 533 lib tests, fmt/clippy clean (pre-existing
   `tests/soak.rs` fmt diff belongs to another session), no-orphan lint green.
   **R DONE (rev 35, sixth new adapter):** `src/diff/lang/adapters/r.rs` — T2 structured
   scanner; R's assignment-of-`function` idiom: `name <- function()` in all operator forms
   (`<-`, `=`, `<<-`, glued), R6 classes (`Name <- R6Class(...)`), S4 `setClass`/
   `setGeneric`; dot-prefix internal convention (`.helper` skipped — NAMESPACE export
   cross-referencing documented as follow-up); definitions below brace depth 0 excluded.
   Parameter-NAME signatures (`(x,factor,...)` — defaults stripped): R callers bind by
   name, so renames/additions register as modifications while default-value changes do not
   (a genuinely different signature semantics from the type-tuple adapters). Boundary-
   respecting keyword search (`myfunction` ≠ `function`), assignment-prefix validation
   (anonymous `lapply(xs, function(x))` rejected), and quote-containing prefix rejection
   (definition-shaped text inside string literals — one FP found and fixed mid-slice).
   Born-correct `#`/`#'` comment handling (R has no block comments). Wired at all four
   contract points + lint EXT_PROBE (`r`) + `UNSUPPORTED_PROBE` redesigned to never-modeled
   extensions (`txt`, `dat`) so the fallback pin survives the adapter-200 queue + SOURCES
   backlog update. Evidence: 10 unit tests; clean gold 1.000 (4 symbols; TOTAL 149 across
   34 langs); messy gold 1.000 (comment/string fakes rejected, born-clean); calibration row
   exactly as designed (pos 4/4, neg 0/2, breaking 2/3, modified 3/4, sig 2/3 — rename_param
   and add_param fire, change_default_value control invisible); TOTALs rose to 174/174 pos,
   91/100 breaking, 96/136 modified, 20/24 signature with zero regressions; 543 lib tests,
   fmt/clippy clean (pre-existing `tests/soak.rs` fmt diff belongs to another session),
   no-orphan lint green.

   **OBJC DONE (rev 36, seventh new adapter):** `src/diff/lang/adapters/objc.rs` — T2
   structured scanner; Objective-C's API is its *runtime* surface: `@interface`/
   `@implementation`/`@protocol`, `@property`, `NS_ENUM`/`NS_OPTIONS`, and methods
   identified by their **selector** — the full keyword name with colons
   (`setName:age:active:`), the stable identity used by message dispatch and `@selector`.
   The selector is the symbol name directly, so renaming any keyword segment registers
   as breaking, matching ObjC semantics. Parameter types/names are dispatch-invisible
   (no signatures, 0/0 by design). Character-level keyword scan: a keyword is an
   identifier immediately followed by `:(` — the colon is fused to the parameter-type
   paren with no space, so whitespace tokenization missed it (one mid-slice bug:
   `setName:(NSString` is a single token; fixed by scanning chars and requiring the
   `:(` shape, which also rejects `::` in `.mm`). Apple underscore-prefix internal
   convention gates surface (no visibility model — `.h` is owned by the C adapter,
   this adapter claims `.m`/`.mm`; confidence band 0.7). Multi-line headers (one
   keyword segment per line) accumulate to the `;`/`{` terminator at paren depth 0.
   Born-correct `//` + `/* */` comment handling (rev-24/26 discipline). Wired at all
   four contract points + lint EXT_PROBE (`m`, `mm`) + SOURCES backlog update.
   Evidence: 9 unit tests; clean gold 1.000 (4 symbols incl. full-selector method);
   messy gold 1.000 (comment fakes rejected, born-clean); calibration row exactly as
   designed (pos 4/4, neg 0/2, breaking 2/3, modified 3/4, sig 0/0 — underscore control
   invisible, body-change control invisible); TOTALs rose to 178/178 pos, 93/103
   breaking, 99/140 modified, 20/24 signature with zero regressions; 552 lib tests,
   fmt/clippy clean (pre-existing `tests/soak.rs` fmt diff belongs to another session),
   no-orphan lint green. Incident (recovered): a stale `.git/index.lock` from another
   session was removed after process inspection, but the subsequent `git checkout --`
   on the two label files also discarded six slices' uncommitted label entries
   (sql/hcl/solidity/groovy/julia/r); all 12 entries were rebuilt from a probe test
   that printed adapter-extracted symbols plus the fixture files (untracked, so
   untouched), and both gold corpora re-verified at TOTAL 153 clean / 34 messy @
   1.000. Lesson: label-file edits are append-only string surgery — never
   `git checkout --` them on this shared host.

   **ZIG DONE (rev 37, eighth new adapter):** `src/diff/lang/adapters/zig.rs` — T2
   structured scanner; Zig's visibility model is explicit: a declaration is public
   exactly when it carries `pub` (or `export`, implying a public C-ABI entry point),
   so the adapter joins the explicit-visibility 0.8 band. Surface: `pub fn` (incl.
   `export`/`pub extern "..."` forms), `pub const` containers (`struct` incl.
   `packed`/`extern`, `enum`, `union`/`union(enum)`, `opaque`), other `pub const`
   values, `pub var`, and struct-body fields (`name: Type` — Zig has no field-level
   privacy, so every field of an accessible container is reachable; brace-depth
   gating keeps enum/union members and function-body locals out). Methods are
   container-level `pub fn`s emitted flat under their own name (groovy precedent —
   cross-type collisions merge, documented T2 limitation). Canonical parameter-type
   signatures: `name: Type` pairs reduce to their type, so renames are invisible and
   type changes register (`comptime` prefixes dropped, variadic `...` skipped,
   commas inside `fn (i32, u8) void` pointer types kept via paren/bracket-depth
   splitting — Zig has no angle-bracket generics). Multi-line headers (one parameter
   per line is idiomatic) accumulate to the `;`/`{` terminator at paren depth 0.
   Born-correct comment handling: Zig has ONLY `//` line comments (`///` doc, `//!`
   module — no block comments); the stripper is string-aware (a `//` inside a URL
   default survives) and `\\`-prefixed multi-line string literal lines are never
   parsed — both covered by the messy probe. Exclusions documented: non-pub
   declarations, plain `extern fn` imports, `usingnamespace` re-exports,
   `test`/`comptime` blocks, single-line container-body fields. Wired at all four
   contract points + lint EXT_PROBE (`zig`) + LANGUAGE_MATRIX row + SOURCES backlog
   update. Evidence: 11 unit tests (first-run green); clean gold 1.000 (8 symbols:
   VERSION|const, Point|struct, x/y|field, distance|function flat from the struct
   body, Color|enum, greet|function, counter|variable; TOTAL 161); messy gold 1.000
   (comment/doc-comment/module-comment/multiline-string fakes rejected, born-clean;
   TOTAL 36); calibration row exactly as designed (pos 4/4, neg 0/2, breaking 2/3,
   modified 3/4, sig 2/3 — change_param_type and add_param fire, rename_param control
   invisible: Zig callers bind positionally, so types not names are the contract);
   TOTALs rose to 182/182 pos, 95/106 breaking, 102/144 modified, 22/27 signature
   with zero regressions; 563 lib tests, fmt/clippy clean (pre-existing
   `tests/soak.rs` fmt diff belongs to another session), no-orphan lint green.
11. **Per-language feature flags** for behavior-changing rollout (ROADMAP §10).
