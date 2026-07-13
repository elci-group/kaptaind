# Julia adapter calibration corpus (adapter-200 item 10, rev 34)

Semantics: Julia's public API is convention-gated — top-level declarations
whose names do not start with `_`: `module`/`baremodule`, `struct`/`mutable
struct`, `abstract type`, long-form `function name(params)`, short-form
`name(params) = expr`, `macro name(params)`, `const NAME = ...`, and struct
fields (dot-accessible; they define the default constructor). Qualified
definitions (`function Base.show(io, x)`, `Base.length(t) = ...`) emit the
final dotted component. Declarations nested below block depth 1 (keyword-
delimited `end` tracking) are not surface. Method signatures are canonical
dispatch-type tuples (`(Int,String)`; untyped params → `Any`, defaults
dropped, `{}` parametric commas preserved), so parameter renames are
invisible but type changes register as modifications. Headers complete on
balanced parens. Born-correct `#`/`#= =#` comments and `"""` docstring
tracking. Known T2 limits: export-list cross-referencing (follow-up),
operator overloads, `primitive type`, `where` clauses, `do`-block depth skew.

- positive/: a module with const/abstract/struct+fields/functions,
  short-form functions incl. parametric struct and `!`-mutating names,
  qualified `Base.show`/`Base.length` extensions, and multi-line headers →
  all must yield symbols.
- negative/: plain script statements (assignments, calls, for/if blocks) and
  fake declarations in `#`/`#= =#` comments and `"""` docstrings → zero
  symbols.
- breaking/: `remove_function`/`remove_field` pairs delete surface members →
  `diff.removed` non-empty → breaking fires. `control` adds a statement
  inside a function body — surface unchanged → NOT breaking.
- modified/: same-name declaration changes kind (struct→abstract,
  function→const, macro→function) → X2 `modified` fires. `control` adds a
  body statement → symbols and signatures unchanged → not modified (by
  design).
- signature/: `change_param_type` alters a parameter type → dispatch tuple
  changes → `modified` fires via signature. `rename_param` renames a
  parameter → tuple unchanged → NOT modified (control).
