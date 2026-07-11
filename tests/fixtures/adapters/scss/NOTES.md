# SCSS/Sass/Less adapter — fixture expectations

Source: `src/diff/lang/adapters/scss.rs` (`ScssAdapter`) + helpers in
`src/diff/lang/adapters/common.rs`. All expectations below are derived strictly
from what the source does today, not from what ideal SCSS semantics would be.

## `detect_files` — extensions matched
Exactly these lowercase extensions (case-sensitive `ext ==`):
- `scss`
- `sass`
- `less`

Anything else (e.g. `css`, `SCSS`, `sassc`) is NOT matched by this adapter.

## `parse_ast` — how it works
Line-based scanner: `read_lines_safe`, `trim()` each line, then a single
`if / else if` chain. **At most ONE `Symbol` is emitted per input line.** The
`name` stored for `variable` / `css_var` / `forward` is the **entire trimmed
line** (value included); for `mixin` it is the text after `@mixin `.
`extract_api` returns all symbols as `public_symbols`. `diff_ast` = `basic_diff`,
which compares by `name` only and always returns an empty `modified` list (every
change is reported as remove + add).

## Public-symbol rules by `kind`
- `variable` — `trimmed.starts_with('$') && contains(':')` (SCSS/Sass var),
  OR `starts_with('@') && contains(':')` AND the line does NOT start with any of
  `@media`, `@import`, `@include`, `@mixin`, `@use`, `@forward` (Less var).
- `mixin` — `trimmed` starts with `@mixin ` (the space is required).
- `css_var` — `trimmed.starts_with('--') && contains(':')`.
- `forward` — `trimmed` starts with `@forward ` (the space is required).

## Known misses / gaps (report, do not fix)
- `@use` is NOT detected (the source comment groups it with `@forward` as module
  API, but only `@forward` has a branch). `@use 'x';` -> 0 symbols.
- `@function` is NOT detected -> 0 symbols (it is real Sass public API).
- Plain selectors/properties (`.btn`, `#id`, `color: red`) are never symbols.
- No privacy filtering: Sass "private" members (`$_x`, `$-x`) are emitted as
  public `variable`.
- No comment/string state: a `$x: 1;` line inside a multi-line `/* ... */` block
  IS flagged. (Single-line `//` / `/* ... */` lines are safe only because their
  trimmed start is not `$`/`@`/`--`.)
- One-symbol-per-line: a minified single line with several constructs emits only
  the first `$`-prefixed match; later `$vars`/`@mixin`s on the same line are lost.
- `css_var` removal is NOT treated as breaking (kind not in the breaking set).
- Because `name` includes the value, changing a variable's VALUE changes its
  `name` -> reported as removed+added -> flagged breaking (over-sensitive).

## Breaking definition (`detect_breaking_changes`)
Removals only. Returns `true` iff any `diff.removed` symbol has `kind` in
{`variable`, `mixin`, `forward`}. Additions are never breaking. `css_var`
removals and signature/value-only changes of mixins/forward are only breaking
insofar as they change the stored `name` (see gap above).

## Per-file expectations

positive/
- `variables.scss` -> 2 symbols, both kind `variable`.
- `mixins.scss` -> 2 symbols, both kind `mixin` (body lines produce none).
- `css_vars.scss` -> 2 symbols, both kind `css_var`.
- `forward.scss` -> 2 symbols, both kind `forward`.
- `less_vars.less` -> 2 symbols, both kind `variable` (`@primary`, `@font-size`);
  the `@media` line is excluded and emits nothing.
- `sass_syntax.sass` -> 2 symbols, both kind `variable` (`.sass` ext matches).

negative/
- `selectors.scss` -> 0 public symbols.
- `at_rules.scss` -> 0 public symbols (`@import`/`@media`/`@include`/`@use`).
- `functions.scss` -> 0 public symbols (`@function` is a known miss).
- `commented.scss` -> 0 public symbols (single-line comments only).

breaking/ (before -> after)
- `remove_variable`: after drops `$primary` -> 1 removed `variable` -> breaking=true.
- `remove_mixin`: after drops `@mixin flex-center` -> 1 removed `mixin` -> breaking=true.
- `change_value`: `$primary: #007bff;` -> `$primary: #ff0000;` changes the stored
  `name`, so it is seen as 1 removed `variable` (+1 added) -> breaking=true
  (documents value-as-name over-sensitivity).

edge/
- `minified.scss` (`$a:1;$b:2;@mixin x{}`) -> exactly 1 symbol, kind `variable`,
  name = the whole line; `$b` and `@mixin` are lost (one-symbol-per-line).
- `block_comment.scss` -> 1 symbol, kind `variable` (`$inside: 1;`); the adapter
  does not track multi-line `/* */`, so it over-detects inside the comment.
- `private_members.scss` -> 3 symbols, all kind `variable`; `$_internal` /
  `$-legacy` are NOT treated as private (no privacy filtering).
