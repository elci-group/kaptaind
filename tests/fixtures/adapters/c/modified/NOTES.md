# C adapter — `modified` (kind-change) fixture expectations (source-derived)

Adapter: `src/diff/lang/adapters/c.rs` (`CAdapter`, `c_parse`).
Extension used: `.h` (adapter `detect_files` matches exactly `.c` and `.h`).
Each file emits exactly ONE symbol so the same-name/different-kind signal is
unambiguous. Expectations describe what the adapter emits TODAY; the parser was
NOT run here.

Kind strings relied on (copied verbatim from `c.rs`):
- `"macro"`    (`c.rs:59`)  — line starts with `#define `; name = first token after.
- `"struct"`   (`c.rs:72`)  — line starts with `struct `; name = first token after,
  trailing `{ ; *` trimmed.
- `"enum"`     (`c.rs:86`)  — line starts with `enum `; name = first token after,
  trailing `{ ; *` trimmed.
- `"function"` (`c.rs:109`) — first `(` on the line; name = token immediately
  before `(`, with a valid-identifier "return type" token before that, and name
  not a control keyword.

Name held byte-identical across every before/after pair; only the kind-bearing
keyword/shape is swapped.

## Pairs

1. struct_to_function  — NAME `widget` — `struct -> function`
   - policy hint: `yes` — removes a `struct widget` type consumers instantiate
     (`struct widget w;`, `sizeof(struct widget)`); even though C keeps tags and
     ordinary identifiers in separate namespaces, the type itself is gone.
   - kinds: `"struct"` -> `"function"`.

2. enum_to_macro  — NAME `mode` — `enum -> macro`
   - policy hint: `yes` — removes the `enum mode` tag and replaces it with an
     object-like macro that textually substitutes every `mode` token, risking
     silent semantic changes beyond the lost type.
   - kinds: `"enum"` -> `"macro"`.

3. function_to_enum  — NAME `state` — `function -> enum`
   - policy hint: `yes` — removes a callable `state()` consumers link/call;
     replacing it with an enum tag breaks all call sites.
   - kinds: `"function"` -> `"enum"`.

4. control  — NAME `config` — `same_kind (control)`
   - policy hint: `no` — same name AND same kind (`"struct" -> "struct"`); only
     a member was added, so no kind change occurred and the `modified` signal
     must NOT fire (guards over-firing). (Member addition may be ABI-, not
     source-, relevant, but it is not a kind change.)
   - kinds: `"struct"` -> `"struct"`.

## Uncertainty (parser not run)

- High confidence the four single-line declaration forms emit the stated kinds:
  they mirror the proven `positive/{structs,enums,functions,macros}.h` fixtures.
- Body/member lines (`int id;`, `OFF,`, `return 0;`, `int timeout;`) emit NO
  extra symbol: none has `(` and none starts with a recognized prefix. I avoided
  the `struct node *next;` self-reference pattern that double-emits in
  `positive/structs.h`, so each file yields exactly one symbol.
- `return 0;` is safe from the `return name(...)` false-positive (NOTES gap #7):
  it has no `(`.
- The shared `modified` diff signal is the target of these fixtures. Note that
  TODAY's `basic_diff` compares by NAME only and never populates `modified`
  (`NOTES.md` gap #10); these pairs still satisfy the parse-layer premise of
  same-name/different-kind for whenever the kind-aware signal is wired in.
- `enum_to_macro` uses a lowercase macro name (`mode`); unconventional but
  syntactically valid and required to keep the NAME byte-identical to the enum.
