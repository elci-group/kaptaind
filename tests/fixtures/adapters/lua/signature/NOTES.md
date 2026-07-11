# Lua `signature/` corpus (rev 12)

Lua emits a **bare-identifier** `name` for module functions (`M.add`) via `extract_function_name`
(which stops at `(`), with `kind = "function"`; the parameter list is not part of `name`, so
before rev 12 an arity change was invisible. rev 12 records `signatures[name]` as the balanced
parameter list `( … )` (body-independent; the `return … end` body is not captured), so arity /
parameter changes register as `modified` while the stable name is preserved.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `M.add` | `(a)` -> `(a, b)` | **yes** — Lua callers passing one argument leave `b` nil; arity widening is observable. |

Notes:
- `local M = {}` and `return M` emit no symbol; only the `M.add` function signature change
  drives `modified`. (`M.add = function(...)`, the assignment form, is emitted by the
  `module_export` branch and is out of scope for the signature side-channel.)
- Registers as `modified`, NOT `removed`; `LuaAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
