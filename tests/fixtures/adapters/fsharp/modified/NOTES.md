# F# adapter — `modified` (kind-change) fixture notes

These pairs exercise the shared `modified` diff signal: **same symbol `name`,
different `kind`**. The adapter (`src/diff/lang/adapters/fsharp.rs`) is
line-based; it emits exactly three `kind` strings and extracts `name` as the
first identifier token immediately after the declaration keyword (after
stripping leading `[<...>]` attributes and the modifiers `rec` / `inline` /
`mutable` / `global`). Each pair keeps that post-keyword token byte-identical
and changes only the kind-bearing keyword.

Exact `kind` strings relied on (copied from the adapter):
- `"module"` — `module ...`  (`parse_fsharp_line`, `strip_prefix("module ")`)
- `"type"`   — `type ...`    (`strip_prefix("type ")`)
- `"value"`  — `let ...` and `val ...` (`strip_prefix("let ")` / `strip_prefix("val ")`)

Detected extension used for every file here: `.fs` (adapter also accepts
`.fsx`, `.fsi`).

## Pair 1 — `widget`
- NAME held constant: `Widget`
- old_kind -> new_kind: `module` -> `type`
- breaking-policy hint: **yes** — `module Widget` is a namespace reached by
  member access (`Widget.create`); `type Widget` is a type used in annotations
  and pattern matching (`Widget.Small`). Swapping the role breaks both member
  access and any `: Widget` type usage.
- kind strings used: `"module"`, `"type"`
- uncertainty: low. The `module` body also contributes a `create`/`value`
  symbol that disappears on the `type` side; that is a separate `removed`
  signal and does not affect the name-keyed `modified` entry for `Widget`.
  Could not run the parser to confirm.

## Pair 2 — `max_retries`
- NAME held constant: `MaxRetries`
- old_kind -> new_kind: `type` -> `value`
- breaking-policy hint: **yes** — `type MaxRetries` was a DU consumed via
  `MaxRetries.Default` / `Custom of int` and as a type in signatures; after the
  change `MaxRetries` is an integer literal value, so all case matches and
  `: MaxRetries` annotations break.
- kind strings used: `"type"`, `"value"`
- uncertainty: low. The after-side relies on same-line attribute stripping
  (`[<Literal>] let MaxRetries = 3`) so the name is captured after `[<Literal>]`
  is removed; the proven `positive/attributes.fs` fixture already exercises
  same-line `[<...>] let ...` stripping, so confidence is high. An uppercase
  `let` is only warning-free/idiomatic as a `[<Literal>]`, which is why that
  form is used (a plain uppercase `let` would lint FS0049). Could not run the
  parser to confirm.

## Pair 3 — `default_timeout`
- NAME held constant: `DefaultTimeout`
- old_kind -> new_kind: `module` -> `value`
- breaking-policy hint: **yes** — `module DefaultTimeout` was reached as
  `DefaultTimeout.milliseconds`; after the change `DefaultTimeout` is a single
  integer literal, so every qualified member access breaks.
- kind strings used: `"module"`, `"value"`
- uncertainty: low. Same `[<Literal>] let` same-line-stripping note as Pair 2.
  The `module` body contributes a `milliseconds`/`value` symbol that is absent
  on the after side (separate `removed` signal, unrelated to the `modified`
  entry for `DefaultTimeout`). Could not run the parser to confirm.

## Pair 4 — `control`
- NAME held constant: `bootstrap` (plus the enclosing `Control` module)
- old_kind -> new_kind: `value` -> `value` (same_kind; control)
- breaking-policy hint: **no** — name and kind are unchanged; only the integer
  body (`1` -> `2`) and a comment differ, which the line-based parser does not
  model. This pair must yield NO `modified` symbol and guards against
  over-firing.
- kind strings used: `"value"` (and `"module"` for the unchanged `Control`)
- uncertainty: low. Both sides emit identical (`name`, `kind`) pairs, so the
  expected `modified` set is empty. Could not run the parser to confirm.
