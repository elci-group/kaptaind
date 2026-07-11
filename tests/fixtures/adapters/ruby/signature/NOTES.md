# Ruby `signature/` corpus (rev 12)

Ruby emits a **bare-identifier** `name` for methods (`add`) via `rest.split(['(', ' ', ';'])`,
with `kind = "method"`; the parameter list is not part of `name`, so before rev 12 an arity
change was invisible. rev 12 records `signatures[name]` as the balanced parameter list `( … )`
(body-independent; the method body is not captured, and a method with no parameter list records
no signature), so arity / parameter changes register as `modified` while the stable name is
preserved.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `(a)` -> `(a, b)` | **yes** — adding a required positional argument breaks every call site (ArgumentError). |

Notes:
- `class Calc` is unchanged on both sides (no diff entry); only the `add` signature change
  drives `modified`.
- Registers as `modified`, NOT `removed`; `RubyAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
