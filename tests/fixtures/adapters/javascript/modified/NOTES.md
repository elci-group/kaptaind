# JavaScript `modified/` corpus (rev 5)

Unreachable in rev 4 (`name = full export-line remainder`, so `name` embedded the kind
keyword). Became reachable after `javascript.rs::export_name` was introduced to extract a
stable identifier as `name`. This corpus exercises that change end-to-end.

Kind strings used (verbatim from `classify_ts_export`, `common.rs`): `class`, `function`,
`binding`. Adapter emits `name = export_name(rest)` (the declared identifier) and `kind`
from `classify_ts_export(rest)`. Extension `.js` (`detect_files`: js/jsx/cjs/mjs).

| pair | name held constant | old_kind -> new_kind | breaking-policy hint |
|------|--------------------|----------------------|----------------------|
| class_to_function | `Foo` | `class` -> `function` | **depends** — call/construct form changes (`new Foo()` vs `Foo()`); breaking if consumers construct it. |
| function_to_binding | `bar` | `function` -> `binding` (arrow `const`) | **depends** — `function` declarations hoist and bind `this`; arrow `const` does not hoist and has lexical `this`. Subtle, often non-breaking for pure callers. |
| function_to_class | `Widget` | `function` -> `class` | **yes** — callable becomes construct-only; `Widget()` without `new` now throws. |
| control | `same` | `function` -> `function` (body `1` -> `2`) | **no** — same name+kind; body-only change must NOT fire `modified`. |

Uncertainty: low. Each pair keeps the identifier token byte-identical and varies only the
kind-bearing keyword; `export_name` extracts the identifier and `classify_ts_export` yields
the distinct kind. None of the lines start with `export function use` / `export const use`,
so the hook double-emission branch is not triggered (one symbol per line).
