# SCSS adapter — `modified` (same name, different kind) fixtures

Adapter: `src/diff/lang/adapters/scss.rs` (`ScssAdapter`, line-based, no real parser).
Detected extensions used here: `.scss` (adapter also detects `.sass`, `.less`).

## Name extraction (how names are derived — drives same-name matching)

- `variable`: `name = <entire trimmed line>` (line must start with `$` and contain `:`,
  or start with `@` + `:` while NOT being `@media/@import/@include/@mixin/@use/@forward`).
- `mixin`: `name = <line with the leading "@mixin " stripped>`.
- `css_var`: `name = <entire trimmed line>` (line starts with `--` and contains `:`).
- `forward`: `name = <entire trimmed line>` (line starts with `@forward `).

Consequence: for `variable` / `css_var` / `forward` the name INCLUDES the kind-discriminating
prefix (`$`, `--`, `@forward `). Two full-line kinds can therefore never share an identical
name (identical text => identical branch => identical kind). The ONLY way to get a
same-name/different-kind signal is to let a `mixin` name (prefix stripped) reproduce the
full textual line of another kind. Every kind-change pair below uses that mechanism.

## Pairs

### 1. var_to_mixin — `variable -> mixin`
- NAME held constant: `$brand-color: #007bff;`
- before: `$brand-color: #007bff;`            -> kind `variable`
- after : `@mixin $brand-color: #007bff;`      -> kind `mixin` (name after stripping `@mixin `)
- breaking-policy hint: **yes** — consumers read it as a value (`color: $brand-color;`); a
  mixin is invoked with `@include`, so existing value references break.
- kind strings relied on: `"variable"`, `"mixin"`.

### 2. mixin_to_cssvar — `mixin -> css_var`
- NAME held constant: `--brand: #000;`
- before: `@mixin --brand: #000;`  -> kind `mixin` (name after stripping `@mixin `)
- after : `--brand: #000;`         -> kind `css_var`
- breaking-policy hint: **yes** — consumption changes from `@include --brand` to `var(--brand)`;
  call sites are not interchangeable.
- kind strings relied on: `"mixin"`, `"css_var"`.

### 3. forward_to_mixin — `forward -> mixin`
- NAME held constant: `@forward 'buttons';`
- before: `@forward 'buttons';`         -> kind `forward`
- after : `@mixin @forward 'buttons';`  -> kind `mixin` (name after stripping `@mixin `)
- breaking-policy hint: **yes** — `@forward` is the Sass module public API re-exported to `@use`
  consumers; swapping it for a mixin drops that module surface.
- kind strings relied on: `"forward"`, `"mixin"`.

### 4. control — `same_kind (control)`
- NAME held constant: `flex-center {`  (mixin name = text after `@mixin ` on the first line)
- before / after: identical `@mixin flex-center {` first line; only the mixin BODY differs
  (added `align-items: center;`). Body lines emit no symbols.
- expected: NO `modified` symbol (same name + same kind `mixin`).
- breaking-policy hint: **no** — same kind, same signature; body-only change.
- kind strings relied on: `"mixin"`.

## Uncertainty (cannot run the parser)

- All three kind-change pairs rely on a mixin whose name reproduces another kind's full line
  (e.g. `@mixin $brand-color: #007bff;`, `@mixin --brand: #000;`, `@mixin @forward 'buttons';`).
  These are NOT syntactically valid SCSS (a mixin name cannot start with `$`, `--`, or `@forward`,
  and a top-level `--brand: #000;` custom property is invalid outside a selector block). The
  adapter is purely line-pattern based and WILL emit the same-name/different-kind symbols as
  traced above, but the files are not valid SCSS. I prioritized the hard requirement
  ("pairs the parser will actually emit as same-name/different-kind") over SCSS validity because,
  given this adapter's name extraction, the two are mutually exclusive.
- I traced each line through the branch order by hand; I did not execute the parser, so there is
  residual risk a branch guard behaves differently than read (e.g. the `@`-variable exclusion list).
- The control pair's safety depends on body lines (`display: flex;`, `align-items: center;`, `}`)
  emitting zero symbols, which they do per the branch rules.
