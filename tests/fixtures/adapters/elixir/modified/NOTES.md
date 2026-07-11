# Elixir `modified` (kind-change) fixture corpus

Diff signal under test: a symbol is `modified` when its extracted **name** is
byte-identical across before/after but its **kind** changes.

Adapter facts (from `src/diff/lang/adapters/elixir.rs`):
- Name = the whole trimmed remainder of the line after the matched keyword
  (`rest.to_string()`). Keeping the token(s) after the keyword byte-identical
  keeps the name identical.
- Keyword -> `kind` (checked in this order, first match wins):
  - `defmodule `  -> `"module"`
  - `defprotocol` -> `"protocol"`
  - `defmacro `   -> `"macro"`
  - `def `        -> `"function"`
- Extensions detected: `.ex`, `.exs` (these fixtures use `.ex`).

## Pairs

1. `func_to_macro`
   - NAME held constant: `transform(expr) do`
   - kind: `function` -> `macro`
   - breaking-policy hint: **yes** — a `def` is called at runtime with a value;
     a `defmacro` receives AST and runs at compile time, so existing call sites
     that pass runtime values break / change semantics.
   - kind strings relied on: `"function"`, `"macro"`.
   - uncertainty: low. `def transform(expr) do` vs `defmacro transform(expr) do`
     keep `transform(expr) do` byte-identical and the adapter checks `defmacro`
     before `def`, so the after-line cannot be mis-read as a function. The
     wrapping `defmodule MyApp.Transforms do` is identical both sides (not
     flagged), so the only modified symbol is the target.

2. `module_to_protocol`
   - NAME held constant: `Countable do`
   - kind: `module` -> `protocol`
   - breaking-policy hint: **yes** — a module and a protocol are different
     beasts; callers that `import`/`alias` the module or call its functions
     break when it becomes a protocol dispatch contract.
   - kind strings relied on: `"module"`, `"protocol"`.
   - uncertainty: low-medium on the target (`Countable do` is identical), but
     this pair carries INCIDENTAL noise: the after-file adds a protocol
     function head `def count(data)` (kind `"function"`) because an empty
     `defprotocol` is invalid Elixir, while the before module is empty. Expect
     one extra `added` symbol (`count(data)`) alongside the intended modified
     `Countable do`.

3. `protocol_to_module`
   - NAME held constant: `Enumerable do`
   - kind: `protocol` -> `module`
   - breaking-policy hint: **yes** — reverse of (2); protocol dispatch
     implementations (`defimpl`) and callers relying on dispatch break when the
     name becomes a plain module.
   - kind strings relied on: `"protocol"`, `"module"`.
   - uncertainty: low-medium on the target (`Enumerable do` is identical), with
     INCIDENTAL noise: the before protocol declares `def reduce(collection, acc, fun)`
     (kind `"function"`) that the empty after-module drops, so expect one extra
     `removed` symbol alongside the intended modified `Enumerable do`.

4. `control`
   - NAME held constant: `greet(name) do`
   - kind: `function` -> `function` (same_kind — control)
   - breaking-policy hint: **no** — only the body string and a blank line
     changed; same name and same kind, so it must NOT fire a modified symbol
     (guards against over-firing).
   - kind strings relied on: `"function"` (unchanged).
   - uncertainty: low. The `def greet(name) do` line is byte-identical both
     sides; only the body/whitespace differs, so name and kind are unchanged.

## Cross-pair note (honest limitation)

The adapter distinguishes four kinds (`module`, `protocol`, `macro`, `function`),
but a byte-identical NAME must also be syntactically valid on BOTH sides of the
kind change. Elixir name rules split the four kinds into exactly two families
that share a valid identical token:
- Alias (Capitalized): valid for `module` <-> `protocol` (pairs 2, 3).
- lowercase-with-args: valid for `function` <-> `macro` (pair 1).

There is no Elixir identifier that is simultaneously a valid function/macro name
(lowercase) and a valid module/protocol name (Alias), so cross-family
transitions (e.g. `function` -> `protocol`) cannot hold the name byte-identical
while staying valid code. The three kind-change pairs therefore cover the only
two viable families, using both directions of the Alias family (pairs 2 and 3)
to vary from/to. I could not run the parser to confirm; reasoning is from the
adapter source only.
