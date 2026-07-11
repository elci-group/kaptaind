# Rust adapter — `modified` (kind-change) fixture notes

Adapter source: `src/diff/lang/adapters/rust.rs`.
Detection: `detect_files` matches extension `.rs` only (line 204: `e == "rs"`).
Name extraction (the token held byte-identical across each pair):
- functions: `format_fn_sig` (`rust.rs:23`) → for a no-arg fn, `"<ident>()"`; the ident is `sig.ident`.
- struct / enum / trait: `node.ident.to_string()` → the bare item ident.

`modified` signal under test: SAME `name`, DIFFERENT `kind`.

Kind strings relied on (copied verbatim from the adapter):
- `"function"` (`rust.rs:54`)
- `"async_function"` (`rust.rs:52`)
- `"struct"` (`rust.rs:69`)
- `"enum"` (`rust.rs:93`)
- `"trait"` (`rust.rs:110`)

## Pairs

### 1. `fn_async` — `fn_async_before.rs` / `fn_async_after.rs`
- NAME held constant: `compute()` (no args → `"compute()"`; body `42` unchanged).
- `old_kind -> new_kind`: `function` -> `async_function` (only `async` keyword added).
- Breaking-policy hint: **yes** — an `async fn` returns `impl Future<Output = u32>` instead of `u32`, so callers must now `.await`; signature/category change breaks direct use.
- Kind strings used: `"function"`, `"async_function"`.
- Uncertainty: low. `format_fn_sig` ignores `asyncness`, so the name stays `compute()` and only the kind flips via `node.sig.asyncness.is_some()`. The residual risk is syn treating `pub async fn` identically at the `sig.ident` level — which it does.

### 2. `struct_to_enum` — `struct_to_enum_before.rs` / `struct_to_enum_after.rs`
- NAME held constant: `Config` (bare ident).
- `old_kind -> new_kind`: `struct` -> `enum`.
- Breaking-policy hint: **yes** — value construction/matching changes form (`Config` unit value vs `Config::Variant`); existing literals and pattern matches no longer compile.
- Kind strings used: `"struct"`, `"enum"`.
- Uncertainty: low. Before is a unit struct (`Fields::Unit`), so the `Fields::Named` branch is skipped and no `field` symbols are emitted; after is an empty enum, so no `variant` symbols are emitted. Each side emits exactly one symbol (`Config`) with the differing kind, so the diff should be a single clean `modified`. Minor residual risk: an empty enum is unusual but valid Rust and is parseable by syn.

### 3. `trait_to_struct` — `trait_to_struct_before.rs` / `trait_to_struct_after.rs`
- NAME held constant: `Handler` (bare ident).
- `old_kind -> new_kind`: `trait` -> `struct`.
- Breaking-policy hint: **yes** — item category changes entirely; `impl Handler for T` and `T: Handler` bounds no longer apply to a struct, so downstream impls/generics break.
- Kind strings used: `"trait"`, `"struct"`.
- Uncertainty: low. Empty trait has no items, so no `trait_method`/`associated_type` symbols; unit struct emits no `field` symbols. Each side is a single `Handler` symbol with the differing kind → one clean `modified`.

### 4. `control` — `control_before.rs` / `control_after.rs`
- NAME held constant: `answer()` (no args → `"answer()"`).
- `old_kind -> new_kind`: `same_kind (control)` — `function` -> `function`; only the body literal changes (`41` -> `42`).
- Breaking-policy hint: **no** (control) — body-only change; name, signature, and kind are identical, so this must NOT produce a `modified` symbol (guards against over-firing).
- Kind strings used: `"function"`.
- Uncertainty: low. Signature is byte-identical across the pair, so `format_fn_sig` yields the same name and kind; the body literal is not part of the symbol. If this ever fires as `modified`, the engine is over-firing.
