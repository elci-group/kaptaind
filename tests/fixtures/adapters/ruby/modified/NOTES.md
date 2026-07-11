# Ruby adapter — `modified` kind-change fixtures

Diff signal exercised: a symbol is `modified` when its NAME is unchanged but its
KIND changes (same `name`, different `kind`). Each pair below keeps the extracted
name byte-identical and swaps only the kind-bearing keyword/declaration.

Adapter facts relied on (`src/diff/lang/adapters/ruby.rs`):
- Detected extensions: `rb`, `rake`, `gemspec` (fixtures use `.rb`).
- Name extraction:
  - `module `: `rest.split_whitespace().next()` then `.split("::").next()`.
  - `class `: same as module.
  - `def `: `rest.split(['(', ' ', ';']).next()`.
  - constant: `trimmed.split('=').next().trim()`, emitted only if every char is
    `is_ascii_uppercase()` or `_` and non-empty.
- Exact kind strings (copied from source): `"module"`, `"class"`, `"method"`,
  `"constant"`.
- Branch order is `module` -> `class` -> `def` -> constant; first match wins.
  Constant fixtures therefore use a line that has none of the `module`/`class`/
  `def` prefixes.

## Pair 1 — `module_to_class`
- NAME held constant: `Widget`
- `old_kind -> new_kind`: `module -> class`
- Breaking-policy hint: `depends` — `include`/`extend` only work on modules and
  subclassing (`<`) only on classes, so breakage hinges on how `Widget` is used;
  a plain constant reference (`Widget::X`) keeps resolving either way.
- Kind strings relied on: `"module"` -> `"class"`.
- Uncertainty: low. Both branches share the identical name-extraction path; only
  the leading keyword (`module` vs `class`) differs. Cannot run the parser to
  confirm, but the same-name/different-kind condition is met by construction.

## Pair 2 — `class_to_constant`
- NAME held constant: `THING`
- `old_kind -> new_kind`: `class -> constant`
- Breaking-policy hint: `yes` — replacing a class with a constant assignment
  drops instantiation (`THING.new`), subclassing, and class-method dispatch that
  consumers of a class rely on.
- Kind strings relied on: `"class"` -> `"constant"`.
- Uncertainty: low. `THING` is all-uppercase so it satisfies the constant
  guard, and the `after` line has no `module`/`class`/`def` prefix, so only the
  constant branch fires. The `class` symbol present in `before` is gone in
  `after`, leaving `THING` as same-name/different-kind. Cannot run the parser.

## Pair 3 — `module_to_constant`
- NAME held constant: `API`
- `old_kind -> new_kind`: `module -> constant`
- Breaking-policy hint: `yes` — once `API` is a non-module constant,
  namespacing (`API::Foo`) and `include API`/`extend API` stop working, which is
  the common use of a module.
- Kind strings relied on: `"module"` -> `"constant"`.
- Uncertainty: low. Same reasoning as Pair 2 for the constant branch (all-caps
  name, no class/module/def prefix on the `after` line). Cannot run the parser.

## Pair 4 — `control` (must NOT fire)
- NAME held constant: `Greeter`
- `old_kind -> new_kind`: `same_kind (control)` — `class -> class`
- Breaking-policy hint: `no` — declaration is unchanged; only a comment line was
  added, so consumers are unaffected and no `modified` symbol should be emitted.
- Kind strings relied on: `"class"` (unchanged); helper `def hello` stays
  `"method"` in both.
- Uncertainty: low. The added `# greeting entry point` line has no `module`/
  `class`/`def` prefix and contains no `=`, so it emits no symbol; the symbol
  lists for `before` and `after` are identical (`Greeter:class`, `hello:method`),
  so the shared diff should report zero `modified` entries. Cannot run the
  parser to confirm.
