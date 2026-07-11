# PHP `signature/` corpus (rev 10)

PHP emits a **bare-identifier** `name` for functions and methods (`add`) with `kind =
"function"` / `"method"`; the parameter list is not part of `name`, so before rev 10 an arity
change was invisible (same name, same kind). rev 10 records `signatures[name]` as the balanced
parameter list `( … )` (body-independent, so the `{ … }` body and any `: returnType` are not
captured), so arity / parameter-type changes register as `modified` while the stable name is
preserved.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `(int $a)` -> `(int $a, int $b)` | **yes** — adding a required parameter breaks every call site (ArgumentCountError). |

Notes:
- `namespace App` and `class Calc` are present on both sides and unchanged, so they emit no
  diff entry; only the `add` method signature change drives `modified`. (PHP classes have no
  visibility modifier and are public surface by default — consistent with the positive corpus.)
- Registers as `modified`, NOT `removed`; `PhpAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
