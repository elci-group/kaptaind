# Erlang adapter fixture notes

Source of truth: `src/diff/lang/adapters/erlang.rs` (+ `src/diff/lang/adapters/common.rs`).
All expectations below are derived strictly from that source as it reads today,
including behavior that may be undesirable. Nothing here was measured by running
the adapter.

## Extensions matched (`detect_files`)

Only `.erl` and `.hrl`. Anything else (`.beam`, `.app.src`, `rebar.config`,
`.yrl`, `.xrl`, `.config`, no extension) is ignored.

## Public-symbol rules by `kind` (`erlang_parse` / `parse_ast`)

Line-oriented, prefix based; the trimmed line is matched with `strip_prefix`.

- `module` — line starts with `-module(`; name = text up to the first `)`,
  trimmed; emitted **unconditionally** (no export required). One per directive.
- `record` — line starts with `-record(`; name = text up to the first `,`,
  trimmed; emitted **unconditionally** (records are treated as always-public).
- `macro` — line starts with `-define(`; name = text up to the first `,`,
  trimmed; emitted **unconditionally** (macros are treated as always-public).
- `function` — emitted only when ALL hold:
  1. the def line contains the substring ` ->` (space-arrow);
  2. the head (before ` ->`) contains `(`; the name before `(` is non-empty and
     starts with a **lowercase** letter (Erlang atom);
  3. the args slice from `(` onward `ends_with(')')`;
  4. `name/arity` is present in the `-export` set, where `arity` is
     `function_arity`: 0 for `()`, otherwise the count of `split(',')` of the
     text inside the outer parens (naive — nested commas in tuples/maps are
     miscounted).
- The `-export` set is built only from a **single-line** `-export([ ... ])`: the
  parser takes text up to the first `]` on that same line, strips a leading `[`,
  splits on `,`, and trims each item. Multi-line export lists are not captured.
- Everything else is ignored: `-import`, `-include`/`-include_lib`,
  `-ifdef`/`-ifndef`/`-else`/`-endif` (transparent — inner lines are still
  parsed), `-behaviour`, `-spec`/`-type`/`-opaque`, `-compile`, `%%`/`%` comments
  (ignored only because the trimmed line starts with `%`, so no prefix matches),
  and function bodies.

There is no string-literal awareness: directive-shaped text inside a multi-line
string that is not prefixed by `"` on its physical line is parsed as a real
directive (see `edge/codegen_string.erl`).

## Known misses / gaps (source-derived, NOT fixed here)

1. **Multi-line `-export([...]).`** — only the first line is read; exports listed
   on following lines are never added to the set, so their functions are never
   emitted (`edge/multiline_export.erl`).
2. **Multi-argument guarded clauses** — a head like `f(A, B) when A > B ->` has
   an args slice `(A, B) when A > B` that does not end with `)`, so rule 3 fails
   and an exported function is missed. Single-arg guards parse by accident when
   the guard's last token ends in `)` (`edge/guarded_multiarg.erl` vs.
   `positive/guarded_single_arg.erl`).
3. **Naive arity** — `function_arity` counts raw commas; a single tuple/map
   argument `f({A, B})` is counted as arity 2, so it won't match a real `f/1`
   export.
4. **` ->` requires a leading space** — `f()->ok.` (no space) is not matched.
5. **No string/comment awareness beyond `%` line prefix** — phantom symbols can
   be emitted from code embedded in multi-line strings (`edge/codegen_string.erl`).
6. **Records/macros always public** — `-record`/`-define` are emitted even when
   the module exports nothing; headers (`.hrl`) are the legitimate case, but in a
   plain `.erl` they are over-reported as API.

## Breaking definition (`detect_breaking_changes`)

`!diff.removed.is_empty()` — **removals only**, where `basic_diff` compares
symbol **names** (kind is ignored). Therefore breaking = any previously-public
name disappears:

- removing an exported function (e.g. `stop/1` drops out);
- changing a function's arity, since the qualified name changes
  (`connect/1` -> `connect/2`: `connect/1` is removed -> breaking);
- removing a `-record`, `-define`, or `-module` (they are always-public names);
- renaming a module.

Not breaking (per this code): adding symbols; changing a function body; changing
record fields or a macro's value (name unchanged); widening an export list;
signature/`-spec` changes that keep the same `name/arity`.

## Per-file expectations

### positive/
- `positive/basic_module.erl` -> module `basic_module`; functions `start/0`,
  `stop/1`; record `state`; macro `MAX_LIMIT`. Must NOT contain
  `private_helper/0` (not exported). Total 5 symbols.
- `positive/header.hrl` -> record `user`; macros `MY_HEADER_HRL`, `APP_NAME`,
  `TIMEOUT_MS`; 0 module; 0 function. Total 4 symbols.
- `positive/multi_arity.erl` -> module `multi_arity`; functions `connect/1` and
  `connect/2` (distinct arities = distinct symbols; `connect/2` emitted once).
- `positive/guarded_single_arg.erl` -> module `guarded_single_arg`; function
  `classify/1` emitted (single-arg guarded heads happen to end with `)`; the
  catch-all clause also matches).
- `positive/zero_arity.erl` -> module `zero_arity`; functions `start/0`,
  `version/0` (arity 0 from `()`).

### negative/
- `negative/private_functions.erl` -> module `private_functions` only; 0
  function/record/macro (`-export([])` -> empty export set). Total 1 symbol.
- `negative/comments_only.erl` -> module `comments_only` only; the
  `%`-commented `-export`/`-record`/`-define` are ignored; `ghost/1` not
  exported. Total 1 symbol.
- `negative/no_export_attribute.erl` -> module `no_export_attribute` only; no
  `-export` directive -> export set empty -> `start/0`, `stop/1` not emitted.
  Total 1 symbol.

### breaking/ (before -> after, all breaking=true)
- `breaking/remove_function` -> after removes `stop/1` from export + def;
  removed={`stop/1`} -> breaking=true.
- `breaking/change_arity` -> `connect/1` becomes `connect/2`;
  removed={`connect/1`}, added={`connect/2`} -> breaking=true.
- `breaking/remove_record` -> `-record(state, ...)` removed; removed={`state`}
  -> breaking=true (records are always-public per the adapter).

### edge/
- `edge/multiline_export.erl` -> module `multiline_export` only; 0 functions.
  `start/0`,`stop/0` ARE exported in source but the single-line export parser
  misses the multi-line list -> KNOWN MISS (expect 0 function today).
- `edge/guarded_multiarg.erl` -> module `guarded_multiarg` only; 0 functions.
  `max/2` IS exported but both clauses are multi-arg guarded (args slice does not
  end with `)`) -> KNOWN MISS (expect 0 function today).
- `edge/codegen_string.erl` -> module `codegen_string` PLUS a spurious function
  `generated/0`: the inner `-export([generated/0]).` line (no leading `"`)
  pollutes the export set and the inner `generated() ->` head matches it.
  The inner `-module(generated)` line is NOT emitted (leading `"`).
  BUG: string-unaware parsing -> expect a phantom `generated/0` today.
