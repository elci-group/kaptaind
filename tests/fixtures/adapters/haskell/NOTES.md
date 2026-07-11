# Haskell adapter fixture notes

Source of truth: `src/diff/lang/adapters/haskell.rs` (+ `common.rs`).
All expectations below are derived from the adapter AS WRITTEN, including
its known gaps. They are not claims about ideal Haskell semantics.

## Extensions matched (`detect_files`)
- `.hs`, `.lhs` only. Everything else is ignored.

## Public-symbol rules by `kind` (`parse_ast`)
For `.lhs`, lines beginning with `>` have the `>` (and one following space)
stripped first; all other lines are scanned as raw text.

A symbol is emitted only from a NON-empty line.

- `function`: only on a TOP-LEVEL line (column 0; line does not start with
  whitespace). The first whitespace-delimited token must be a valid function
  identifier (starts with ASCII lowercase or `_`; subsequent chars
  alphanumeric/`_`/`'`; and NOT in the reserved set: module, import, data,
  newtype, type, class, where, let, in, if, then, else, case, of, do). It is
  emitted when the remainder of the line either starts with `::` (type
  signature) OR contains `=` (equation/CAF/guard head). Both the signature
  line and the equation line are emitted independently, so one function often
  yields two same-named symbols.
- `data` / `newtype` / `class` / `type`: emitted when the (trimmed) line
  starts with that keyword followed by a space. The name is
  `first_type_token(rest)`: text after the LAST `=>` (so class contexts are
  skipped), then the first whitespace token, with trailing non-alphanumeric
  (except `_` and `'`) punctuation stripped. NOTE: this rule is NOT gated by
  indentation — an indented `data`/`class`/etc. line is still matched.

## What is deliberately ignored
- Indented (`let`/`where`) local bindings: excluded by the top-level gate
  for functions; but the TYPE-keyword rule is not indented-gated.
- Comments: `--` line comments and `{- -}` block comments are NOT parsed as
  code as long as the trimmed line does not start with a type keyword or a
  lowercase-identifier-then-`=` (the scanner is line-based, not comment-aware).
- String contents: never scanned for declarations.
- Pragmas (`{-# ... #-}`) and Template-Haskell splices (`$(...)`): first token
  is not a valid identifier/keyword, so they are skipped (macros are NOT
  expanded and their generated names are NOT seen — per §8 macros-usually-not-API).
- `module ... where` and `import ...` lines: reserved words, never emitted.

## Known misses / gaps (current source)
1. Module export lists are NOT parsed: every top-level declaration is treated
   as public, so unexported helpers are over-reported (see `edge/export_list.hs`).
2. `type family` / `data family`: `first_type_token` returns the literal token
   `family` instead of the family name (see `edge/type_family.hs`).
3. Infix operator definitions, e.g. `(+++) x y = ...`: first token is not a
   valid identifier, so they are not flagged.
4. `pattern` synonyms (`pattern P x = ...`): `pattern` is not in the reserved
   set and the line contains `=`, so it is mis-reported as a `function`
   named `pattern`.
5. Signature/type changes that keep the same name are INVISIBLE to breaking
   detection (see breaking definition below).
6. `modified` is never populated by `basic_diff`; only name membership matters.
7. `.lhs` PROSE lines are still scanned raw: a prose line that (after trim)
   starts with `data`/`newtype`/`class`/`type`, or with a lowercase identifier
   later followed by `=`, would false-positive. The negative literate fixture
   avoids this by capitalizing prose sentences.

## Breaking definition (`detect_breaking_changes`)
`basic_diff` compares symbol NAMES only (a `HashSet<&name>`). `added` = names
new in `new`; `removed` = names present in `old` but absent in `new`;
`modified` = always empty. `detect_breaking_changes` returns
`!diff.removed.is_empty()` — i.e. breaking IFF at least one previously-detected
public symbol NAME is no longer present. Consequences:
- Removing or RENAMING a declaration => breaking (a rename removes the old name).
- Adding a declaration => NOT breaking.
- Changing a function's type, or turning `data` into `newtype`, while keeping
  the SAME name => NOT breaking (name still present; kind is ignored).

## Per-file expectations

positive/
- `functions.hs`  -> expect >=1 symbol kind `function` (names include `add`,
  `secret`, `greet`; sig+equation lines duplicate names).
- `data_newtype.hs` -> expect kind `data` name `Result` AND kind `newtype`
  name `Identity`.
- `class_type.hs` -> expect kind `class` name `Comparable` (context skipped)
  AND kind `type` name `Name`; the indented `compare` method is NOT flagged.
- `guards.hs` -> expect >=1 kind `function` name `clamp` (from the signature
  line; the guard `|` lines are indented and ignored).
- `literate.lhs` -> expect >=1 kind `function` name `double` from the `>`
  Bird-track lines; capitalized prose yields nothing.

negative/
- `comments.hs` -> expect 0 public symbols (commented-out decls not flagged).
- `module_imports.hs` -> expect 0 public symbols (`module`/`import` reserved;
  export list not parsed).
- `literate_prose.lhs` -> expect 0 public symbols (no `>` code lines;
  capitalized prose does not start a declaration).

breaking/ (each is a before->after PAIR)
- `remove_function` -> after removes the `add` function while keeping
  `Result`; `removed` contains `add` -> breaking=true.
- `remove_data` -> after removes the `Result` data while keeping `combine`;
  `removed` contains `Result` -> breaking=true.
- `rename_function` -> `foo` renamed to `bar`; `removed` contains `foo`
  (and `added` contains `bar`) -> breaking=true (rename == removal by name).

edge/
- `export_list.hs` -> adapter IGNORES the export list, so BOTH `exported`
  and the unexported `notExported` are flagged (expect >=2 distinct function
  names). Demonstrates the visibility over-report gap; not "correct" Haskell.
- `template_haskell.hs` -> expect kind `function` name `real` only; the
  `$(makeLenses ''Config)` splice and the LANGUAGE pragma are ignored and
  produce no symbols (TH expansions are not API per §8).
- `type_family.hs` -> expect kind `type` AND kind `data` symbols, but their
  `name` is the literal `family` (NOT `Elem`/`Array`). Documents the
  `first_type_token` type/data-family naming gap.
