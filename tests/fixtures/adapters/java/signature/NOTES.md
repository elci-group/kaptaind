# Java `signature/` corpus (rev 8)

Java emits a **bare-identifier** `name` for methods (e.g. `add`) with `kind = "method"`; the
parameter list is not part of `name`, so before rev 8 an arity change was invisible (same
name, same kind). rev 8 populates `signatures[name]` with the method signature (from the
first `(`, trailing `{` stripped), so arity / parameter changes register as `modified` while
the stable name is preserved.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `(int a)` -> `(int a, int b)` | **yes** — adding a required parameter breaks every call site in Java (compile error). |

Notes:
- `public class Calc` is present on both sides and unchanged, so it emits no diff entry; only
  the `add` method signature change drives `modified`.
- This registers as `modified`, NOT `removed`; `JavaAdapter::detect_breaking_changes` keys
  off `removed`, so it is intentionally **not** auto-breaking (breaking policy for signature
  changes is the deferred, gold-gated decision — see CALIBRATION.md).
