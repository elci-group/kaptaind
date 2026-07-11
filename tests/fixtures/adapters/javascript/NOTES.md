# JavaScript adapter — detection rules & fixture expectations

Source of truth: `src/diff/lang/adapters/javascript.rs` (+ `common.rs`).
All expectations below are derived strictly from the current source, even where
the behavior is questionable (see "Known misses / gaps").

## Extensions matched (`detect_files`)

`js`, `jsx`, `cjs`, `mjs` (matched on `Path::extension()`, i.e. the last
extension segment — so `foo.min.js` matches as `js`).

## Public-symbol rules (`parse_ast`)

Line-based scanner over `read_lines_safe` (files > 5 MiB are skipped). For each
`trim()`med line, evaluated in this order:

1. `else if`-free first branch — if the line starts with `export ` (note the
   required trailing space), the remainder `rest` is classified by
   `classify_ts_export(rest)` and emitted as ONE symbol with `name = rest`:
   - starts with `default function ` / `default class ` / `default `, or equals
     `default` -> kind `default_export`
   - starts with `function ` or `async function ` -> `function`
   - starts with `class ` -> `class`
   - starts with `interface ` -> `interface` (TS-ism, still emitted here)
   - starts with `type ` -> `type`
   - starts with `const ` / `let ` / `var ` -> `binding`
   - starts with `enum ` -> `enum`
   - anything else (e.g. `* from ...`, `{ foo } from ...`) -> `export`
2. else-if the line starts with `module.exports` -> kind `cjs_export`,
   `name =` the remainder after `module.exports` (includes leading ` = ...` /
   `.foo`).
3. Independent extra check (runs regardless of 1/2): if the line starts with
   `export function use` OR `export const use` AND does not contain `// ` ->
   kind `hook`, `name =` the FULL trimmed line (including `export `).

Consequence of (1)+(3): an exported hook line (`export function useX` /
`export const useX`) emits **two** symbols — the `function`/`binding` one from
(1) and the `hook` one from (3).

There is NO comment/string stripping. A line is ignored only because its
trimmed start does not match any prefix above. Inline `//` at line start, `*`
inside block comments, and string contents are safe **as long as the line does
not itself begin with `export ` / `module.exports`**.

## Breaking definition (`detect_breaking_changes` + `basic_diff`)

`basic_diff` keys purely on symbol `name` (the string), building added/removed
sets; `modified` is always empty. `detect_breaking_changes` returns
`!diff.removed.is_empty()`. Therefore **any change that removes a `name` is
breaking**: deletions, renames, AND signature/body edits that alter the captured
line (since the `name` is the whole `rest`/`trimmed` string). Pure additions are
not breaking.

## Known misses / gaps (report-only; not fixed)

- G1 Hook double-count: exported hooks yield two symbols (function/binding +
  hook). Inflates counts; diffs report two removed+two added on a rename.
- G2 No generated/minified handling (§8): `.min.js` is not skipped/down-weighted.
  A single-line bundle is parsed as one garbled symbol keyed by its first token.
- G3 Multi-line `export\nfunction foo` is missed: branch (1) needs `export `
  (trailing space) on the SAME line; the `function` line has no `export` prefix.
- G4 Re-exports / barrels (`export * from`, `export {x} from`) are counted as
  new `export` symbols rather than treated as pass-through (matches §8 note).
- G5 Residual string false-positive: a line literally starting with
  `export const s = "export function x()"` IS flagged as `binding` (line-prefix
  match wins; strings are not inspected). Kept out of the negative corpus.
- G6 Asymmetric `name`: export/`cjs_export` use the remainder after the prefix,
  but `hook` uses the full trimmed line.

## Per-file expectations

positive/functions.js -> 2 symbols, both kind `function` (`function greet...`,
`async function loadUser...`).
positive/classes.js -> >=1 symbol kind `class` (name `class UserService {`).
positive/bindings.js -> 3 symbols, all kind `binding` (const/let/var).
positive/default.js -> 1 symbol kind `default_export` (`default function App...`).
positive/hooks.js -> 4 symbols: `function` (useAuth) + `binding` (useTheme) +
`hook` x2 (same two lines) — demonstrates G1 double-count.
positive/cjs.cjs -> 2 symbols kind `cjs_export`; also proves `.cjs` matching.

negative/private.js -> 0 public symbols (no `export`/`module.exports` lines).
negative/comments.js -> 0 public symbols (every export-looking token is behind
`//`, `/*`, or `*`).
negative/strings.js -> 0 public symbols (export text is inside strings; no line
begins with `export `/`module.exports`).

breaking/remove_function -> after removes `function farewell...`; removed set
non-empty -> breaking=true.
breaking/rename_export -> old name `const API_VERSION = 2` removed ->
breaking=true.
breaking/signature_change -> name changes from `function connect(host) {` to
`function connect(host, port) {`, so the old name is removed -> breaking=true
(signature change is breaking because the captured line string changes).

edge/barrel_reexport.js -> 3 symbols, all kind `export` (barrels counted, G4).
edge/minified.min.js -> ext `js` matches; the one line starts with `export `,
yielding 1 garbled `function` symbol = whole remainder (NOT skipped, G2).
edge/multiline_export.js -> 0 symbols: `export` line lacks trailing space and
the `function` line is not prefixed (MISS, G3).
