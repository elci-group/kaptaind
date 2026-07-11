# F# `signature/` corpus (rev 15)

F# emits a **bare-identifier** `name` for `let` bindings (`add`) via `take_identifier` inside
`parse_decl` (kind `"value"`), after skipping modifiers and `[<…>]` attributes; the
whitespace-separated arguments (`x` / `x y`) are not part of `name`, so before rev 15 an arity
change was invisible. rev 15 records `signatures[name]` as the tokens between the binding name
and `=` (body-independent; the right-hand side after `=` is not captured), computed from the
attribute-stripped line so `[<Literal>]`-style prefixes don't shift the name. A value binding
with no arguments (`let myValue = 1`) records no signature (graceful).

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `x` -> `x y` | **yes** — F# arity is fixed; `add` now expects two arguments, breaking 1-arg callers. |

Notes:
- Only the argument list changes (`x` → `x y` between `add` and `=`); the body is not captured.
  `private`/`internal` bindings and `[<…>]`-prefixed values behave as before (no over-capture).
- Registers as `modified`, NOT `removed`; `FsharpAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
