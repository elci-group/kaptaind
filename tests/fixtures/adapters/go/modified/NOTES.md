# Go adapter — `modified` fixture corpus

Diff signal under test: a symbol is `modified` when its `name` is unchanged but its
`kind` changes (same name, different `kind`).

Adapter facts (from `src/diff/lang/adapters/go.rs`, read-only):

- `detect_files` keeps paths whose extension is `go` → all fixtures use `.go`.
- `name` is the ENTIRE post-keyword line text (`name: rest.to_string()`), where `rest`
  is everything after `func ` / `type ` on the trimmed line (e.g. `func Serve() {`
  yields name `Serve() {`). It is NOT just the identifier token.
- kind strings emitted (copied from source): `"function"`, `"generic_function"`,
  `"type"`, `"generic_type"`.
- kind is chosen ONLY by the line prefix: `func ` → `function` (and an extra
  `generic_function` when generic), `type ` → `type` (or `generic_type` when generic).

Because `name` embeds the whole post-keyword token stream, name-equality across a
pair forces that text to be byte-identical, so the ONLY way to also flip `kind` is to
swap the `func `/`type ` prefix. See UNCERTAINTY.

## Pairs

### 1. `serve` — `function` -> `type`
- NAME held constant: `Serve() {`
- old_kind -> new_kind: `function` -> `type`
- breaking-policy hint: `yes` — exported `pkg.Serve()` call sites and any use of
  `pkg.Serve` as a value break if it becomes a type (a type cannot be called).
- kind strings relied on: `"function"`, `"type"`

### 2. `record` — `type` -> `function`
- NAME held constant: `Record struct {`
- old_kind -> new_kind: `type` -> `function`
- breaking-policy hint: `yes` — `pkg.Record{...}` composite literals and
  `var x pkg.Record` type usages break if it becomes a function (a function cannot
  be used as a type).
- kind strings relied on: `"type"`, `"function"`

### 3. `convert` — `function` -> `type`
- NAME held constant: `Convert(s string) (int, error) {`
- old_kind -> new_kind: `function` -> `type`
- breaking-policy hint: `yes` — same shape as `serve`: call sites
  `pkg.Convert(s)` break when the identifier becomes a type.
- kind strings relied on: `"function"`, `"type"`

### 4. `control` — `same_kind (control)`
- NAME held constant: `Keep() int {`
- old_kind -> new_kind: `function` -> `function` (only the body `return 1` -> `return 2`)
- breaking-policy hint: `no` — kind and signature are unchanged; this is a body-only
  change and must NOT emit a `modified` symbol (guards against over-firing).
- kind strings relied on: `"function"`

## Kind strings used (copied from `go.rs`)
`"function"`, `"type"` (and, not reachable for same-name transitions,
`"generic_function"`, `"generic_type"`).

## UNCERTAINTY
- I could NOT run the parser (cargo/formatter prohibited in this shared repo), so the
  same-name/different-kind behavior is inferred from source, not observed.
- Core caveat: because `name` = the full post-keyword text, keeping `name`
  byte-identical while changing `kind` REQUIRES swapping the `func `/`type ` prefix on
  otherwise identical text. That makes one side of each kind-change pair
  NON-COMPILABLE Go (`type Serve() {`, `func Record struct {`,
  `type Convert(s string) (int, error) { return 0, nil }`). The `_before` side of every
  pair is valid Go; the `_after` side is the minimal prefix swap the parser requires.
  There is no valid-Go input that yields identical `rest` under both `func ` and
  `type `, so "syntactically valid on both sides" and "same-name/different-kind" are
  mutually exclusive for THIS adapter; parser-correctness was prioritized.
- `generic_function`/`generic_type` cannot participate in a same-name transition: their
  `name` embeds `[...]` / type-parameter text that plain kinds never produce, so the
  names never coincide. All three kind-change pairs therefore necessarily reuse the
  single achievable transition `function` <-> `type`, varied only by direction
  (pair 2 is reversed) and signature shape. This is the maximum variety the adapter
  supports for the `modified` signal.
