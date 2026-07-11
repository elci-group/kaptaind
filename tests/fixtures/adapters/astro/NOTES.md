# astro adapter — fixture corpus + detection rules

Source of truth: `src/diff/lang/adapters/astro.rs` (+ `src/diff/lang/adapters/common.rs`).
All expectations below are derived strictly from the current source, not from what the
adapter *ought* to do.

## Extensions matched (`detect_files`)

- Only paths whose `extension()` is exactly `astro` (case-sensitive). Anything else
  (`.astrox`, `.astro.bak`, no extension, uppercase `.ASTRO`) is ignored.

## Public-symbol rules (`parse_ast`), by `kind`

The parser only ever looks at the **frontmatter**: lines between a pair of fences whose
trimmed form is exactly `---`. A lone `---` toggles `in_frontmatter`; the `---` line
itself is skipped. Lines outside the fences (HTML template, `<style>`, `<script>`) are
never scanned. Inside frontmatter it emits two kinds:

- `kind = "export"` — for any frontmatter line whose trimmed form starts with `export `.
  `name` is everything after `export ` (e.g. `const prerender = true;`). No further
  classification (functions/const/interface all collapse to kind `export`).
- `kind = "props"` — for any frontmatter line whose trimmed form `contains("Astro.props")`.
  `name` is the full trimmed line.

Everything else (non-exported bindings, template/style/script, comments) yields no symbol
— **but comments and strings are NOT filtered**, so a `//` comment or a string/template
literal that mentions `Astro.props` still emits a `props` symbol (see known gaps).

## Diff / breaking definition (`detect_breaking_changes`)

`diff_ast` uses `basic_diff`: it compares symbols by **`name` only** (HashSet of names),
producing `added`/`removed`; `modified` is always empty. `kind` plays no role in the set
math. `detect_breaking_changes` returns `true` iff `diff.removed` contains **any** symbol
with `kind == "props"`. Consequences:

- Removing or editing a `props` line (its full-line name changes) => `removed` has a
  `props` => **breaking = true**.
- Removing/renaming/retyping an `export` symbol is **never** breaking (only props count).
- Signature changes are not modeled as `modified`; they show as remove + add.

## Per-file expectations

### positive/
- `positive/export_const.astro` -> 2 symbols: >=1 kind `export`, >=1 kind `props`.
- `positive/export_functions.astro` -> 2 symbols, both kind `export` (`getStaticPaths`,
  `GET`); 0 `props`.
- `positive/export_interface.astro` -> 2 symbols: >=1 `export` (`interface Props...`),
  >=1 `props`.
- `positive/props_only.astro` -> 1 symbol kind `props`; 0 `export`.
- `positive/endpoint_post.astro` -> 2 symbols, both kind `export` (`prerender`, `POST`);
  0 `props`.

### negative/ (each: expect exactly 0 public symbols)
- `negative/template_only.astro` -> 0 (non-exported bindings; no `Astro.props`).
- `negative/no_frontmatter.astro` -> 0 (no `---` fence; `<script>` body never scanned,
  even though it contains `export`/`Astro.props` text).
- `negative/commented_export.astro` -> 0 (commented/string `export` lines do not start
  with `export `; no `Astro.props` substring present).
- `negative/generated_banner.astro` -> 0 (generated banner + non-exported consts only).

### breaking/ (before/after pairs; all TRUE breaking per the adapter)
- `breaking/remove_props` -> after drops `const { title } = Astro.props;`; a `props`
  symbol is removed -> breaking = true.
- `breaking/rename_props_destructure` -> `{ title }` -> `{ heading }` changes the
  full-line name, so the old `props` symbol is removed (new one added) -> breaking = true.
- `breaking/props_to_static` -> `const title = Astro.props.title;` removed -> a `props`
  symbol is removed -> breaking = true.

### edge/
- `edge/multilang_template.astro` -> exactly 1 `export` + 1 `props` from frontmatter;
  the `export`/`Astro.props` text inside `<style>`/`<script>` is NOT counted (multi-
  language single file: only frontmatter is routed to the parser).
- `edge/string_comment_false_positive.astro` -> 1 `export` + **3 `props`** (a `//`
  comment, a string, and a template literal each mention `Astro.props` and all match);
  documents the comment/string false-positive gap.
- `edge/unclosed_frontmatter.astro` -> 1 `export` + 1 `props`; missing closing fence keeps
  `in_frontmatter` true, so the trailing template is scanned as frontmatter (no panic).

## Known misses / suspected gaps (reported, not fixed)

1. **No comment/string filtering in frontmatter.** `//` lines and string/template
   literals containing `Astro.props` emit false `props` symbols
   (`edge/string_comment_false_positive.astro`).
2. **Naive `---` toggle.** An unclosed frontmatter scans the whole remaining file as
   frontmatter (`edge/unclosed_frontmatter.astro`); a leading BOM (U+FEFF, not stripped
   by `trim()`) on the opening fence would prevent it from ever opening (0 symbols).
3. **Breaking ignores exports.** Only removed `props` count as breaking; deleting or
   changing an `export` (the real public surface, e.g. an endpoint `GET`) yields
   breaking = false.
4. **`basic_diff` is name-only, `modified` always empty; `kind` ignored by set math.**
   Any props-line edit becomes remove+add (=> breaking); any export-line edit is also
   remove+add but never breaking.
5. **Silent empty results.** `read_lines_safe` bails on files >5 MB and the `if let Ok`
   swallows the error => 0 symbols (not an error); invalid-UTF8 lines truncate iteration
   via `map_while(Result::ok)`.
6. **No export sub-classification / no `Props`↔`Astro.props` linkage.** All exports are
   kind `export`; the `interface Props` symbol and the props usage are independent.
