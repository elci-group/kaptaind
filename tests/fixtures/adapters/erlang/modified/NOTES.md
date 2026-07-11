# Erlang adapter — `modified` fixture notes

The shared diff engine flags a symbol as `modified` when its NAME is unchanged
but its KIND changes (same name, different `kind`).

The Erlang adapter (`src/diff/lang/adapters/erlang.rs`) extracts a bare atom
token as the NAME for three of its kinds, so those three can collide on the
same name while changing kind:

- `-module(NAME).`   -> name = token between `-module(` and `)`, kind `"module"`.
- `-record(NAME, ..)`-> name = token between `-record(` and the first `,`, kind `"record"`.
- `-define(NAME, ..)`-> name = token between `-define(` and the first `,`, kind `"macro"`.

The fourth kind, `"function"`, stores its name qualified with arity
(e.g. `start/0`), so it can never share a name with the bare-atom kinds and is
therefore unusable for a same-name/different-kind pair here.

All fixtures use the `.erl` extension (adapter detects `erl` and `hrl`).

## Pairs

### 1. module_to_record
- NAME held constant: `state`
- `module -> record`
- Breaking-policy hint: **yes** — a module `state` is called as `state:fun/1`,
  while a record `state` is used as `#state{}` after `-include`; the call
  surface and the include-time syntax are entirely different, so consumers
  break.
- Kind strings relied on: `"module"`, `"record"`.
- Uncertainty: low. `-module(state).` extracts `state` (split on `)`); the
  after-file `-record(state, {count :: integer()}).` extracts `state` (split on
  first `,`). Both yield kind-bearing symbols of name `state`.

### 2. module_to_macro
- NAME held constant: `limit`
- `module -> macro`
- Breaking-policy hint: **depends** — a module `limit` and a macro `?limit`
  live in different namespaces (runtime calls vs compile-time substitution);
  pure `?MODULE` users are unaffected, but anyone calling `limit:fun/1` breaks.
- Kind strings relied on: `"module"`, `"macro"`.
- Uncertainty: low-to-medium. `-define(limit, 100).` uses a lowercase macro
  name, which is valid Erlang (atom) but unconventional (macros are usually
  uppercase). The adapter splits on the first `,` and trims, so it emits
  `limit`; risk is only if a consumer expected uppercase-by-convention, not in
  the parse itself.

### 3. record_to_macro
- NAME held constant: `config`
- `record -> macro`
- Breaking-policy hint: **yes** — code matching/building `#config{}` breaks
  when the record becomes a macro `?config`; records and macros are not
  interchangeable at use sites.
- Kind strings relied on: `"record"`, `"macro"`.
- Uncertainty: low. Both directives split the NAME on the first `,`; the
  after-file `-define(config, default).` yields name `config` to match the
  record's `config`.

### 4. control (control_before / control_after)
- NAME held constant: `state`
- `same_kind (control)` — record in both; only the field list (body) grows by
  one field. Name and kind are unchanged.
- Breaking-policy hint: **no** — same name and same kind, so the engine must
  NOT emit a `modified` symbol; this guards against over-firing on body edits.
- Kind strings relied on: `"record"` (both sides).
- Uncertainty: low. The adapter only reads up to the first `,` for a record's
  name, so adding `, name :: atom()` to the field list does not change the
  emitted name (`state`) or kind (`record`); the symbol set is identical.
