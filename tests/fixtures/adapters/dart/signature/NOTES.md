# Dart `signature/` corpus (rev 11)

Dart emits a **bare-identifier** `name` for top-level functions (`add`) via
`extract_top_level_function_name` + `clean_identifier`, with `kind = "function"`; the parameter
list is not part of `name`, so before rev 11 an arity change was invisible. rev 11 records
`signatures[name]` as the balanced parameter list `( … )` (body-independent; the `=> …` body is
not captured), so arity / parameter-type changes register as `modified` while the stable name
is preserved.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `(int a)` -> `(int a, int b)` | **yes** — adding a required positional parameter breaks every call site. |

Notes:
- The Dart adapter only emits TOP-LEVEL functions (indented class members are skipped), so the
  pair uses a top-level `int add(...)`.
- Registers as `modified`, NOT `removed`; `DartAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
