# csharp `modified` fixture notes

Shared signal: a symbol is `modified` when its NAME is unchanged but its KIND
changes (same name, different `kind`). Each pair below keeps the extracted
symbol NAME byte-identical and swaps only the kind-bearing keyword/syntax.

Adapter facts (from `src/diff/lang/adapters/csharp.rs`):

- Extension detected: `.cs` (`detect_files` filters `extension() == "cs"`).
- Type kinds come from `type_keyword()` and are emitted verbatim via
  `kind: keyword.to_string()`: `"class"`, `"interface"`, `"struct"`, `"enum"`.
  A type is only emitted when `has_public_modifier()` is true (a whole-word
  `public` token) AND the declaration line contains one of those keywords.
- Member kinds come from `extract_public_member()` and are emitted via
  `kind: kind.to_string()`: `"method"` (identifier immediately before `(`),
  `"property"` (expression body `=>`, or a `{` getter/setter block).
- Type NAME = the whitespace token immediately following the type keyword
  (`extract_type_name`), truncated at the first of `< : { ( ;`.
- Member NAME = the last whitespace token before `(` / `=>` / `{`
  (`extract_public_member`), with generics stripped.
- Note: a type-declaration line is consumed by the type branch (`continue`)
  and is NOT re-checked by the member branch, so a type never double-emits.
- Public fields (e.g. `public int X;`) are NOT emitted by this adapter.

## Pair 1 — `type_class_to_interface`

- Files: `type_class_to_interface_before.cs` / `type_class_to_interface_after.cs`
- NAME held constant: `Repository`
- `old_kind -> new_kind`: `class -> interface`
- Breaking-policy hint: `yes` — `new Repository()`, inheritance, and any
  member-with-body usage stop compiling when the type becomes an interface.
- Kind strings relied on (copied from source): `"class"`, `"interface"`.
- Uncertainty: low. Both lines are `public <kw> Repository`; `public` satisfies
  `has_public_modifier`, the keyword is in the type-keyword set, and the token
  after the keyword is `Repository` in both. The `{` is on the next line, so
  `extract_type_name` returns `Repository` either way (truncation is moot).

## Pair 2 — `type_struct_to_enum`

- Files: `type_struct_to_enum_before.cs` / `type_struct_to_enum_after.cs`
- NAME held constant: `Status`
- `old_kind -> new_kind`: `struct -> enum`
- Breaking-policy hint: `yes` — value-type vs. enum underlying type,
  assignment/comparison semantics, and member access all change for consumers.
- Kind strings relied on (copied from source): `"struct"`, `"enum"`.
- Uncertainty: low. Same reasoning as Pair 1; the keyword swap is on the same
  declaration line and the following token is `Status` in both. Empty bodies
  (`struct`/`enum { }`) are line-based and do not affect extraction.

## Pair 3 — `member_method_to_property`

- Files: `member_method_to_property_before.cs` / `member_method_to_property_after.cs`
- NAME held constant: `Count` (inside an identical `public class Widget`)
- `old_kind -> new_kind`: `method -> property`
- Breaking-policy hint: `yes` — call sites `w.Count()` no longer compile and
  must become `w.Count`; source- and binary-breaking for callers.
- Kind strings relied on (copied from source): `"method"`, `"property"`.
- Mechanism: before line `public int Count()` matches the `(` branch -> kind
  `"method"`, name last token before `(` = `Count`. After line
  `public int Count { get; private set; }` has no `(` and no `=>`, so it falls
  to the `{` branch -> kind `"property"`, name last token before `{` = `Count`.
- Uncertainty: low-to-moderate. The containing `public class Widget` is
  byte-identical across the pair, so it emits `(Widget, "class")` in BOTH and
  must NOT be reported as modified; only `Count` should fire. Moderate only
  because the pair now emits two symbols per side and relies on the shared
  diff to isolate the kind change on `Count`.

## Pair 4 — `control`

- Files: `control_before.cs` / `control_after.cs`
- NAME held constant: `Service` (type) and `Run` (member)
- `old_kind -> new_kind`: `same_kind (control)` — `class -> class`,
  `method -> method`
- Breaking-policy hint: `no` — no API-surface change; only a comment/whitespace
  delta inside the method body.
- Kind strings relied on (copied from source): `"class"`, `"method"`.
- Uncertainty: low. Both sides emit exactly `(Service, "class")` and
  `(Run, "method")`; the only difference (`{ }` vs `{ // no-op }`) is on lines
  the adapter does not emit. This pair MUST yield zero modified symbols.

## General uncertainty (applies to all pairs)

I did not run the parser or the diff engine (per task rules: no `cargo`).
Confidence is based on reading the line-based scanner only. The shared
`basic_diff` is assumed to implement the documented same-name/different-kind
rule for `modified`; if its actual definition differs, the `modified` outcome
for Pairs 1-3 and the zero-result guarantee for Pair 4 would not hold.
