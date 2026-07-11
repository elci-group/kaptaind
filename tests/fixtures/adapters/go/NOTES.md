# Go adapter fixture notes

Source of truth: `src/diff/lang/adapters/go.rs` (helpers in `src/diff/lang/adapters/common.rs`).
Expectations below are derived strictly from that source today, not from "correct" Go semantics.

## Extensions matched (`detect_files`)
- Only paths whose `Path::extension()` equals the literal lowercase `go` (`e == "go"`).
- Case-sensitive: `.Go` / `.GO` do NOT match. No shebang or filename fallback.

## Public-symbol rules (`go_parse`)
Line-based scanner over `read_lines_safe` (files >5 MiB rejected; reading stops at the
first I/O error via `map_while(Result::ok)`). Each line is `trim()`med, then:

- Line starts with `func ` -> let `rest` = text after `func `.
  - If `rest.chars().next()` is uppercase -> emit `kind="function"`, `name = rest` (the
    FULL remainder of the line: params, returns, trailing `{`).
  - ADDITIONALLY, if `ver >= (1,18)` and `rest.contains('[')` -> emit
    `kind="generic_function"`, `name = "<part before '['>[...]"` (when that prefix starts
    uppercase). So an exported generic func emits TWO symbols under a versioned parse.
- Line starts with `type ` -> let `rest` = text after `type `.
  - If `rest.chars().next()` is uppercase -> emit `kind="generic_type"` when
    `ver >= (1,18)` and `rest.contains('[')`, else `kind="type"`; `name = rest` (full
    remainder).
- `name` always stores the whole `rest`, so diffs key on the entire signature/type text.
- Default `parse_ast` uses `ver = (1,0)`, so the `generic_*` kinds are NOT produced unless
  `parse_ast_versioned` is called with a Go version `>= 1.18`.

## What it deliberately ignores (produces no symbol)
- Any line not starting with `func ` / `type ` after trim: `package`, imports, `const`,
  `var`, comments (`//`, `/*`), string literals, blank lines, build tags (`//go:build`).
- Lowercase-first identifiers after `func`/`type` (unexported) — uppercase test gates them.
- NOTE: there is no generated-file guard (`// Code generated ... DO NOT EDIT.` is ignored
  like any comment); generated files are parsed normally.

## Known misses / gaps (source-derived, NOT fixed here)
1. Exported methods with a receiver are NOT detected: `func (s *Server) Start()` -> after
   `func ` the next rune is `(`, which is not uppercase, so 0 `function` symbols even
   though the method is public API.
2. Exported `const` / `var` (single or grouped) are never emitted — no rules for them.
3. Grouped `type ( ... )` blocks: inner lines do not start with `type `, so they are missed.
4. Generics get `generic_*` kinds only under versioned parse `>= 1.18`; default parsing
   emits plain `function`/`type` kinds for generic declarations.
5. No generated/minified down-weight or skip (roadmap §8): a generated file's exported
   `func`/`type` would be flagged as public.

## Breaking definition
`detect_breaking_changes(diff) = !diff.removed.is_empty()`.
`basic_diff` compares symbol sets by `name` only (added/removed filled; `modified` always
empty). Because `name` embeds the full signature/type text, BOTH a pure removal AND a
signature/parameter change surface as a non-empty `removed` set -> `breaking = true`.
Body-only edits, added comments, and added new symbols leave `removed` empty -> not breaking.

## Per-file expectations

positive/
- exported_function.go      -> >=1 symbol, kind `function` (name `Add(a, b int) int {`).
- exported_struct.go        -> >=1 symbol, kind `type` (name `Point struct {`).
- exported_interface.go     -> >=1 symbol, kind `type` (name `Stringer interface {`).
- exported_type_alias.go    -> >=1 symbol, kind `type` (name `UserID string`).
- multiple_exported.go      -> >=2 symbols: >=1 `type` (Config), >=1 `function` (NewConfig).

negative/
- private_function.go       -> 0 public symbols (lowercase `add`/`helper`).
- private_type.go           -> 0 public symbols (lowercase `point`/`stringer`).
- comments_and_strings.go   -> 0 public symbols (func/type text only in comments/strings).
- blank_and_imports.go      -> 0 public symbols (no `func`/`type` lines).

breaking/ (each pair: breaking=true)
- remove_function: before has `Add(...)`, after renames to `Sum(...)` -> old name removed.
- change_signature: `Greet(name string)` -> `Greet(name string, loud bool)` changes the
  stored name (full signature), so the old name is removed -> breaking (signature change
  is detected via name-includes-signature).
- remove_type: `Config` type removed, `Settings` added -> old type name removed.

edge/
- generics.go -> default parse (`ver 1,0`): 1 `function` (Map...) + 1 `type` (Stack...);
  NOT `generic_*`. Under `parse_ast_versioned` with go>=1.18: additionally
  `generic_function` (`Map[...]`) and `generic_type` (Stack...). Version-dependent.
- build_tag.go -> >=1 `function` (PlatformName...); the `//go:build` line is not a symbol.
  Conditional compilation does not gate detection.
- method_receiver.go -> 1 `type` (Server...); 0 `function`. The exported receiver method
  `Start` is missed (rune after `func ` is `(`) -> documents known miss #1.
