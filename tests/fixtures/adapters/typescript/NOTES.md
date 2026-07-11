# TypeScript adapter — fixture expectations (source-derived)

Source: `src/diff/lang/adapters/typescript.rs` + `src/diff/lang/adapters/common.rs`
(`ts_parse`, `classify_ts_export`, `basic_diff`). Expectations below describe what the
code does TODAY, not what is ideal. The default entry point is `parse_ast()` →
`ts_parse(file, (4, 0))`, so the language version is pinned to **(4, 0)** unless the
eval calls `parse_ast_versioned()`.

## Extensions matched (`detect_files`)
Exact, case-sensitive extension equality: **`ts`** and **`tsx`** only.
Notably `*.d.ts` is matched too (its `Path::extension()` is `ts`) — see `edge/ambient.d.ts`.

## Public-symbol rules by `kind` (line scanner, prefix/contains based)
The scanner walks lines (`trimmed = line.trim()`) and pushes a `Symbol` per matching
rule. Several rules can fire on the SAME line (double emission).

- `export type <tail>` (ver ≥ 3.8 → ACTIVE at 4.0): emits **`type_export`** with
  `name = <tail>` (text after `export type `). AND the generic rule below also fires.
- `export <rest>`: classifies `<rest>` via `classify_ts_export`, `name = <rest>`:
  - `default function ` / `default class ` / `default` / `default ` → **`default_export`**
  - `function ` / `async function ` → **`function`**
  - `class ` → **`class`**
  - `interface ` → **`interface`**
  - `type ` → **`type`**
  - `const ` / `let ` / `var ` → **`binding`**
  - `enum ` → **`enum`**
  - anything else (e.g. `export { … }`, `export * from`, `export =`, `export declare …`) → **`export`**
- Hook: line starts with `export function use` or `export const use` AND does NOT contain
  the substring `// ` → **`hook`**, `name = full trimmed line` (emitted IN ADDITION to the
  function/binding above).
- Route export: line `contains()` one of `generateMetadata`, `generateStaticParams`,
  `getServerSideProps`, `getStaticProps`, `getStaticPaths` AND `contains("export")` →
  **`route_export`**, `name = <marker>` (the literal marker string).
- Middleware: line starts with `export function middleware` or `export const middleware`
  → **`middleware`**, `name = "middleware"` (plus the function/binding above).
- `type <rest> = …` (ver ≥ 5.0 → INACTIVE at default 4.0): would emit **`type_alias`**;
  never produced by `parse_ast()` as wired.

`name` for the common kinds is the FULL export tail (whole signature text), not just the
identifier. This is what drives breaking detection (below).

## Deliberately ignored / known misses
- No class-body parsing: member visibility (`private`/`protected`/`#`/public) and any
  member-level change are invisible. Only top-level `export …` lines are seen.
- Multi-line `interface`/`type`/`class`: only the `export …` header line contributes the
  `name`; member add/remove/retype on following lines does NOT change the symbol.
- Prefix rules naturally ignore `//` and `*`-prefixed comment lines, but the route rule
  is `contains()`-based and can fire inside comments/strings (gap G1).
- CommonJS / dynamic / computed exports (`module.exports`, `exports.x`,
  `Object.assign(exports,…)`) are not detected (§8 dynamic class).
- No generated / minified / bundled detection or down-weighting (§8 generated class).
  Files > 5 MiB are rejected by `read_lines_safe()` (error, not skip).

## Breaking definition
`basic_diff()` compares `symbol.name` sets: `added` = names only in new, `removed` = names
only in old, `modified` = always empty (never populated). `detect_breaking_changes` =
`!diff.removed.is_empty()`. Because `name` is the full export tail, ANY textual edit to an
existing export line changes its `name` → old name counts as removed → **breaking = true**,
even for semantically additive changes (new optional param) or same-line reformatting.
Conversely, member-only changes inside multi-line bodies leave the header `name` unchanged
→ `removed` empty → **breaking = false** (false-negative for real breaking member edits).
Pure additions (a brand-new export, nothing else changed) → `removed` empty → non-breaking.
Re-exports/barrels ARE counted as symbols (per roadmap §8), so editing a barrel line is breaking.

## Per-file expectations

