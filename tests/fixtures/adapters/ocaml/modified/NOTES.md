# OCaml `modified` fixture notes

Kind strings are copied verbatim from `src/diff/lang/adapters/ocaml.rs`
(`kind: "..."`.to_string()` sites): `"module_type"`, `"module"`, `"let"`,
`"type"`, `"val"`. The symbol `name` is the first identifier token after the
keyword prefix, extracted by `first_ocaml_name` (skips leading type params
`'a`/`(...)`, skips a leading `rec`, and trims trailing `, ) = :`). Detected
extensions: `.ml`, `.mli`.

## Pair: module_to_module_type  (`module_to_module_type_before.ml` -> `_after.ml`)
- NAME held constant: `Greeter`
- old_kind -> new_kind: `module` -> `module_type`
- breaking-policy hint: **yes** — `Greeter` goes from a usable module
  (`Greeter.x`) to a signature, which cannot be referenced as a value/module,
  so consumers of `Greeter` break.
- exact kind strings relied on: `"module"`, `"module_type"`
- uncertainty: low. `module Greeter` is matched by the `module ` branch
  (the `module type ` branch is checked first and does NOT match here), and
  `module type Greeter` is matched by the `module type ` branch; both yield
  name `Greeter` via `first_ocaml_name`. Confident same-name/different-kind.

## Pair: type_to_let  (`type_to_let_before.ml` -> `_after.ml`)
- NAME held constant: `count`
- old_kind -> new_kind: `type` -> `let`
- breaking-policy hint: **yes** — a type alias replaced by a same-named value
  breaks any code that used `count` as a type (e.g. `let x : count = ...`),
  even though OCaml's separate namespaces would otherwise allow both to
  coexist.
- exact kind strings relied on: `"type"`, `"let"`
- uncertainty: low. Both `type count = int` and `let count = 0` are valid
  OCaml and the parser extracts `count` as the first identifier token in each;
  case rules that differ between real OCaml namespaces are not enforced by
  this text-based adapter, so the name is byte-identical.

## Pair: val_to_type  (`val_to_type_before.mli` -> `_after.mli`)
- NAME held constant: `name`
- old_kind -> new_kind: `val` -> `type`
- breaking-policy hint: **yes** — an exported value `val name : string`
  becomes a type `type name = string`; consumers using `name` as a value
  break (and vice versa for type usage).
- exact kind strings relied on: `"val"`, `"type"`
- uncertainty: low. Both declarations are valid in an `.mli` interface, and
  the `name` token is the first identifier after each keyword in both cases.
  Note `val` is only realistic in `.mli`, hence the `.mli` extension.

## Pair: control  (`control_before.ml` -> `control_after.ml`)
- NAME held constant: `answer`
- old_kind -> new_kind: `same_kind (control)` — `let` -> `let`
- breaking-policy hint: **no** — only the right-hand-side literal changed
  (`42` -> `100`); the declaration name and kind are unchanged, so this pair
  must NOT produce a `modified` symbol (guards against over-firing).
- exact kind strings relied on: `"let"`
- uncertainty: low. The `let answer` prefix is byte-identical across the pair;
  only the value expression after `=` differs, which the parser ignores.
