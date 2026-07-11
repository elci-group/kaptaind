# C# `signature/` corpus (rev 9)

C# emits a **bare-identifier** `name` for methods (`Add`) with `kind = "method"`; the
parameter list is not part of `name`, so before rev 9 an arity change was invisible. rev 9
records `signatures[name]` as the balanced parameter list `( … )` (body-independent, so
expression-bodied methods don't leak the body), so arity / parameter-type changes register
as `modified` while the stable name is preserved.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `Add` | `(int a)` -> `(int a, int b)` | **yes** — adding a required parameter breaks every call site in C# (compile error). |

Notes:
- `public class Calc` is unchanged on both sides (no diff entry); only the `Add` signature
  change drives `modified`.
- Registers as `modified`, NOT `removed`; `CsharpAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
