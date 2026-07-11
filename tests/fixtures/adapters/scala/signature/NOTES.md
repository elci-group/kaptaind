# Scala `signature/` corpus (rev 11)

Scala emits a **bare-identifier** `name` for `def` (`add`) via `extract_identifier` (which stops
at `(`), with `kind = "def"`; the parameter list is not part of `name`, so before rev 11 an
arity change was invisible. rev 11 records `signatures[name]` as the balanced parameter list
`( … )` (body-independent; the `=` body and the `: returnType` are not captured), so arity /
parameter-type changes register as `modified` while the stable name is preserved.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `(a: Int)` -> `(a: Int, b: Int)` | **yes** — adding a required parameter breaks every call site (compile error). |

Notes:
- `object Calc` is unchanged on both sides (no diff entry); only the `add` signature change
  drives `modified`.
- Registers as `modified`, NOT `removed`; `ScalaAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
