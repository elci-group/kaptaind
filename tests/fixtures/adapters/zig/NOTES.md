# Zig adapter calibration corpus (adapter-200 item 10, rev 37)

Semantics: Zig's visibility model is explicit — a declaration is public
exactly when it carries `pub` (or `export`, which implies a public C-ABI
entry point) — so the adapter sits in the explicit-visibility 0.8 band.
Surface: `pub fn` (incl. `export fn`, `pub export fn`, `pub extern "..."`
fn), `pub const` containers (`struct` incl. `packed`/`extern`, `enum`,
`union`/`union(enum)`, `opaque`), other `pub const` values, `pub var`, and
struct-body fields (`name: Type` — Zig has no field-level privacy, so every
field of an accessible container is reachable). Enum/union members are NOT
emitted (type identity only, NS_ENUM precedent). Methods are container-level
`pub fn`s emitted flat under their own name (groovy precedent — cross-type
name collisions merge, a known T2 limitation). Function signatures are
canonical parameter-type tuples: `name: Type` pairs reduce to their type, so
a parameter rename leaves the signature untouched while a type change alters
it (`comptime` prefixes dropped, variadic `...` skipped, whitespace
normalized, commas inside `fn (i32, u8) void` pointer types kept).
Multi-line headers accumulate to the `;`/`{` terminator at paren depth 0.
Born-correct comment handling: Zig has ONLY `//` line comments (`///` doc,
`//!` module — no block comments); the stripper is string-aware so a `//`
inside a URL default is kept, and `\\`-prefixed multi-line string literal
lines are never parsed. Exclusions: non-`pub` declarations, plain `extern
fn` imports, `usingnamespace` re-exports, `test`/`comptime` blocks, and
fields written on the container's opening line (single-line bodies — the
type symbol is still emitted).

- positive/: pub functions in all forms (plain, export, pub extern,
  multi-line error-union), typed const containers with fields, struct
  methods with a non-pub member excluded, and signature shapes (slices,
  `comptime`, fn-pointer types, optionals) → all must yield symbols.
- negative/: call sites, assignments, non-pub declarations, `test` blocks,
  and fake declarations in `//`/`///`/`//!` comments and `\\` multi-line
  string literals → zero symbols.
- breaking/: `remove_fn`/`remove_field` pairs delete surface members →
  `diff.removed` non-empty → breaking fires. `control` removes a non-pub
  function — surface unchanged → NOT breaking.
- modified/: same-name declaration changes kind (function→variable,
  variable→const, struct→enum) → X2 `modified` fires. `control` changes
  only a function body → symbols and signatures unchanged → not modified
  (by design).
- signature/: `change_param_type` alters a parameter type and `add_param`
  adds one → canonical tuple changes → `modified` fires via signature.
  `rename_param` renames a parameter → tuple unchanged → NOT modified
  (control) — types, not names, are the contract (callers in Zig bind
  positionally).
