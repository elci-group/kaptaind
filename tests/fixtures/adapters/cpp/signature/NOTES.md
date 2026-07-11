# C++ `signature/` corpus (rev 13)

C++ emits a **bare-identifier** `name` for function definitions (`add`) via
`extract_function_definition` (token before `(`, skipping control-flow/keyword prefixes and
`;`-terminated declarations), with `kind = "function"`; the parameter list is not part of
`name`, so before rev 13 an arity / parameter-type change was invisible. rev 13 records
`signatures[name]` as the balanced parameter list `( … )` (body-independent; the `{ … }` body is
not captured), so arity / parameter-type changes register as `modified` while the stable name
is preserved.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `(int a)` -> `(int a, int b)` | **yes** — changes the overload set / mangled symbol's contract for callers. |

Notes:
- The adapter only emits DEFINITIONS (declarations ending with `;` are skipped), so the pair
  uses a definition form (`int add(...) { … }`). Only the parameter list changes.
- Registers as `modified`, NOT `removed`; `CppAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
