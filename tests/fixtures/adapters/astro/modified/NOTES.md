# Astro adapter — `modified` (kind-change) fixture corpus

Adapter: `src/diff/lang/adapters/astro.rs`
Detected extension: `.astro` (`detect_files` keeps `extension() == "astro"`).
Parser only emits symbols INSIDE frontmatter (between `---` lines); everything
else is ignored, so every fixture keeps its declarations in frontmatter and the
template body is just realistic filler.

## Kind strings emitted (copied from source)

- `"export"` — `kind: "export".to_string()` — for a frontmatter line that
  `strip_prefix("export ")` succeeds on. `name` = the REST of the line after
  the `export ` prefix (`rest.to_string()`).
- `"props"` — `kind: "props".to_string()` — for a frontmatter line that
  `contains("Astro.props")`. `name` = the FULL trimmed line (`trimmed.to_string()`).

These are the ONLY two kinds the adapter emits.

## Same-name / different-kind mechanism

Because `name` is built differently for the two kinds, a name collision across
kinds only happens when the SAME destructure text is produced once as an
`export` (name = text without `export `) and once as a `props` (name = full
line). Concretely, the constant NAME is always an `Astro.props` destructure
string WITHOUT the `export ` prefix, and the kind flip is driven by adding or
removing the `export ` keyword on that line.

## Pairs

### 1. `props_export_removed`
- NAME held constant: `const { title } = Astro.props;`
- old_kind -> new_kind: `export` -> `props`
- change: `export const { title } = Astro.props;` -> `const { title } = Astro.props;`
- breaking-policy hint: `depends` — dropping `export` hides the binding from the
  template/page surface; breaking only if something consumed the exported binding.
- kinds relied on: `"export"`, `"props"`

### 2. `props_export_added`
- NAME held constant: `const { description } = Astro.props;`
- old_kind -> new_kind: `props` -> `export`
- change: `const { description } = Astro.props;` -> `export const { description } = Astro.props;`
- breaking-policy hint: `no` — exposing an additional binding is additive;
  callers passing props are unaffected.
- kinds relied on: `"export"`, `"props"`

### 3. `props_aliased_export_removed`
- NAME held constant: `const { title: pageTitle } = Astro.props;`
- old_kind -> new_kind: `export` -> `props`
- change: `export const { title: pageTitle } = Astro.props;` -> `const { title: pageTitle } = Astro.props;`
  (aliased destructure variant to vary the realistic shape)
- breaking-policy hint: `depends` — same reasoning as pair 1 (export removal);
  the alias rename itself is unchanged across the pair.
- kinds relied on: `"export"`, `"props"`

### 4. `control` (control)
- NAME held constant: `const prerender = true;`
- old_kind -> new_kind: `same_kind (control)` — `export` -> `export`
- change: leading whitespace only (`export const prerender = true;` ->
  `  export const prerender = true;`); `line.trim()` normalizes the indentation,
  so name and kind are byte-identical.
- expected: NO modified symbol (guards against over-firing).
- kinds relied on: `"export"`

## Uncertainties / caveats (honest)

- Only two kinds exist (`export`, `props`), so three fully DISTINCT
  kind-transitions are impossible; pairs 1 and 3 share the `export -> props`
  transition and pair 2 is its inverse. The pairs vary the realistic code
  shape (plain vs aliased destructure) and direction instead.
- A line matching BOTH rules (e.g. `export const { title } = Astro.props;`)
  emits TWO symbols: the `export` symbol (name without prefix) AND a `props`
  symbol (name = full line, WITH `export `). Consequently each kind-change
  pair also produces one collateral added/removed symbol — the full-line
  `export const ...` `props` symbol present only on the `export` side. The
  NAME we hold constant is the destructure string WITHOUT `export `, which is
  the one that flips kind. A strict harness that expects EXACTLY one modified
  and zero added/removed may need to account for this collateral symbol.
- The parser was NOT run (forbidden by task rules); behavior above is derived
  solely from reading `astro.rs`. If `basic_diff` matches on something other
  than the `name` field, or if trimming/frontmatter toggling behaves
  differently than read, the same-name/different-kind outcome could differ.
- The control relies on `line.trim()` stripping leading whitespace so the
  indented and non-indented export lines yield identical symbols; if the
  harness diffs raw file text instead of parsed symbols, this still holds
  because the modified signal is defined over parsed `(name, kind)`.
