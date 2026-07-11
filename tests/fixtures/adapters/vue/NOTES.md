# Vue adapter fixture notes

Source of truth: `src/diff/lang/adapters/vue.rs` (+ `src/diff/lang/adapters/common.rs`).
All expectations below are derived strictly from that source, not from ideal Vue semantics.

## Extensions matched (`detect_files`)

- Only extension `vue` (exact, case-sensitive via `Path::extension() == "vue"`). One extension only.

## Public-symbol rules (`parse_ast`) by `kind`

`parse_ast` reads lines (`read_lines_safe`, errors if file > 5 MiB) and gates everything
behind a `<script>` block:
- a trimmed line starting with `<script` sets `in_script = true` (covers `<script>`,
  `<script setup>`, `<script lang="ts" setup>` — any attribute order); the tag line itself
  is skipped.
- a trimmed line starting with `</script` sets `in_script = false`.
- all `<template>`/`<style>`/other lines are ignored (`!in_script`).

Inside a script block, each trimmed line is matched by an `else if` chain — at most ONE
symbol per line, first match wins, `name` is the full trimmed line (no comment/string
stripping, no dedup):

| `kind`    | rule (on trimmed line)                  | `name` value                |
|-----------|-----------------------------------------|-----------------------------|
| `props`   | `.contains("defineProps")`              | the whole line              |
| `emits`   | `.contains("defineEmits")`              | the whole line              |
| `expose`  | `.contains("defineExpose")`             | the whole line              |
| `export`  | `.strip_prefix("export ")` succeeds     | the text after `export `    |

`extract_api` returns every parsed symbol as `public_symbols` (no real public/private split).

## Breaking definition (`detect_breaking_changes`)

`diff_ast` = `basic_diff`: compares symbols by `name` ONLY (HashSet of full-line strings);
`modified` is always empty; a line whose text changes becomes one `removed` + one `added`.
`detect_breaking_changes` is `true` iff any **removed** symbol has `kind == "props"` or
`kind == "emits"`. Removals of `expose`/`export` are NOT breaking. Consequence: ANY textual
change to a `defineProps`/`defineEmits` line (even adding a prop) changes the whole-line
`name` → the old name is "removed" → flagged breaking (over-sensitive).

## Known misses / gaps (reported, not fixed)

- No comment/string filtering: `// defineProps(...)` or a string containing `defineEmits`
  inside `<script>` is flagged as a real symbol (see `edge/`).
- Substring matching: `defineProps` also matches `withDefaults(defineProps<...>())`
  (intended-ish) and the legacy `definePropsWithDefaults` name (counted as `props`).
- Breaking over-sensitivity: additive/reformat edits to a props/emits line read as breaking.
- Breaking under-detection: removing `defineExpose` or an `export` is not treated as breaking.
- Naive block toggle: a second `<script>` re-opens parsing; no special handling of
  `<script src>` external blocks.

## Per-file expectations

positive/
- `composition_setup.vue` -> expect >=1 `props`, >=1 `emits`, >=1 `expose` (3 symbols).
- `props_only.vue`        -> expect >=1 `props`; 0 `emits`/`expose`/`export`.
- `emits_array.vue`       -> expect >=1 `emits`; 0 others.
- `export_default.vue`    -> expect >=1 `export` (name `default {`); 0 `props`/`emits`/`expose`
  (`props: ['title']` does NOT contain `defineProps`).
- `with_ts_lang.vue`      -> expect >=1 `props` (lang attr still toggles script on).
- `named_export.vue`      -> expect 2 `export` symbols.

negative/ (all -> expect 0 public symbols)
- `internal_composables.vue` -> imports/refs/computed/internal fn; no `define*`/`export`.
- `private_members.vue`      -> `_`-prefixed internals; no `define*`/`export`.
- `template_only.vue`        -> no `<script>` block at all.
- `style_and_template.vue`   -> `<template>` + `<style>` only; no `<script>`.

breaking/ (before/after pairs; all TRUE breaking per the adapter -> breaking=true)
- `remove_prop`   -> after removes the `defineProps` line (removed `props`).
- `remove_emit`   -> after removes the `defineEmits` line (removed `emits`).
- `rename_prop`   -> prop signature text changes (`label`->`title`); old full-line name is
  removed (`props`) => breaking=true (illustrates over-sensitivity).

edge/
- `commented_macro.vue` -> `// const props = defineProps<...>()` is INSIDE `<script>` and
  contains `defineProps`, so source flags >=1 `props` (FALSE POSITIVE: no comment stripping).
- `string_macro.vue`    -> string literal contains `defineEmits`; source flags >=1 `emits`
  (FALSE POSITIVE: no string filtering).
- `template_gating.vue` -> the `defineEmits` mention is in `<template>` (ignored); only the
  real `defineProps` in `<script setup>` counts -> expect exactly 1 symbol, kind `props`,
  0 `emits` (proves script-block gating).
