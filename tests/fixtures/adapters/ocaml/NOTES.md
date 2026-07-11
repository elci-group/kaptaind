# OCaml adapter fixture notes

Source: `src/diff/lang/adapters/ocaml.rs` (+ `src/diff/lang/adapters/common.rs`).
All expectations below are derived strictly from what the source does today,
not from ideal OCaml semantics.

## Extensions matched (`detect_files`)
- `ml`, `mli` only (lowercase; `matches!(e, "ml" | "mli")`). Anything else is ignored.

## Public-symbol rules (`parse_ast` -> `kind`)
Line-oriented scan via `read_lines_safe` (file must be <= 5 MiB). For each line:
`trim`, then if the trimmed line `starts_with("(*")` the whole line is skipped.
Otherwise the FIRST matching prefix wins (one symbol per line max), in this order:
- `module type ` -> kind `module_type`, name = first_ocaml_name(rest)
- `module `      -> kind `module`,      name = first_ocaml_name(rest)
- `let `         -> kind `let`,         name = first_ocaml_name(rest)
- `type `        -> kind `type`,        name = first_ocaml_name(rest)
- `val `         -> kind `val`,         name = first_ocaml_name(rest)

`first_ocaml_name`: walk whitespace tokens; drop tokens starting with `'` or `(`
(type params `'a` / `('a, 'b)`); drop a leading `rec` (so `let rec f` -> `f`);
trim trailing `, ) = :`; return the first token whose first char is alphabetic,
else `None` (so `let () =`, `let _ =` yield NO symbol).

There is NO visibility and NO scope model: every matched declaration is emitted
as a public symbol. `.ml` and `.mli` are treated identically. Comment/string
handling is limited to the single `starts_with("(*")` check.

## Breaking definition (`detect_breaking_changes` + `basic_diff`)
`basic_diff` compares symbol sets by NAME only (`modified` is always empty).
`detect_breaking_changes` = `!diff.removed.is_empty()`. Therefore:
- Breaking ONLY when a previously-present symbol NAME disappears.
- NOT breaking: additions; body/signature changes with the same name
  (e.g. `val sum : int->int` -> `val sum : string->string`); kind changes with
  the same name (`let x` -> `type x`); type-definition changes with same name.

## Known misses / gaps (report, do not fix)
- Block comments: only lines beginning with `(*` are skipped. A commented-out
  declaration on its own line (no leading `*`) is parsed as a real symbol
  (false positive). See `edge/block_comment_false_positive.ml`.
- No scope awareness: local `let` bindings inside a function body are reported
  as public top-level `let`s. See `edge/local_let_over_detection.ml`.
- No visibility model: items hidden by an `.mli`, `private` type qualifiers, and
  module-local bindings are all treated as public.
- No constructs beyond the 5 prefixes: `exception`, `external`, `class`/`method`,
  `include`, `open`, attributes/PPX (`[@@...]`) are all ignored.
- String contents are not special-cased; a `let`-looking line inside a
  multi-line string would be parsed.
- Signature/kind/body changes are invisible to breaking detection (name-only
  diff); only removals count.

## Per-file expectations

positive/let_bindings.ml     -> 4 symbols, all kind `let`
                                (add, fact[`let rec`->fact], name, pi); expect >=1 `let`
positive/types.ml            -> 4 symbols, all kind `type`
                                (t, color, tree[`'a` skipped], pair[(`'a,'b`) skipped]); expect >=1 `type`
positive/modules.ml          -> Foo(module), S(module_type), Make(module);
                                `module type` matched before `module` (no double count);
                                expect >=1 `module` and >=1 `module_type`
positive/interface.mli       -> sum(val), prod(val), t(type), option(type [`'a` skipped]),
                                MONAD(module_type); expect >=1 `val`,`type`,`module_type`
positive/mixed.ml            -> version(let), length(let [`rec`]), t(type), Config(module);
                                leading `(* ... *)` line skipped; expect >=1 `let`,`type`,`module`

negative/comments.ml         -> every line starts with `(*`, all skipped; expect 0 symbols
negative/nondecl.ml          -> open/include/exception/external/[@@@...] unmatched,
                                `let () =` and `let _ =` yield no name; expect 0 symbols
negative/interface_comments.mli -> comment-only mli; expect 0 symbols

breaking/remove_val          -> before has val sum+prod, after keeps only prod;
                                name `sum` removed -> breaking=true
breaking/remove_let          -> before has let add+sub, after keeps only sub;
                                name `add` removed -> breaking=true
breaking/remove_type         -> before has type t+u, after keeps only u;
                                name `t` removed -> breaking=true

edge/block_comment_false_positive.ml -> source emits 1 symbol `ghost` (kind `let`)
                                from inside a `(* ... *)` block (naive comment scan);
                                expect >=1 `let` (spurious -- known miss)
edge/local_let_over_detection.ml -> emits total/acc/add, all kind `let`; acc & add are
                                local bindings reported as public (no scope model);
                                expect >=3 `let` (over-detection -- known miss)
edge/poly_and_functor.ml     -> result(type, params skipped), id(let, locally-abstract
                                `(type a)` harmless), OrderedType(module_type),
                                Make(module, functor arg skipped);
                                expect >=1 `type`,`let`,`module_type`,`module`
