# PHP `modified` fixture notes

Signal under test: a symbol is `modified` when its NAME is unchanged but its KIND
changes across a before/after pair (same name, different `kind`).

Adapter facts relied on (`src/diff/lang/adapters/php.rs`):
- `detect_files` matches extension `.php` only.
- For the type-declaration family (`class ` / `interface ` / `trait ` / `enum `),
  the name is `rest.split_whitespace().next()` — the first token after the
  keyword — and the `kind` is the literal prefix. Name extraction is identical
  across all four, so swapping ONLY the keyword guarantees a same-name /
  different-kind emission.
- A `public function …` member is emitted as `kind = "method"`; its name is taken
  from before the `(`, so a bodyless (`;`) vs bodied (`{}`) signature with the
  same prefix yields an identical symbol and cancels out of the diff.
- Lines `case …;`, `<?php`, blank lines and `//`/`#` comments emit no symbol.

Kind strings used (copied verbatim): `"class"`, `"interface"`, `"trait"`,
`"enum"`, `"method"`.

## Pairs

### repo — class → interface
- NAME held constant: `UserRepository`
- `old_kind -> new_kind`: `class` -> `interface`
- Supporting member `find` keeps an identical `public function find(int $id)`
  prefix in both files (class body `{}` vs interface bodyless `;`), so it stays
  `method`/`find` and cancels; only the declaration keyword changes.
- BREAKING-POLICY HINT: `yes` — an interface cannot be instantiated with `new`
  and forces `implements`; existing `new UserRepository()` consumers break.
- kind strings relied on: `"class"`, `"interface"`, `"method"`.
- UNCERTAINTY: low. Same-name/different-kind is certain for the declaration;
  assumes `basic_diff` keys symbols on (name, kind), which the task statement
  defines. The class method body returns `$this` with no declared return type,
  which is valid PHP.

### contract — interface → trait
- NAME held constant: `Cacheable`
- `old_kind -> new_kind`: `interface` -> `trait`
- Member `key` keeps an identical `public function key(): string` prefix in both
  (interface bodyless `;` vs trait body `{}`), so it cancels as `method`/`key`.
- BREAKING-POLICY HINT: `depends` — consumers using `implements Cacheable` or
  type-hinting the contract break, while `instanceof`/usage semantics shift; code
  that only called the method may keep working.
- kind strings relied on: `"interface"`, `"trait"`, `"method"`.
- UNCERTAINTY: low; same (name, kind) reasoning as `repo`.

### status_enum — trait → enum
- NAME held constant: `Status`
- `old_kind -> new_kind`: `trait` -> `enum`
- `before` is an empty trait (emits only `{Status, trait}`); `after` is an enum
  whose `case …;` lines emit no symbols (emits only `{Status, enum}`), so the
  declaration is the sole differing symbol.
- BREAKING-POLICY HINT: `yes` — an enum cannot be `use`d as a trait and is
  referenced via `Status::Active` cases rather than mixed into a class.
- kind strings relied on: `"trait"`, `"enum"`.
- UNCERTAINTY: low-to-medium. The keyword swap is certain; the only assumption
  is that `case Active;` / `case Inactive;` lines match no adapter prefix (they
  do not start with `namespace`/`function`/`public`/`class`/`interface`/`trait`/
  `enum`), so they stay symbol-free. Cannot confirm by running the parser.

### control — same_kind (control)
- NAME held constant: `User` (member `save` also unchanged)
- `same_kind (control)`: `class` stays `class`; only the method body
  (`return true;` -> `return false;`) and an added blank line change.
- Expected: NO modified symbol (identical (name, kind) set: `{User, class}`,
  `{save, method}`). Guards against over-firing on body/whitespace edits.
- BREAKING-POLICY HINT: `no` — declaration kind and name are unchanged; this is
  an in-place behavior tweak, not an API-surface kind change.
- kind strings relied on: `"class"`, `"method"`.
- UNCERTAINTY: low. Relies on `basic_diff` treating identical (name, kind)
  symbols as unchanged; body text past the `(` is never inspected for the name.
