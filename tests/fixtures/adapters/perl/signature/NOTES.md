# Perl `signature/` corpus (rev 13)

Perl emits a **bare-identifier** `name` for subs (`add`) via `rest.split(whitespace|(|{|;|:)`,
with `kind = "sub"`; the subroutine signature (`($a)`) is not part of `name`, so before rev 13 an
arity change was invisible. rev 13 records `signatures[name]` as the balanced parameter list
`( … )` for subs that declare a Perl signature (Perl 5.36+); classic `sub foo {` has no `(` and
records no signature (graceful no-op). Body-independent; the `{ … }` body is not captured.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `($a)` -> `($a, $b)` | **yes** — signature arity is enforced at runtime (too few/many arguments). |

Notes:
- `package Calc;` is unchanged on both sides (no diff entry); only the `add` signature change
  drives `modified`.
- Registers as `modified`, NOT `removed`; `PerlAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
