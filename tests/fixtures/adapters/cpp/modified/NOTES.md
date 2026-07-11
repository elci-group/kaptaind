# C++ adapter — `modified` (kind-change) fixture notes

The diff engine flags a symbol as `modified` when the **name is unchanged but the
`kind` changes** (same name, different `kind`). Each pair below holds the
extracted symbol NAME byte-identical and swaps only the kind-bearing keyword.
Every file emits exactly one symbol so the symbol set is `[{Name, oldKind}]`
before and `[{Name, newKind}]` after.

Kind strings relied on, copied verbatim from `src/diff/lang/adapters/cpp.rs`
(lines 42/47/52/57): `"class"`, `"struct"`, `"namespace"`, `"function"`.

Name extraction (for reference): `extract_class`/`extract_struct` take the token
after the `class ` / `struct ` prefix up to whitespace/`{`/`:`/`;`/`<`;
`extract_namespace` takes the token after `namespace ` up to whitespace/`{`/`;`;
`extract_function_definition` takes the last whitespace token before `(`, with
trailing `*`/`&` trimmed. The parse loop is an ordered `if/else if`
(class > struct > namespace > function), so one kind-bearing line = one symbol.

---

## Pair 1 — `access_default`
- Files: `access_default_before.hpp` / `access_default_after.hpp`
- NAME held constant: `Widget`
- Kind change: `"class"` -> `"struct"`
- Breaking-policy hint: **yes** — `class` -> `struct` flips default member
  access (private -> public) and default base-class inheritance access
  (private -> public), which changes the observable API/ABI for consumers and
  derived types.
- Kind strings used: `"class"`, `"struct"`.
- Uncertainty: Low. `class Widget {` and `struct Widget {` each extract the name
  `Widget` from the token after the prefix, stopping at `{`. Only the keyword
  differs, so same name / different kind is expected. Member lines end in `;`
  and are not emitted as symbols.

## Pair 2 — `promote_to_class`
- Files: `promote_to_class_before.hpp` / `promote_to_class_after.hpp`
- NAME held constant: `Engine`
- Kind change: `"function"` -> `"class"`
- Breaking-policy hint: **yes** — call sites `Engine()` that previously invoked a
  free function now name a type constructor (different semantics; requires the
  type to be constructible), and `Engine e;`/`new Engine` change meaning. Any
  consumer calling the free function breaks.
- Kind strings used: `"function"`, `"class"`.
- Uncertainty: Medium. `inline void Engine() {` — the function extractor takes
  the last whitespace token before `(`, which is `Engine`; `inline` and `void`
  are earlier tokens and are not the last token, and `inline ` is not a skip
  prefix, so the name should be `Engine` with kind `"function"`. The PascalCase
  free function is unconventional style but syntactically valid C++; the
  extraction rule is what matters. After side `class Engine {` -> `Engine` /
  `"class"` is straightforward.

## Pair 3 — `namespace_to_struct`
- Files: `namespace_to_struct_before.hpp` / `namespace_to_struct_after.hpp`
- NAME held constant: `net`
- Kind change: `"namespace"` -> `"struct"`
- Breaking-policy hint: **yes** — `net::connect()` qualification and
  `using namespace net;` stop compiling once `net` is a struct type instead of a
  namespace; consumers qualified through the namespace break.
- Kind strings used: `"namespace"`, `"struct"`.
- Uncertainty: Low-Medium. `namespace net {` extracts `net` (token after the
  prefix, stops at `{`); the inner `int connect();` is a declaration ending in
  `;`, which `extract_function_definition` rejects, so only the `net` symbol is
  emitted. `struct net {` -> `net` / `"struct"`. Lowercase `struct net` is
  unconventional but valid; the keyword swap drives the kind change.

## Pair 4 — `control` (CONTROL — must NOT fire)
- Files: `control_before.hpp` / `control_after.hpp`
- NAME held constant: `Control`
- Kind change: `same_kind (control)` — `"class"` -> `"class"` (unchanged)
- Breaking-policy hint: **no** — only a member declaration name changed
  (`value` -> `count`); the type name and kind are identical, so the public type
  is unchanged for consumers (member declarations are not emitted symbols).
- Kind strings used: `"class"`.
- Uncertainty: Low. The `class Control {` line is byte-identical across the pair,
  so name `Control` / kind `"class"` is identical. The changed member
  (`int value() const;` -> `int count() const;`) ends in `;`, which
  `extract_function_definition` rejects, so it emits no symbol in either file —
  the symbol sets are identical and the engine must NOT report a modified
  symbol. This guards against over-firing on non-kind body edits.

---

## General uncertainty (applies to all pairs)
- I could not run the parser; all behavior is inferred from
  `src/diff/lang/adapters/cpp.rs` and the task's stated `modified` rule
  (same name, different `kind`). I did not inspect `basic_diff` (out of scope
  for this task), so I rely on the documented same-name/different-kind
  definition of `modified` rather than the exact diff implementation.
- All files use the `.hpp` extension, which is in `detect_files`
  (`cpp`/`cc`/`cxx`/`hpp`/`h`).
