# htmlcss adapter fixture notes

Source of truth: `src/diff/lang/adapters/htmlcss.rs` (+ `src/diff/lang/adapters/common.rs`,
`src/diff/lang/adapter.rs`). All expectations below are derived strictly from what the
source does today, not from what correct HTML/CSS semantics would imply.

## Extensions matched (`detect_files`)

Only files whose extension is **exactly** `html` or `css` (case-sensitive). NOT matched by
this adapter: `.htm`, `.HTML`, `.CSS`, `.scss`/`.sass`/`.less` (SCSS has its own adapter),
or any other extension.

## Public-symbol rules (`parse_ast`), by `kind`

The file is read line-by-line (`read_lines_safe`, bail if >5MB, non-UTF8 lines silently
dropped). Each line is `trim()`ed, then matched with two `if / else if` checks. The emitted
`Symbol.name` is the **entire trimmed line** (not a normalized identifier).

- `kind = "css_var"`: trimmed line `starts_with("--")` AND `contains(':')`. This is a CSS
  custom-property *definition at line start* (e.g. `--primary: #fff;`). Usages like
  `color: var(--primary);` do NOT qualify (line starts with the property name, not `--`).
- `kind = "css_class"`: (else) trimmed line `starts_with('.')` AND `contains('{')`. The
  selector must begin at the start of the line AND the `{` must be on the SAME line
  (e.g. `.btn {`, `.card:hover {`, `.btn, .link {`).

The two branches are mutually exclusive (`else if`); a line can emit at most one symbol.
No HTML is parsed at all — `.html` files are scanned with the exact same two raw-line rules.

## Deliberately ignored (never emitted)

- ID selectors `#id {`, element/type selectors `div {`/`body {`, universal `* {`,
  attribute selectors `[type="text"] {`, pseudo-only `:hover {`/`::before {`.
- At-rules `@media`/`@keyframes`/`@import`/`@charset`/`@supports` (the at-rule line itself;
  see known miss about scope-blindness).
- Comments (`/* ... */`, `// ...`) and the contents of strings (line start governs).
- HTML tags, `id=`/`class=` attributes, inline `style=""`, `<script>` contents, HTML
  comments — there is no HTML extractor.

## Breaking definition (`detect_breaking_changes`)

Hardcoded to return **`false` for every diff**. This adapter never reports a breaking
change, regardless of added/removed/modified symbols. `diff_ast` uses `basic_diff`, which
computes `added`/`removed` by symbol name and always leaves `modified` empty — but
`detect_breaking_changes` ignores all of it. Therefore every `breaking/` pair below is
semantically a breaking API change yet the adapter reports `breaking=false`.

## Known misses / gaps (report, do not fix)

1. **No breaking detection** — `detect_breaking_changes` is `false` unconditionally; removed
   or renamed public classes/vars have zero semver effect via this adapter.
2. **No HTML parsing** — the "HTML" half of "HTML/CSS" yields nothing; tags, `id`/`class`
   attributes, inline styles, and `<script>` are invisible. Only raw CSS-like lines count.
3. **Line-start + same-line-`{` requirement** — selectors not starting at line start
   (`div.container {`, `ul > .item {`) are missed; so are Allman-style selectors where `{`
   is on the next line (false negatives).
4. **Scope-blind** — a `.x {` line nested inside `@media`/`@supports` IS detected (the
   adapter does not track at-rule scope), so at-rule membership is neither recorded nor used
   to exclude.
5. **`name` = whole trimmed line** — two rules sharing a selector but differing in trailing
   text get different names; property/signature changes within one selector are never
   `modified` (`basic_diff.modified` is always empty).
6. **No minified/generated handling (§8)** — a single minified line collapses to one symbol
   (the whole line), heavily undercounting; no down-weight/skip for `*.min.css`/`dist/`.
