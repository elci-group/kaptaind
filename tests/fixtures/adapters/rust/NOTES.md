# Rust adapter fixture expectations

Source of truth: `src/diff/lang/adapters/rust.rs` (syn `Visit` over `syn::File`).
All expectations below are derived from that source as-is, not from semver ideology.

## Extensions matched by `detect_files`
- Only `rs` (`.rs`). Matched via `p.extension() == "rs"`. Nothing else (no `.rlib`,
  no shebang, no `build.rs`-by-name special case — `build.rs` is matched solely by
  its `.rs` extension).

## Public-symbol rules (`parse_ast` → `kind`)
Visibility gate: `is_pub` matches ONLY `syn::Visibility::Public(_)`. Therefore
`pub(crate)`, `pub(super)`, `pub(in …)` and private items are NOT emitted.
Inline `mod { … }` contents ARE recursed into by the default visitor, so a `pub`
item nested in an inline module is still emitted.

| kind             | emitted for                                                    | `name` format                                   |
|------------------|----------------------------------------------------------------|-------------------------------------------------|
| `function`       | `pub fn` (non-async)                                           | `name(arg1, arg2)` — args are the PATTERN idents, `self` for any receiver, `_` for non-ident pats; `name()` if none |
| `async_function` | `pub async fn`                                                 | same as `function`                              |
| `struct`         | `pub struct` (named/tuple/unit)                                | `Name`                                          |
| `field`          | `pub` fields of a `pub` NAMED-field struct only                | `Struct.field`                                  |
| `enum`           | `pub enum`                                                     | `Name`                                          |
| `variant`        | EVERY variant of a `pub` enum                                  | `Enum::Variant`                                 |
| `trait`          | `pub trait`                                                    | `Name`                                          |
| `trait_method`   | every `fn` item inside a `pub trait`                           | `Trait::method` (ident ONLY — no args/receiver) |
| `associated_type`| every `type` item inside a `pub trait`                         | `Trait::Type`                                   |
| `method`         | `pub` fns in an inherent `impl`; ALL fns in a `impl Trait for` | `Type::sig(args)` inherent; `Type<Trait>::sig(args)` for trait impls |
| `type_alias`     | `pub type`                                                     | `Name`                                          |
| `const`          | `pub const`                                                    | `NAME`                                          |
| `static`         | `pub static`                                                   | `NAME`                                          |

`impl` self-type must be a `syn::Type::Path`; if `type_name` cannot be resolved
(e.g. `impl Trait for &T`, `impl Trait for [T]`, `impl Trait for dyn …`), the whole
impl is skipped and emits ZERO method symbols.

## Deliberately ignored (not emitted)
- Non-`Public` visibility: private, `pub(crate)`, `pub(super)`, `pub(in …)`.
- Comments and string/byte-string contents — syn only yields real items, so
  `pub fn` text inside `//`, `/* … */`, `"…"`, `r"…"` is never an item.
- `use` / `pub use` (no `visit_item_use`); `macro_rules!` / `pub macro`
  (no `visit_item_macro`); `extern crate`; attributes are kept but never evaluated.

## Known misses / gaps (source behavior)
1. Associated consts in traits (`TraitItem::Const`) and impls (`ImplItem::Const`)
   are NOT emitted. Associated types in impls (`ImplItem::Type`) likewise NOT emitted.
2. Tuple/unit struct fields: only `Fields::Named` produce `field` symbols; tuple
   structs emit a `struct` symbol but zero `field` symbols even for `pub` fields.
3. `impl` for a non-path self type (`&T`, `[T]`, etc.) emits nothing.
4. `#[cfg(…)]` is NOT evaluated: a cfg-gated `pub` item is emitted even when its
   feature/condition is false.
5. `pub(crate)`/`pub(super)` are treated as private (not flagged).
6. No generated-file awareness: a `*.rs` produced by prost/tonic/etc. is parsed like
   any other file and its `pub` items ARE flagged (detection/down-weighting of
   generated code is not this adapter's responsibility).

## Breaking definition (`detect_breaking_changes`)
REMOVALS ONLY, decided by `basic_diff` which keys symbols purely on `name`
(`modified` is always empty). Returns true if any REMOVED symbol's kind is in:
`function, async_function, method, trait_method, struct, field, enum, variant,
trait, type_alias, associated_type`.
NOT in the set → removal is NOT breaking: `const` and `static` removals are NOT
flagged breaking (gap). Because the diff keys on `name`, a signature change that
alters the formatted name (e.g. adding an arg) appears as old-name removed +
new-name added and IS breaking; a change that keeps the same name (e.g. changing an
arg/return TYPE but not its ident) is NOT detected as removed → NOT breaking.

## Per-file expectations

### positive/
- `functions.rs` → 2 symbols: `greet(name, count)` kind `function`,
  `fetch(url)` kind `async_function`. (`private_helper` excluded.)
- `structs.rs` → kinds {`struct`×3: Config, Point, Marker; `field`×2: Config.host,
  Config.port}. `secret` excluded; tuple/unit structs emit struct but no fields;
  private `Private` excluded.
- `enums.rs` → `enum` Status + `variant`×3 (Status::Active/Inactive/Pending).
  Private enum excluded.
- `traits.rs` → `trait` Handler, `associated_type` Handler::Output,
  `trait_method`×2 (Handler::handle, Handler::name). Private trait excluded.
- `impls.rs` → `struct` Db; `method` Db::connect(url), Db::query(self, sql)
  (private `internal` excluded); `trait` Repo + `trait_method` Repo::find;
  `method` Db<Repo>::find(self, id) (trait-impl fn emitted despite no `pub`).
- `consts_statics_aliases.rs` → `const` VERSION, `static` COUNTER,
  `type_alias` MyResult. Private const/static/type excluded.

### negative/ (each: expect 0 public symbols)
- `private_items.rs` → 0 (everything private).
- `restricted_visibility.rs` → 0 (`pub(crate)`/`pub(super)`/`pub(in …)` not Public).
- `comments_and_strings.rs` → 0 (`pub fn` only inside comments/strings/const text).
- `reexports_and_macros.rs` → 0 (`pub use`, `macro_rules!`, private mod member).

### breaking/ (before→after; expect breaking=true)
- `remove_function`: removes `serve(addr)` kind `function` → breaking=true.
- `remove_field`: removes field `Config.host` kind `field` → breaking=true.
- `remove_trait_method`: removes `Handler::handle` kind `trait_method` → breaking=true.

### edge/ (single files; documents subtle/gap behavior)
- `cfg_gated.rs` → 3 symbols emitted: `gated()` `function`, `net_fetch()`
  `async_function`, `always()` `function`. (cfg NOT evaluated — all emitted.)
- `trait_assoc_const.rs` → emits `trait` Pool, `associated_type` Pool::Conn,
  `trait_method` Pool::acquire; emits NO symbol for `const MAX_CONN` (known miss).
- `impl_reference_self.rs` → emits `struct` Widget, `trait` Draw,
  `trait_method` Draw::draw, `method` Widget::inherent(self) ONLY. The
  `impl Draw for &Widget` block contributes ZERO methods (non-path self type gap).
