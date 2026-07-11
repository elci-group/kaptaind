# C adapter — fixture expectations (source-derived)

Adapter: `src/diff/lang/adapters/c.rs` (`CAdapter`, `c_parse`).
Helpers: `read_lines_safe`, `basic_diff`, `calculate_hash` (`common.rs`).
All expectations below describe what the adapter does TODAY, not what it
"should" do. No accuracy/precision/recall/F1 is claimed (adapter not run here).

## detect_files — extensions matched
Only exact, case-sensitive extensions: `.c` and `.h` (`e == "c" || e == "h"`).
Anything else (`.C`, `.H`, `.cc`, `.cpp`, `.rs`, no extension) is ignored.

## parse_ast — public-symbol rules (per line, in order; first match `continue`s)
Per line: `trimmed = line.trim()`. Empty lines and lines starting with `//` are
skipped (only `//` line comments; `/* */` block comments are NOT stripped).

- `kind = "macro"` — line starts with `#define ` (note the required trailing
  space). Name = first whitespace token of the remainder; must be a valid
  identifier. Object-like macros only: `#define MAX_SIZE 1024` -> `MAX_SIZE`.
  Function-like macros (`#define ADD(a,b) ...`) are NOT detected because the
  first token (`ADD(a,`) fails `is_valid_identifier`.
- `kind = "struct"` — line starts with `struct `. Name = first token of the
  remainder, with trailing `{`, `;`, `*` trimmed; must be a valid identifier.
  Forward declarations (`struct config;`) are detected. `typedef struct {...}`
  (does not start with `struct `) and anonymous structs are missed.
- `kind = "enum"` — same shape as struct, prefix `enum `. `typedef enum` /
  anonymous enums are missed.
- `kind = "function"` — first `(` on the line; text before it split on
  whitespace; needs >= 2 tokens; name = last token, "return type" = the token
  immediately before it; both must be valid identifiers and name must not be a
  control keyword (`if for while switch return goto case sizeof`). Works for
  declarations and definitions, single- or multi-word return types (uses only
  the token right before the name). Pointer-return functions (`int *f(...)`,
  `const char *f(...)`) are MISSED (name token starts with `*`). `static` is
  ignored -> file-local functions ARE flagged public (false positive). A
  `return name(...)` statement can be misread as a function (false positive).

There is no visibility model: every matched construct is emitted as public.
`#include`, `#undef`, `#ifdef`/`#if`/`#endif` are ignored (no prefix/`(` match).

## breaking definition (`detect_breaking_changes` + `basic_diff`)
`basic_diff` compares symbol SETS BY NAME ONLY (kind is ignored for diffing;
`modified` is always empty). `detect_breaking_changes` = `!diff.removed.is_empty()`.
So breaking == at least one old symbol NAME is absent in the new file.
- Removal of any named symbol (function/struct/enum/macro) -> breaking = true.
- Rename -> old name removed -> breaking = true.
- Signature/body/return-type/param change that KEEPS the same name -> NOT
  removed -> breaking = FALSE (signature changes are not detected as breaking).
- Kind change with same name (`struct point` -> `enum point`) -> not breaking.
- Pure addition -> not breaking.
(Note: `structure_hash` DOES change on kind/signature edits, but the breaking
decision uses only the name-based `removed` set.)

## Known misses / gaps (report only; not fixed)
1. Function-like macros not detected (`#define ADD(a,b)`).
2. Pointer-return functions missed (`int *f(...)`, `const char *f(...)`).
3. `typedef struct` / `typedef enum` and anonymous structs/enums missed.
4. Multi-line signatures where the return type is on its own line are missed.
5. `static` (file-local) functions are flagged public — no visibility model.
6. `/* ... */` block comments are not stripped -> prototypes inside block
   comments are false positives.
7. `return name(...)` can be misread as a function declaration.
8. Generated/minified/codegen files (lex/yacc, `*.generated.c`) are not
   detected or skipped (§8 generated class unhandled).
9. Signature/return-type/param changes that keep the same name are NOT breaking.
10. Diff is by name only; `AstDiff.modified` is never populated.

## Per-file expectations

positive/macros.h             -> 3 symbols kind 'macro': MAX_SIZE, APP_NAME, PI
positive/structs.h            -> kind 'struct' symbols incl. point, config, node
                                 (node appears twice: decl + member line `struct node *next;`)
positive/enums.h              -> 2 symbols kind 'enum': color, status
positive/functions.h          -> 3 symbols kind 'function': add, print_point, get_id
positive/definitions.c        -> 2 symbols kind 'function': add, greet (.c detected too)
positive/multi_word_return.h  -> 3 symbols kind 'function': get_ticks, get_mode, compute_delta

negative/line_comments.h      -> 0 public symbols (all `//` lines skipped)
negative/control_flow.c       -> 0 public symbols (control keywords/calls skipped;
                                 `int *allocate(...)` itself missed: pointer-return miss)
negative/string_literals.c    -> 0 public symbols (code-like text inside strings not flagged)
negative/typedef_anonymous.h  -> 0 public symbols (typedef/anonymous struct+enum missed)

breaking/remove_function      -> after removes function `add` -> removed non-empty -> breaking = true
breaking/remove_struct        -> after removes struct `point` -> removed non-empty -> breaking = true
breaking/rename_macro         -> MAX_SIZE renamed to MAX_BYTES (old name removed) -> breaking = true

edge/function_like_macro.h    -> macro PI detected; ADD/SQUARE NOT detected (function-like miss)
edge/static_visibility.c      -> 2 functions detected: helper, public_api; `helper` is
                                 `static` (file-local) but still flagged public (gap #5)
edge/block_comment.h          -> 3 functions detected: commented_out, also_hidden, real_api;
                                 commented_out/also_hidden live inside a `/* */` block and
                                 are FALSE POSITIVES (gap #6)