7. **Custom-property usages** (`var(--x)`) are not symbols; only line-start definitions are.
8. Documented detection confidence for HTML/CSS is the lowest tier (per `AGENTS.md`); the
   adapter itself does not set confidence.

## Per-file expectations

### positive/
- `positive/classes.css` -> 4 symbols, all kind `css_class` (`.btn {`, `.card:hover {`,
  `.nav .item {`, `.btn, .link {`); 0 `css_var`. Expect `>=3 css_class`, `0 css_var`.
- `positive/vars.css` -> 3 symbols, all kind `css_var` (`--primary-color: ...`,
  `--secondary-color: ...`, `--font-size-base: ...`); `:root {` is NOT counted. Expect
  `>=3 css_var`, `0 css_class`.
- `positive/mixed.css` -> 2 symbols: 1 `css_var` (`--brand: #3366ff;`) + 1 `css_class`
  (`.button {`); the `background: var(--brand);` usage is ignored. Expect `>=1 css_class`
  AND `>=1 css_var`.
- `positive/pseudo_class.css` -> 3 symbols, all `css_class` (`.btn:hover {`,
  `.item::before {`, `.link:visited {`). Expect `>=3 css_class`, `0 css_var`.
- `positive/embedded_style.html` -> 2 symbols: 1 `css_class` (`.btn {`) + 1 `css_var`
  (`--brand: #3366ff;`), both from inside the `<style>` block; the `<div class="btn">`
  attribute is ignored. Expect `>=1 css_class` AND `>=1 css_var`.

### negative/
- `negative/element_and_id.css` -> 0 public symbols (`body`, `#app`, `div.container`, `*`,
  `[type="text"]` all ignored; note `div.container {` is a false-negative line-start miss).
- `negative/at_rules.css` -> 0 public symbols (`@import`/`@charset`/`@media`/`@keyframes`
  ignored; inner lines are element/property only — deliberately no class line so the count
  stays 0).
- `negative/comments_and_strings.css` -> 0 public symbols (`@charset` ignored; `/* */`
  comment lines ignored; the `body { content: ".literal-class {"; }` line starts with
  `body`, so the string containing `.literal-class {` is not flagged).
- `negative/html_tags.html` -> 0 public symbols (tags, `class=`/`id=` attributes, and HTML
  comments are not parsed).

### breaking/  (all semantically breaking; adapter reports `breaking=false` for every pair)
- `breaking/remove_class_before.css` -> 2 `css_class` (`.btn {`, `.card {`).
- `breaking/remove_class_after.css` -> 1 `css_class` (`.btn {`); removes `.card {` ->
  `basic_diff.removed` non-empty, but `breaking=false`.
- `breaking/remove_var_before.css` -> 2 `css_var` (`--primary: ...`, `--secondary: ...`).
- `breaking/remove_var_after.css` -> 1 `css_var` (`--secondary: ...`); removes
  `--primary: ...` -> `breaking=false`.
- `breaking/rename_class_before.css` -> 1 `css_class` (`.btn {`).
- `breaking/rename_class_after.css` -> 1 `css_class` (`.button {`); old selector gone,
  new one added (`removed`+`added` both non-empty) -> `breaking=false`.

### edge/
- `edge/minified.css` -> exactly 1 symbol, kind `css_class`, whose `name` is the ENTIRE
  single line (`.a{color:red}.b{margin:0}.c{padding:1px}`); 3 selectors collapse to 1
  symbol, 0 `css_var`. Documents §8 generated/minified undercount + no special handling.
- `edge/multiline_selector.css` -> 0 public symbols (`.btn` line has no `{`; the `{` is on
  the next line). Documents the same-line-`{` false-negative.
- `edge/inline_style_attrs.html` -> 0 public symbols (inline `style=""`, `class=` values,
  and `<script>` strings `".btn {"` / `"--brand: #fff"` are not parsed). Complements
  `positive/embedded_style.html`: only `<style>`-block raw CSS lines are scanned.
