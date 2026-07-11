# Elixir adapter fixture notes

Source of truth: `src/diff/lang/adapters/elixir.rs` (+ `common.rs`:
`read_lines_safe`, `basic_diff`, `calculate_hash`). All expectations below are
derived strictly from that source — NOT from ideal Elixir semantics.

## detect_files — extensions matched
- `.ex` and `.exs` only (`extension() == "ex" || "exs"`). Nothing else (no `.eex`,
  no `.heex`, no `.erl`, no `.beam`).

## parse_ast — public-symbol rules by kind
Each line is `.trim()`d, then matched by `strip_prefix` in this exact order
(first match wins, via `if / else if`):

| prefix (literal, trailing space required) | emitted `kind` | emitted `name`        |
|-------------------------------------------|----------------|-----------------------|
| `defmodule `                              | `module`       | the rest of the line  |
| `defprotocol `                            | `protocol`     | the rest of the line  |
| `defmacro `                               | `macro`        | the rest of the line  |
| `def `                                    | `function`     | the rest of the line  |

Everything parse_ast emits is returned as public by `extract_api`
(`public_symbols = ast.symbols.clone()`); there is no separate visibility pass.

### Why private/compound forms are skipped (the trailing-space requirement)
Because the prefixes demand a literal space, these do NOT match and yield NO
symbol: `defp`/`defmacrop` (private), and the compound keywords `defstruct`,
`defimpl`, `defexception`, `defdelegate`, `defguard`, `defoverridable`.
`defmacro` is checked before `def`, so public macros are `macro`, not `function`.
Lines starting with `#` (comment), `@` (module attribute: `@doc`, `@spec`,
`@moduledoc`, `@callback`, `@behaviour`), or any identifier/sigil never match.

## Known misses / gaps (report-only; not fixed)
- **Recall (false negatives):** `defdelegate`, `defguard`, `defimpl`, `defstruct`,
  `defexception` are real public surface but are invisible (no trailing space).
  `@callback`/`@behaviour` and Phoenix/Ecto DSL (`get`/`post`/`resources`,
  `pipeline`/`scope`, `schema`/`field`) are not detected at all.
- **Precision (false positives):** a `def ...` line that appears *textually*
  inside a `quote do ... end` block, a `@doc`/`~S"""` heredoc, or a multi-line
  string is still counted as a public `function` (no AST/quoting awareness).
- **Visibility ignored:** `@doc false` / `@moduledoc false` do not suppress the
  following `def`/`defmodule`; hidden API is still flagged public.
- **Generated code:** no detection/down-weight of generated files; any `def*`
  line in generated output is counted.
- `extract_api` never sets `version_tag`/`parser_kind` beyond defaults.

## Breaking definition (`detect_breaking_changes`)
`diff_ast` = `basic_diff`: compares symbols by **name string only**
(`modified` is always empty). A signature/arity change alters the stored name
(the whole rest-of-line, e.g. `greet(name) do` → `greet(name, locale) do`), so
it appears as one removal + one addition. `detect_breaking_changes` =
`!diff.removed.is_empty()`. Therefore: **any removed/changed-name symbol is
breaking; pure additions and body-only edits (same name) are not.** Note: this
over-reports — even adding an *optional* arg changes the name and reads as
breaking; body-only breaking changes are invisible.

## Per-file expectations (source-derived)

positive/
- `modules.ex`        → 2 symbols, both `module` (`MyApp do`, `Inner do`).
- `functions.ex`      → 1 `module` + 3 `function` (incl. the guard/one-liners).
- `macros.ex`         → 1 `module` + 2 `macro` (order keeps `defmacro` ≠ function).
- `protocols.ex`      → 1 `protocol` + 1 `module` + 2 `function`.
- `script.exs`        → `.exs` detected; 1 `module` + 1 `function`.

negative/  (each → exactly 0 symbols)
- `private.ex`        → `defp`/`defmacrop` skipped (no space after `def`).
- `comments.ex`       → `#`- and `@`-prefixed lines never match.
- `strings.ex`        → single-line strings/sigils naming `def*` are not at
                        line-start → not matched. (Multi-line heredocs are the
                        false-positive case; see edge/.)
- `compound_def_keywords.ex` → `defstruct`/`defexception`/`defdelegate`/
                        `defguard`/`@callback`/`@behaviour` → 0 (recall gap:
                        defdelegate/defguard are genuinely public yet missed).

breaking/  (each pair → `detect_breaking_changes == true`)
- `remove_function`   → after drops `def deprecated` → removed=[`deprecated do`] → breaking.
- `add_required_arg`  → name changes `greet(name) do`→`greet(name, locale) do`
                        → removed (old name) + added (new name) → breaking.
- `remove_module`     → after drops `MyApp.Legacy` + its `old` → 2 removals → breaking.

edge/
- `quote_injection.ex` → `defmacro __using__` = `macro`; the `def injected` INSIDE
                        `quote do` is still emitted as a `function` (false
                        positive: quoted/generated code counted as public).
- `doc_false_hidden.ex`→ `@moduledoc false`/`@doc false` ignored → module + 2
                        functions all emitted as public (hidden API not suppressed).
- `phoenix_dsl.exs`   → `.exs` detected; exactly 1 `module` (Router); DSL
                        `use`/`pipeline`/`plug`/`scope`/`get`/`post`/`resources`
                        produce NO symbols (DSL gap).
