# OCaml `signature/` corpus (rev 15)

OCaml emits a **bare-identifier** `name` for `let` bindings (`add`) via `first_ocaml_name`, with
`kind = "let"`; the whitespace-separated arguments (`x` / `x y`) are not part of `name`, so
before rev 15 an arity change was invisible. rev 15 records `signatures[name]` as the tokens
between the binding name and `=` (body-independent; the right-hand side after `=` is not
captured). A value binding with no arguments (`let x = 1`) records no signature (graceful);
`val` type annotations and `type` declarations are out of scope for the side-channel.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `x` -> `x y` | **yes** — OCaml arity is fixed; `add` now expects two arguments, breaking 1-arg callers. |

Notes:
- Only the argument list changes (the `x` → `x y` between `add` and `=`); the body change is not
  captured, so it does not drive a separate diff. No other symbol is emitted from these files.
- Registers as `modified`, NOT `removed`; `OcamlAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
