# Scala adapter — `modified` (kind-change) fixture pairs

Shared signal: a symbol is `modified` when its **name** is unchanged but its
**kind** changes. The adapter extracts the name as the first identifier token
immediately after the kind keyword (split on whitespace / `[` / `(` / `{` / `:` /
`=`). Each pair keeps that token byte-identical and only swaps the kind-bearing
keyword. Extension `.scala` (adapter detects `scala` | `sc`).

Adapter kind strings (copied from `src/diff/lang/adapters/scala.rs`):
`"case_class"`, `"class"`, `"object"`, `"trait"`, `"def"`.
Match order matters: `"case class "` is tested before `"class "`, so a
`case class` line never falls through to plain `class`.

## Pair 1 — `class_to_trait`
- NAME held constant: `Service`
- old_kind -> new_kind: `class` -> `trait`
- Breaking-policy hint: **yes** — a class is instantiable (`new Service()`);
  a trait is not. Constructor/instantiation call sites break, and subclasses
  move from `extends` to mixin semantics.
- Kind strings relied on: `"class"`, `"trait"`.
- Uncertainty: low. Plain `class Service` / `trait Service`; name token is the
  keyword-adjacent identifier in both, no modifiers to strip.

## Pair 2 — `case_class_to_class`
- NAME held constant: `User`
- old_kind -> new_kind: `case_class` -> `class`
- Breaking-policy hint: **yes** — dropping `case` removes the generated
  `apply`/`unapply`, `copy`, structural `equals`/`hashCode`/`toString`, and
  pattern-match extractor. Consumers using `User(...)` construction or
  `case User(x) =>` matching break.
- Kind strings relied on: `"case_class"`, `"class"`.
- Uncertainty: low. Relies on the documented `"case class "` before `"class "`
  ordering; both lines parse to name `User` (split on `(`).

## Pair 3 — `object_to_trait`
- NAME held constant: `Config`
- old_kind -> new_kind: `object` -> `trait`
- Breaking-policy hint: **yes** — an object is a singleton *value* referenced
  as `Config.port`; a trait is a *type* to be mixed in and cannot be referenced
  as a value. All `Config.x` call sites break. (`val port` -> abstract
  `def port` is incidental; the flagged symbol is the kind change on `Config`.)
- Kind strings relied on: `"object"`, `"trait"`.
- Uncertainty: low. `object Config` / `trait Config` parse directly; bodies are
  not inspected for the top-level kind.

## Pair 4 — `control` (must NOT fire)
- NAME held constant: `Counter` (and member `value`)
- old_kind -> new_kind: `same_kind (control)` — `class` -> `class`
- Breaking-policy hint: **no** — same declaration kind and name; only an
  internal literal (`0` -> `42`) inside a method body changed, which the parser
  does not surface as a kind change.
- Kind strings relied on: `"class"` (and `"def"` for the unchanged member).
- Uncertainty: low. Both `class Counter` lines are identical; both `def value`
  lines extract name `value` / kind `def`. Symbol sets are equal, so no
  `modified` entry is expected.
