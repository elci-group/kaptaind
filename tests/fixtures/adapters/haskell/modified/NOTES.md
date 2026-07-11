# Haskell adapter — `modified` (kind-change) fixture notes

These before/after pairs exercise the shared `modified` diff signal: the symbol
NAME is held byte-identical across a pair while the adapter emits a DIFFERENT
`kind`. Extension is `.hs` (adapter `detect_files` matches `"hs" | "lhs"`).

The Haskell adapter (`src/diff/lang/adapters/haskell.rs`) emits exactly these
type-level `kind` strings via `first_type_token` (name = first whitespace token
after the keyword, after any `=>` context): `"data"`, `"newtype"`, `"class"`,
`"type"`. (It also emits `"function"`, but a function name must be lowercase and
a type name must be uppercase, so function↔type transitions cannot share a name;
the four type-level kinds are what this corpus uses.)

## Pair 1 — `widget` (data -> newtype)
- NAME held constant: `Widget`
- old_kind -> new_kind: `data` -> `newtype`
- breaking-policy hint: `depends` — source-level pattern matches on the single
  constructor still line up, but `newtype` changes representation/strictness and
  enables `coerce`, while `data` permits extra constructors; downstream ABI and
  `Generic`/`coerce` assumptions may differ.
- kind strings relied on: `"data"`, `"newtype"`

## Pair 2 — `converter` (class -> type)
- NAME held constant: `Converter`
- old_kind -> new_kind: `class` -> `type`
- breaking-policy hint: `yes` — replacing a type class with a type alias drops
  the class, its `convert` method, all instances, and every `Converter a =>`
  constraint consumers depend on.
- kind strings relied on: `"class"`, `"type"`

## Pair 3 — `status` (type -> data)
- NAME held constant: `Status`
- old_kind -> new_kind: `type` -> `data`
- breaking-policy hint: `yes` — a type alias is transparently interchangeable
  with its RHS (`String`); making it a nominal `data` type breaks that
  substitution and requires constructors.
- kind strings relied on: `"type"`, `"data"`

## Pair 4 — `control` (same_kind control)
- NAME held constant: `Config`
- old_kind -> new_kind: `same_kind (control)` — `data` -> `data`
- breaking-policy hint: `no` — same name and kind; only a record field was
  added inside the body, so the adapter must emit NO `modified` symbol (guards
  against over-firing).
- kind strings relied on: `"data"`

## Uncertainty (parser not run)
- I could not execute the parser, so I am relying on the source logic: for each
  pair the declaration is top-level (column 0), the keyword is reserved so the
  function-detection branch skips it, and `first_type_token` returns the first
  token after the keyword as the name. Names should therefore be identical
  within each pair.
- In `converter_before.hs` the method line `convert :: a -> String` is indented,
  so the `is_top_level` guard should exclude it (consistent with the
  `ignores_nested_local_bindings` unit test); it should not leak as a
  `"function"` symbol.
- In `status_after.hs` only the LHS name `Status` is captured (constructors
  `Active`/`Inactive` are not top-level symbols), mirroring how
  `data Result a = Ok a | Err String` yields only `Result`.
- Control pair adds a record field inside the body; kind and name must stay
  `data`/`Config`. No uncertainty expected there.
