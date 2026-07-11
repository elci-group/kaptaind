# Lua `modified` (kind-change) fixture notes

Source of truth: `src/diff/lang/adapters/lua.rs`. Extension: `.lua` (only ext the adapter
detects). All pairs hold the parser-emitted `name` byte-identical and flip only the `kind`.

The adapter emits exactly two `kind` strings:
- `"module_export"` — line with an assignment `=`, trimmed LHS literally starts with `M.`;
  `name` = the FULL LHS (e.g. `M.add`, `M.VERSION`).
- `"function"` — line starts with `function ` (and not `local function `); `name` = first
  token after `function ` up to `(` (dotted path allowed, e.g. `M.add`, `M.VERSION`).

The only name form both rules can emit identically is `M.<dotname>`, so every cross-kind pair
below is an `M.x` toggle between `M.x = …` and `function M.x(…)`.

## Pairs

### 1. `export_to_func` — name held constant: `M.add`
- `old_kind -> new_kind`: `module_export` -> `function`
- Relied-on kind strings: `"module_export"`, `"function"`.
- Breaking-policy hint: **no** — `M.add(a, b)` is called identically either way; only the
  definition syntax changes (anon-fn assignment vs named function), same signature/behavior.

### 2. `func_to_export` — name held constant: `M.sub`
- `old_kind -> new_kind`: `function` -> `module_export`
- Relied-on kind strings: `"function"`, `"module_export"`.
- Breaking-policy hint: **no** — `M.sub(a, b)` call site is unchanged; the value stored in
  `M.sub` is still a function with the same body.

### 3. `const_export_to_func` — name held constant: `M.VERSION`
- `old_kind -> new_kind`: `module_export` -> `function`
- Relied-on kind strings: `"module_export"`, `"function"`.
- Breaking-policy hint: **yes** — `M.VERSION` changes from a string value (read directly) to
  a function (must be called `M.VERSION()`); type and access pattern both change for callers.

### 4. `control` — name held constant: `M.greet`
- `same_kind (control)`: `function` -> `function`
- Relied-on kind string: `"function"`.
- Only the body string literal changed (`"hi, "` -> `"hello, "`); the `function M.greet(name)`
  line is byte-identical. Must yield NO `modified` symbol (over-firing guard).
- Breaking-policy hint: **no** — same name, same kind, same signature; trivial body tweak.

## Uncertainties / honesty notes

- **Conflict with existing corpus docs (cannot resolve here).** The pre-existing
  `tests/fixtures/adapters/lua/NOTES.md` states that `basic_diff` "compares symbol names only"
  and that `modified` is "always empty". The task premise, however, defines the shared
  `modified` signal as same-name/different-kind. I am NOT permitted to read `common.rs`/
  `basic_diff` or run the parser, so I can only guarantee the *parser-level* fact: each pair
  emits an identical `name` with a different `kind`. Whether the engine actually surfaces
  these as `modified` depends on `basic_diff` comparing `kind` — which the older NOTES.md
  denies. If `basic_diff` truly ignores `kind`, pairs 1–3 will (incorrectly, per the task
  premise) produce no `modified` and the corpus will not exercise the signal until that is
  fixed. Flagging for whoever reconciles the engine vs. the lua NOTES.md.
- **Only two kinds exist.** With just `module_export` and `function`, only two directed
  kind-transitions are possible, so pair 3 necessarily repeats pair 1's direction
  (`module_export` -> `function`). It is kept as a distinct *scenario* (data/constant export
  turning into a callable, name `M.VERSION`) rather than anon-fn→named-fn.
- **Pair-3 validity.** `M.VERSION = "1.0.0"` is a valid assignment (LHS `M.VERSION`,
  `module_export`); `function M.VERSION()` is valid Lua funcname syntax (dotted `Name`) and
  parses to name `M.VERSION`. Both are realistic and syntactically valid.
- **Control safety.** `control_*` adds no `=` assignment lines and does not touch the
  `function M.greet` line, so it cannot accidentally introduce a `module_export` or rename.
