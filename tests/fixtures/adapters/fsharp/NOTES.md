# F# adapter fixture notes

Source of truth: `src/diff/lang/adapters/fsharp.rs` (plus `common.rs`).
All expectations below are derived strictly from the current source, not from
ideal F# semantics. Where the source is wrong/limited, it is documented as a
known miss rather than "corrected" in the expectation.

## Files matched (`detect_files`)

Extensions (case-sensitive, via `Path::extension`): `fs`, `fsx`, `fsi`.
Everything else is ignored (no `fsproj`, no `ml`/`mli`).

## Public-symbol rules (`parse_ast` -> `parse_fsharp_line` -> `parse_decl`)

Per line, after `trim_start()`:
- Skip if empty, or line starts with `//`, `(*`, `#`, or `open `.
- `strip_leading_attributes` removes any number of leading `[< ... >]` blocks
  (`find_attr_end` handles nested `[< [< ... >] >]`).
- Then the first matching prefix wins; `kind` is assigned:

| Prefix   | kind     | Notes                                                        |
|----------|----------|--------------------------------------------------------------|
| `module `| `module` |                                                              |
| `type `  | `type`   |                                                              |
| `let `   | `value`  |                                                              |
| `val `   | `value`  | signature files (`.fsi`)                                     |

Inside `parse_decl`, after the prefix:
- attributes stripped again;
- if the remainder starts with `private ` or `internal ` -> dropped (non-public);
- modifiers `rec `, `inline `, `mutable `, `global ` are stripped (loop);
- `take_identifier` reads the name until whitespace or any of `()<>:=[]{}`.
  Empty result -> dropped.

Visibility is therefore a *prefix-after-keyword* check only. There is no real
scope/indentation model: an indented `let`/`type` inside a module or class body
is parsed the same as a top-level one (see known misses).

### Name capture specifics (`take_identifier`)
- Generics: `type Box<'a>` / `let id<'a>` -> name `Box` / `id` (stops at `<`).
- Constructors: `type Counter(start)` -> name `Counter` (stops at `(`).
- `val add : int -> int` -> name `add` (stops at the space before `:`).
- Operators `let (++) ...` and active patterns `let (|Even|Odd|) ...` start with
  `(` -> empty name -> **not detected**.

## Breaking definition (`detect_breaking_changes` + `basic_diff`)

`basic_diff` compares symbol **names only** (a `HashSet<&name>`); `kind` and
signatures are ignored. `modified` is always empty. `detect_breaking_changes`
returns `!diff.removed.is_empty()`.

Consequences:
- Any removed public **name** => breaking = true (includes renames, which show
  up as one removal + one addition).
- A signature/body change that keeps the same name => NOT breaking (invisible).
- Reordering, additions, or `kind` changes at the same name => NOT breaking.

## Known misses / questionable behavior (report-only; not fixed here)

1. **Multi-line block comments are not suppressed.** Only the line that starts
   with `(*` is skipped. A declaration on its own line inside `(* ... *)` is
   parsed and reported as public. Demonstrated by `edge/block_comment.fs`.
2. **No scope model.** Nested/member `let` (and `type`) bindings are flagged as
   public API. e.g. `let mutable n = start` inside a class body would emit a
   value `n`. (The `positive/types.fs` class deliberately avoids this to stay
   clean.)
3. **Custom operators and active patterns are invisible** (`take_identifier`
   breaks on `(`). Demonstrated by `edge/operators.fs`.
4. **`namespace` is not handled** — `namespace Foo` lines are ignored (no
   prefix match). `module` still is.
5. **Signature-only changes are non-breaking.** Because diffing is name-only,
   changing `val f : int` to `val f : string` (same name) is not breaking and
   not even "modified".
6. **`type T private = ...`** (private constructor, public type) is reported as
   a public `type` — the `private` here is not at the start of the remainder,
   so it is not treated as a visibility modifier. Arguably correct.
7. **Conditional-compilation bodies are not suppressed.** Only the `#if`/`#endif`
   lines are skipped; a `let` inside `#if DEBUG ... #endif` would be reported.
   (`negative/directives.fs` keeps only directive lines to remain a true 0.)

## Per-file expectations

### positive/ (each: >= 1 public symbol)
- `positive/modules.fs` -> module `Geometry`; type `Shape`; value `area`.
  (`let private helper` is dropped.) => 3 symbols.
- `positive/types.fs` -> module `Types`; types `Person`, `Result`, `UserId`,
  `Counter` (base names; members not flagged). => 5 symbols.
- `positive/values.fs` -> module `Values`; values `answer`, `factorial`,
  `square`, `counter`, `Pi` (modifiers `rec`/`inline`/`mutable` and the
  `[<Literal>]` attribute stripped). => 6 symbols.
- `positive/script.fsx` -> module `Signatures`; value `greeting`; module
  `Inner`; value `double` (indented member still parsed); type `Config`.
  `open` ignored. => 5 symbols. (Also exercises `.fsx` detection.)
- `positive/signatures.fsi` -> module `Api`; values `add`, `isValid` (from
  `val`); type `Handler`. `namespace Sample` ignored. => 4 symbols.
  (Exercises `.fsi` detection and `val`->`value` mapping.)
- `positive/attributes.fs` -> module `Attributed`; types `Legacy`, `Point`;
  values `compute`, `Version`. All leading `[< ... >]` attributes stripped
  (incl. `[<Struct; CompiledName("Point")>]`); `inline` stripped. => 5 symbols.

### negative/ (each: exactly 0 public symbols)
- `negative/private.fs` -> 0 (every decl is `private`/`internal` after the
  keyword; no public module wrapper).
- `negative/comments.fs` -> 0 (only `//` lines, single-line `(* ... *)`, and
  `open`; no keyword line inside a block comment).
- `negative/strings.fs` -> 0 (code-looking text is inside `printfn`/strings;
  no line begins with a declaration keyword).
- `negative/directives.fs` -> 0 (every content line starts with `#`).

### breaking/ (each pair: breaking = true)
- `breaking/remove_type` -> after removes public type `Removed`;
  removed = {`Removed`} => breaking = true.
- `breaking/remove_value` -> after removes public value `sub`;
  removed = {`sub`} => breaking = true.
- `breaking/remove_signature` (`.fsi`) -> after removes `val parse`;
  removed = {`parse`} => breaking = true.

### edge/
- `edge/generics.fs` -> module `Generics`; type `Box`; values `identity`,
  `map`. Expect base names without `<...>` (generics do not break detection).
  => 4 symbols.
- `edge/block_comment.fs` -> module `Commented`; values `ghost` AND `live`.
  `ghost` is a known FALSE POSITIVE: it sits on its own line inside a
  `(* ... *)` block but is still reported (miss #1). => 3 symbols.
- `edge/operators.fs` -> module `Ops`; value `normal` ONLY. Expect NO symbol
  named `(++)`, `Even`, or `Odd` — custom operator and active pattern are not
  detected (miss #3). => 2 symbols.
