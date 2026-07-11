# Svelte adapter fixture notes

Source of truth: `src/diff/lang/adapters/svelte.rs` (+ `common.rs`). Expectations
below are derived strictly from the code as written, not from Svelte semantics.

## Extensions matched (`detect_files`)

- Only paths whose extension is exactly `"svelte"`. No shebang, no other ext.

## Scan window (`svelte_parse`)

Line-based. A boolean `in_script` flips `true` on a line `starts_with("<script")`
and `false` on a line `starts_with("</script")`. Anything outside a `<script>`
block (template HTML, `<style>`) is skipped. Prefixes are matched against the
**trimmed** line.

## Public-symbol rules (inside `<script>` only)

| Prefix (trimmed)         | kind          | notes |
|--------------------------|---------------|-------|
| `export let `            | `prop`        | name = full remainder of line (incl. `= init` / type) |
| `export const `          | `export`      | name = full remainder |
| `export function `       | `export`      | name = full remainder |
| line contains `$props(`  | `rune_props`  | name = full trimmed line |
| line contains `$state(`  | `rune_state`  | see rune gating below |
| line contains `$derived(`| `rune_derived`| see rune gating below |

The `export let/const/function` matches are `if/else if` (first prefix wins).
Rune detection is a separate `if` and can fire in addition.

### Rune gating (important nuance)

The rune block is entered only when `is_svelte5 || line.contains("$props(")`:
- `parse_ast()` calls `svelte_parse(file, false)` → `is_svelte5 = false`. So in the
  default parse **only `$props(` is detected**; `$state(`/`$derived(` emit nothing
  unless the same line also contains `$props(`.
- `parse_ast_versioned(version)` sets `is_svelte5 = (major >= 5)`. Only with major
  >= 5 are `$state(` and `$derived(` emitted. `$effect(` is never detected
  (mentioned in the source comment but not implemented).

## Known misses / gaps (source-derived, not fixed)

1. **`$state`/`$derived` invisible to default `parse_ast`** (need versioned >=5).
   **`$effect` never detected.**
2. **Single-line / minified `.svelte`:** `<script>` and `export let` on the same
   line are missed — the `<script` line only toggles `in_script` and the rest of
   that line is never scanned (`edge/minified_oneline.svelte`).
3. **No string/template-literal awareness:** a trimmed line starting with
   `export let ` inside a backtick template string is flagged as a `prop`
   (`edge/template_literal_false_positive.svelte`).
4. **No comment-awareness edge:** `//`/`/* */` lines are harmless only because they
   don't start with the export prefixes after trim — there is no real stripping.
5. **Identity = full `name` string** (via `basic_diff`, keyed on `name` only). The
   `name` of a `prop` is the whole remainder (e.g. `title = '';`), so ANY change to
   a prop's type/default/rename alters the name → old removed + new added → treated
   as a removal → **breaking** (over-reports; e.g. changing a default is "breaking").

## Breaking definition (`detect_breaking_changes`)

`diff.removed.any(kind == "prop" || kind == "rune_props")`. Breaking **only** when
a removed symbol is a `prop` or `rune_props`. Removing `export const`/`export
function` (`kind "export"`) or `rune_state`/`rune_derived` is **NOT** breaking.
`modified` is always empty (basic_diff only emits added/removed).

## Per-file expectations

positive/props.svelte -> expect 3 symbols, all kind 'prop'
positive/export_const.svelte -> expect >=1 kind 'export' (API_VERSION) AND >=1 kind 'prop' (label)
positive/export_function.svelte -> expect >=1 kind 'export' (greet) AND >=1 kind 'prop' (who)
positive/rune_props.svelte -> expect 1 symbol kind 'rune_props' (default parse detects $props()
positive/mixed.svelte -> expect 1 kind 'prop' (title) + 1 kind 'export' (KIND); 'let internal', <style>, template ignored

negative/private.svelte -> expect 0 public symbols (no exports; let/const/function are local)
negative/template_only.svelte -> expect 0 public symbols (no <script> block)
negative/comments.svelte -> expect 0 public symbols (commented exports + plain 'let'; no real export prefix)
negative/string_literal.svelte -> expect 0 public symbols ('const snippet' is not 'export const'; string content ignored)

breaking/remove_prop -> after removes 'export let title' -> removed kind 'prop' -> breaking=true
breaking/rename_prop -> 'title' renamed to 'heading' -> old name removed (kind 'prop') -> breaking=true (name-keyed)
breaking/remove_rune_props -> after drops the '$props()' line -> removed kind 'rune_props' -> breaking=true

edge/multilang.svelte -> expect 1 kind 'prop'; <style> and template ignored (only <script> scanned)
edge/minified_oneline.svelte -> expect 0 symbols (KNOWN MISS: <script>+export share one line)
edge/template_literal_false_positive.svelte -> expect 2 kind 'prop': 'real' (true) + 'fake' (FALSE POSITIVE inside backtick string)
