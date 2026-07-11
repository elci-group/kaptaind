# C `signature/` corpus (rev 13)

C emits a **bare-identifier** `name` for functions (`add`) via the "token before `(`" heuristic
(with a valid return-type token before it), with `kind = "function"`; the parameter list is not
part of `name`, so before rev 13 an arity / parameter-type change was invisible. rev 13 records
`signatures[name]` as the balanced parameter list `( … )` (body-independent; declarations end
with `;`, definitions have `{` after the list), so arity / parameter-type changes register as
`modified` while the stable name is preserved.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `(int a)` -> `(int a, int b)` | **yes** — C has no overloading; changing the parameter list changes the symbol's contract. |

Notes:
- The pair uses a prototype form (`int add(...);`); only the parameter list changes, driving
  `modified`. No other symbol is emitted from these files.
- Registers as `modified`, NOT `removed`; `CAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
