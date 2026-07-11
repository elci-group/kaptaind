# Perl adapter — `modified` kind-change fixture corpus

Each `*_before.pm` / `*_after.pm` pair holds the extracted symbol **NAME** byte-identical while changing the declaration so the adapter emits a different `kind`. The adapter (`src/diff/lang/adapters/perl.rs`) only emits three kinds and takes the symbol name from the first token after the kind keyword:

- `package <name>`  -> name = first whitespace token, trailing `;` stripped
- `sub <name>`      -> name = first token split on whitespace/`(`/`{`/`;`/`:`
- `use constant <name>` -> name = first whitespace token, trailing `(`/`{`/`;`/`,` stripped

A bare identifier (`Answer`, `Widget`, `Config`, `helper`) survives all three extraction rules identically, so the same bare name is reused across the kind keyword swap. Each pair file contains exactly ONE parsed symbol so there is no duplicate-name aliasing to confound the diff. Extension `.pm` is one of the two the adapter detects (`pl`, `pm`).

## Pairs

### 1. `sub_to_constant`
- NAME held constant: `Answer`
- kind transition: `sub -> constant`
- breaking-policy hint: `depends` — in Perl `use constant Answer => 42` compiles to a 0-arg inlined sub `Answer()`, so plain callers (`Answer()`) keep working, but the prototype/inlining differs from a real `sub Answer` and `&Answer`/redefinition behavior can change.
- exact kind strings relied on: `"sub"`, `"constant"`
- uncertainty: low — name extraction yields `Answer` on both sides; only risk is `basic_diff` not keying strictly by name (task states it does).

### 2. `package_to_sub`
- NAME held constant: `Widget`
- kind transition: `package -> sub`
- breaking-policy hint: `yes` — a namespace (`package Widget`, used via `use Widget` / `Widget->new`) becoming a callable `sub Widget` is a category change; module-load and method-call consumers break.
- exact kind strings relied on: `"package"`, `"sub"`
- uncertainty: low — single symbol per file, name `Widget` identical; same `basic_diff` caveat as above.

### 3. `constant_to_package`
- NAME held constant: `Config`
- kind transition: `constant -> package`
- breaking-policy hint: `yes` — a callable scalar constant `Config()` turning into a non-callable namespace `package Config` breaks anyone reading it as a value.
- exact kind strings relied on: `"constant"`, `"package"`
- uncertainty: low — name `Config` identical on both sides; same `basic_diff` caveat.

### 4. `control`
- NAME held constant: `helper`
- kind transition: `same_kind (control)` — `sub -> sub`, only the returned string body changes.
- breaking-policy hint: `no` — not a kind change; body-only edit must NOT produce a `modified` symbol (guards against over-firing).
- exact kind strings relied on: `"sub"`, `"sub"`
- uncertainty: low — identical name and identical kind keyword; body text is not part of the parsed name/kind.

## General uncertainty
I could not execute the parser or diff engine (no cargo/parser runs permitted in this lane), so the emission of same-name/different-kind is reasoned from the adapter source rather than observed. The intended signal matches the documented `modified` definition (same NAME, different KIND). If `basic_diff` were ever changed to match on `(name, kind)` tuples instead of name alone, these pairs would surface as remove+add rather than `modified`; under the current name-keyed definition they are correct.
