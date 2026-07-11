# Swift `modified` fixture notes

Source of truth: `src/diff/lang/adapters/swift.rs` (read-only). The adapter is a per-line
scanner: a symbol is emitted only when `line.trim()` `starts_with("public ")`/`"open "`, the
visibility prefix is stripped, and the remainder is matched against a fixed keyword list. The
captured `name` is the ENTIRE remainder of the line after the matched keyword (no truncation of
params/body). `detect_files` matches extension exactly `swift`.

`name` identity is the whole trick here: because `name` = the literal post-keyword text, keeping
the `name` byte-identical while changing the `kind` requires changing ONLY the keyword and
leaving every character after it identical. That is only possible for keywords whose trailing
grammar can share an identical token shape — here the type-level keywords `class`/`struct`/
`enum`/`protocol`, all of which accept the empty-body form `<Name> {}`. `func` (needs `()`),
`var`/`let` (need `: Type` and both map to the SAME kind `property`), and `typealias` (needs
`= Type`) cannot share an identical remainder with any other keyword, so they cannot form a
same-name/different-kind pair in this adapter.

## Pairs

| pair | file (before -> after) | NAME held constant | old_kind -> new_kind |
|------|------------------------|--------------------|----------------------|
| class_to_struct | `class_to_struct_before.swift` -> `class_to_struct_after.swift` | `Container {}` | `class` -> `struct` |
| struct_to_protocol | `struct_to_protocol_before.swift` -> `struct_to_protocol_after.swift` | `Shape {}` | `struct` -> `protocol` |
| class_to_enum | `class_to_enum_before.swift` -> `class_to_enum_after.swift` | `State {}` | `class` -> `enum` |
| control | `control_before.swift` -> `control_after.swift` | `Router {` | same_kind (control): `class` -> `class` |

### class_to_struct — `class` -> `struct`
- NAME held constant: `Container {}` (declaration lines `public class Container {}` vs
  `public struct Container {}` differ only in the keyword; remainder `Container {}` is identical).
- Breaking-policy hint: **yes** — class-to-struct switches reference semantics to value semantics,
  drops identity (`===`)/inheritance/`deinit`, so consumers relying on shared references or
  subclassing break.
- `kind` strings relied on: `"class"`, `"struct"` (adapter lines 39-48).

### struct_to_protocol — `struct` -> `protocol`
- NAME held constant: `Shape {}` (`public struct Shape {}` vs `public protocol Shape {}`).
- Breaking-policy hint: **yes** — a concrete type becoming a protocol can no longer be
  instantiated directly (`Shape()` fails); consumers must introduce their own conforming types.
- `kind` strings relied on: `"struct"`, `"protocol"` (adapter lines 44-48, 54-58).

### class_to_enum — `class` -> `enum`
- NAME held constant: `State {}` (`public class State {}` vs `public enum State {}`).
- Breaking-policy hint: **yes** — class-to-enum removes reference semantics/subclassing and, with
  no cases here, the type is uninhabited and cannot be constructed at all, so any `State()` call
  site breaks.
- `kind` strings relied on: `"class"`, `"enum"` (adapter lines 39-43, 49-53).

### control — same_kind (control): `class` -> `class`
- NAME held constant: `Router {` (the declaration line `public class Router {` is byte-identical
  in both files; only the non-`public` body lines change, which the scanner ignores).
- Breaking-policy hint: **no** — body-only edit, declaration (name + kind) unchanged; not breaking.
- `kind` strings relied on: `"class"` (adapter lines 39-43).
- Expected modified signal: **none** (same name, same kind). Guards against over-firing.

## `kind` strings used (copied verbatim from `swift.rs`)
`"function"`, `"class"`, `"struct"`, `"enum"`, `"protocol"`, `"property"`, `"typealias"`,
`"objc_export"`. This set of fixtures exercises `class`, `struct`, `protocol`, `enum`.

## Uncertainties (parser not run)
- The same-name guarantee rests entirely on `name` being the untruncated remainder after the
  keyword (source lines 34-73; corroborated by the parent `../NOTES.md`). On that reading the
  three kind-change pairs are genuinely same-name/different-kind. Risk: low, but unverified by
  execution.
- `var`/`let` were deliberately NOT used: both emit kind `"property"`, so a `var`<->`let` swap is
  a same-kind change and would NOT exercise `modified`. `func`/`typealias` were not used because
  their required trailing tokens (`()` / `= Type`) cannot be reproduced by another keyword, so a
  same-name pair is impossible for this scanner.
- Biggest caveat: the Swift adapter's `diff_ast` delegates to `basic_diff`, which the existing
  `../NOTES.md` documents as name-keyed only ("`modified` is never populated"). These pairs are
  built to the task's stated contract (name unchanged, kind changed) so a *shared* diff engine
  that does consider `kind` can fire `modified`; but this adapter's own diff may not surface it.
  The fixtures are still valid as a same-name/different-kind corpus regardless.
