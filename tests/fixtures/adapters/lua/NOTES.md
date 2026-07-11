# Lua adapter fixture notes

Source of truth: `src/diff/lang/adapters/lua.rs` (+ `src/diff/lang/adapters/common.rs`).
All expectations below are derived from what the code does today, not from ideal Lua
semantics. Where the two disagree, it is called out as a known gap.

## Detection (`detect_files`)

- Extension matched: **exactly `.lua`** (case-sensitive `extension() == "lua"`). No shebang,
  no other extensions. `.luau`, `.lua.txt`, etc. are ignored.

## Public-symbol rules (`parse_ast`)

File is read line-by-line (`read_lines_safe`, 5 MB cap). Each line is `.trim()`med, so
leading indentation is irrelevant and there is no scope/nesting awareness.

| kind | rule | symbol `name` |
|------|------|---------------|
| `module_export` | Line has an assignment `=` (first `=` that is not part of `==`/`~=`/`<=`/`>=`); the trimmed LHS literally starts with `M.` and the suffix is a valid identifier (`.`/`:` allowed). | the full LHS, e.g. `M.foo`, `M.deep.run` |
| `function` | Line starts with `function ` AND does not start with `local function `; name = first token after `function ` up to `(`; must be a valid identifier (`.`/`:` allowed, so `M.foo`/`M:method` qualify). | the function name, e.g. `greet`, `M.add`, `M:reset` |

Privacy model: `local function foo` is the only thing treated as private, and only because
it fails the `starts_with("function ")` test. There is **no** check for `return M`, no
scope tracking, and no comment/string stripping.

## Known misses / gaps (source-derived)

- Only the literal identifier `M` is recognized for `module_export`. `MyModule.foo = ...`,
  `exports.foo = ...`, `mod.foo = ...` are NOT exports. (The `function` rule is NOT
  `M.`-bound, so `function MyModule.bar()` IS still detected — see edge/.)
- Computed / string-key exports are invisible: `M["x"] = ...` and `M[key] = ...` start with
  `M[` not `M.`, so they are never detected (dynamic/string-based API class, roadmap §8).
- Long brackets are not stripped. Detectable patterns inside `--[[ ... ]]` block comments
  or `[[ ... ]]` long strings leak out as false positives on their own lines. (The `--`
  single-line comment and single-line `"..."`/`'...'` strings are safe because the `--`
  prefix blocks the match, and the first `=` on a `local x = "..."` line belongs to the
  real assignment, whose LHS is `local x`.)
- Anonymous functions assigned to a field (`M.f = function() end`) count as `module_export`,
  not `function`; bare `function()` is ignored.
- No generated/minified handling. A fully minified single-line module tends to yield ~0
  symbols because only the first `=` and a line-initial `function ` are considered.
- Signature/parameter changes are invisible: a `Symbol` carries only the name, so changing
  `function M.add(a,b)` to `function M.add(a,b,c)` produces no diff at all.

## Breaking definition (`detect_breaking_changes` + `basic_diff`)

- `basic_diff` compares symbol **names only**: `added` = names in new not in old,
  `removed` = names in old not in new, `modified` = always empty.
- Breaking ⇔ `!diff.removed.is_empty()` — i.e. **removals only**. Renames count as breaking
  (old name removed). Additions, body edits, and same-name signature changes are NOT
  breaking.

## Per-file expectations

positive/

- `module_table.lua` → 3 `module_export`: `M.add`, `M.sub`, `M.VERSION`; 0 `function`
  (anonymous fns sit on the RHS).
- `module_functions.lua` → 3 `function`: `M.add`, `M.sub`, `M:reset` (colon method valid).
- `global_functions.lua` → 2 `function`: `greet`, `sum`.
- `nested_export.lua` → 3 total: `module_export` `M.deep` (from `M.deep = {}`) and
  `M.deep.run`; `function` `M.deep.walk`.
- `mixed_api.lua` → 2 total: `module_export` `M.ping`, `function` `M.echo`; `sanitize`
  NOT detected (`local function`).

negative/

- `private_locals.lua` → 0 public (only `local function` / `local` vars).
- `comments.lua` → 0 public (`--` line comments block the match).
- `strings.lua` → 0 public (patterns live inside single-line string literals; the real
  assignment LHS is `local <var>`).

breaking/

- `remove_export` (before: `M.foo`,`M.bar` → after: `M.bar`) → removed `{M.foo}` →
  breaking = true.
- `remove_function` (before: `function M.add` → after: none) → removed `{M.add}` →
  breaking = true.
- `rename_export` (before: `M.old_name` → after: `M.new_name`) → removed `{M.old_name}`,
  added `{M.new_name}` → breaking = true (rename == removal).

edge/

- `dynamic_computed_keys.lua` → 0 public. `M["dynamic"]`/`M[key]` start with `M[`, not
  `M.`. Known under-detection (conceptually public, unseen).
- `non_M_module_name.lua` → 1 public: `function MyModule.bar` (function rule is
  prefix-agnostic). `MyModule.foo` is NOT an export (only literal `M.`). Shows the
  export/function asymmetry.
- `local_module_not_returned.lua` → 2 public: `module_export M.hidden`, `function
  M.secret`. Known over-detection: members of a local, never-returned table are still
  flagged (no scope/`return M` check).