positive/
- `functions.ts` → expect ≥1 symbol kind `function`; `add`/`helper` and async `load` all
  classify as `function`. No `hook`/`route_export`/`middleware`.
- `classes.ts` → expect ≥1 kind `class` (two: `UserService`, `Repository<T>`). Member
  lines (`constructor`/`find`/`save`) are NOT emitted.
- `types.ts` → expect ≥1 kind `interface`, ≥1 kind `enum`, and for the two
  `export type …` lines BOTH `type_export` AND `type` (double emission). ≥6 symbols total.
- `bindings.ts` → expect 3 symbols kind `binding` (`const`/`let`/`var`).
- `default_export.ts` → expect ≥1 kind `default_export` (from `export default function`);
  also 1 `binding` from `export const helper`.
- `framework.tsx` (.tsx, Next.js-style) → expect ≥1 kind `hook` (`useCart`), ≥1 kind
  `middleware` (`middleware`), ≥1 kind `route_export` (`getStaticProps`,
  `generateMetadata`); plus the underlying `function` symbols.

negative/  (each must yield **0 public symbols**)
- `unexported.ts` → non-exported top-level items + `private`/`protected`/`#`/`public`
  members → 0 (nothing starts with `export `).
- `comments.ts` → `//` lines, `*`-prefixed JSDoc lines, and an inline `/* … */` line → 0
  (no trimmed line starts with `export `; no route markers).
- `strings_and_dynamic.ts` → export-like text inside strings + `module.exports`/
  `exports.x`/`Object.assign(exports,…)` → 0 (not detected; no route marker co-occurs).
- `generated_min.ts` → banner comment + non-exported `const`/`function`/`class` → 0.
  (Gap: a generated file that DOES export would still be flagged — there is no generated-skip.)

breaking/  (all TRUE-breaking per the adapter: `after` makes an old `name` disappear)
- `remove_export` → after drops `export function drop()` → `removed` non-empty → breaking=true.
- `change_signature` → after edits the `greet` export line (adds a param) → its `name`
  changes → old name removed → breaking=true.
- `rename_export` → after renames `oldName`→`newName` → `oldName` removed → breaking=true.
  (In all three, the `keep` export is unchanged and is NOT in `removed`.)

edge/
- `reexports.ts` (§8 re-exports/barrels) → `export {…}`, `export * from`,
  `export { default as … }`, `export =` all fall through to kind **`export`** → expect
  5 symbols kind `export` (re-exports counted as new).
- `hook_comment_quirk.tsx` (§8 comments) → `useTheme` and `useFlag` emit `hook`; the
  `useAuth` line has a trailing `// comment`, so the `// ` substring SUPPRESSES its `hook`
  kind (it still emits `function`). Expect ≥1 `hook`, and `useAuth` NOT a hook.
- `ambient.d.ts` → matched by `detect_files` (ext `ts`); `export declare function` and
  `export declare const` fall through to kind **`export`**, `export interface` → `interface`.
  Expect ≥1 `interface` and ≥1 `export`.

## Suspected bugs / gaps (reported, not fixed)
- G1 route_export over-match: `contains(marker) && contains("export")` fires on comments
  or strings (e.g. a line `// see export getStaticProps docs` yields a spurious
  `route_export`). No fixture added (would break the 0-symbol negatives).
- G2 hook under-match: any `// ` substring anywhere on the line disables `hook`
  (`edge/hook_comment_quirk.tsx`).
- G3 `*.d.ts` treated as source and `export declare …` mis-classified as generic `export`
  (`edge/ambient.d.ts`).
- G4 No member/body parsing → member visibility and member-level breaking changes are
  invisible (false-negatives for interface/type/class member edits).
- G5 No generated/minified/declaration down-weight or skip; >5 MiB errors instead of skip.
- G6 CommonJS/dynamic/computed exports invisible (§8 dynamic class).
- G7 `name`=full-tail makes trivial same-line edits (whitespace/comment) look breaking and
  makes additive signature changes look breaking → noisy `breaking=true`.
- G8 Double emission (`type_export`+`type`, `hook`+`function`/`binding`,
  `middleware`+`function`/`binding`, `route_export`+`function`) inflates counts/hash;
  harmless for `.any()` checks but skews count-based metrics.
