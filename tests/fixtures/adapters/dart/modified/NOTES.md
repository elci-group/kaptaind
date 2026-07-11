# dart `modified` fixture notes

Shared signal: the diff engine flags a symbol as `modified` when its NAME is
unchanged but its KIND changes (same name, different `kind`).

Adapter facts used (from `src/diff/lang/adapters/dart.rs`):
- Extension detected: `.dart` only (`detect_files` filters `extension() == "dart"`).
- Emitted `kind` strings: `"class"`, `"enum"`, `"extension"`, `"mixin"`, `"function"`.
- Top-level only: lines starting with space/tab (members) and `//` / blank lines are skipped.
- One symbol per line via an ordered if/else chain: class -> enum -> extension -> mixin -> function.
- Name extraction for the type-like kinds is "first whitespace token after the keyword",
  then `clean_identifier` (split on `<`, trim trailing `{`). `extension` returns the raw
  first token (no `clean_identifier`), but for a plain identifier it equals the cleaned form.
  All four type kinds use UpperCamelCase names, so holding the name identical is realistic.

## Pair: class_to_enum
- NAME held constant: `Shape`
- old_kind -> new_kind: `class` -> `enum`
- breaking-policy hint: yes — consumers that constructed `Shape()` or used `extends`/
  `implements Shape` break; an enum cannot be instantiated or subclassed like a class.
- kind strings relied on: `"class"` (extract_class_name), `"enum"` (extract_enum_name).
- uncertainty: low. `class Shape {` matches the class branch first; `enum Shape {` skips
  class (no `class ` prefix) and matches enum. Member/value lines are indented (skipped);
  closing `}` emits nothing (no `(` for the function branch).

## Pair: enum_to_mixin
- NAME held constant: `Color`
- old_kind -> new_kind: `enum` -> `mixin`
- breaking-policy hint: yes — enum value access (`Color.red`) and exhaustive switches no
  longer exist; a mixin is consumed via `with`/`on`, a wholly different usage site.
- kind strings relied on: `"enum"` (extract_enum_name), `"mixin"` (extract_mixin_name).
- uncertainty: low. `mixin Color {` is NOT `mixin class ` (which the extractor routes back
  to class), so it yields kind `mixin`. Chain order ensures enum/mixin don't collide.

## Pair: class_to_extension
- NAME held constant: `Helpers`
- old_kind -> new_kind: `class` -> `extension`
- breaking-policy hint: yes — `Helpers.twice(...)` static call and any instantiation break;
  the extension's members become instance methods on the receiver type (`String`), not on
  `Helpers`.
- kind strings relied on: `"class"` (extract_class_name), `"extension"`
  (extract_extension_name).
- uncertainty: low-to-moderate. The extension name is returned RAW (no `clean_identifier`);
  here the first token `Helpers` is plain, so raw equals the cleaned class name. If the
  extension token carried generics/`{` it could diverge, but it does not here.

## Pair: control (no modified expected)
- NAME held constant: `Counter`
- old_kind -> new_kind: same_kind (control) — `class` -> `class`
- breaking-policy hint: no — declaration name and kind unchanged; only an indented member
  body (`+= 1` -> `+= 2`) differs, and indented lines are skipped by the parser.
- kind strings relied on: `"class"` == `"class"`.
- uncertainty: low. The top-level `class Counter {` line is byte-identical across the pair,
  so the emitted symbol is identical; the body change lives on an indented (skipped) line.

## General uncertainty (applies to all pairs)
I cannot run the parser here. I rely on (a) the task's statement that the engine flags
same-name/different-kind as `modified`, and (b) the adapter's ordered if/else chain emitting
exactly one symbol per top-level declaration. If a future adapter change reorders the chain
or makes `clean_identifier` stricter, the extension pair would be the first to drift.
