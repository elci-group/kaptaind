# Groovy adapter calibration corpus (adapter-200 item 10, rev 33)

Semantics: Groovy members are `public` by default, so surface = every
declaration without explicit `private`/`protected`:
`class`/`interface`/`trait`/`enum`/`@interface` declarations, methods
(`def name(...)` or `Type name(...)`, including script-level methods),
constructors (PascalCase name, no return type), and properties — a field
without a visibility keyword generates public getters/setters. Properties are
emitted only at class-body depth 1 so method-local `def x`/`String x` are not
mistaken for them (nested-class members below depth 1 are a documented miss).
Method/constructor signatures are canonical parameter-type tuples
(`(int,String)`; bare parameters canonicalize to `def`, default values and
annotations dropped), so parameter renames are invisible but type changes
register as modifications. Headers may span lines; the scanner completes a
header at the `{`/`;` terminator OR on balanced parens alone (Groovy
interface/abstract methods need neither). Born-correct `//`/`/* */` comments,
`#!` shebang, and `'''`/`"""` triple-quoted string tracking. Known T2 limits:
PascalCase call-with-closure (`Frame(title: "x") { }`) is indistinguishable
from a constructor; annotation arguments on the declaration line can hide the
method; `.gradle` DSL files are out of scope.

- positive/: a service class (properties + constructor + methods incl.
  static), type declarations (interface with terminator-less methods, trait,
  enum, `@interface`), a script with top-level methods and call statements,
  and multi-line headers incl. a generic method → all must yield symbols.
- negative/: pure script statements (assignments, `println`, `assert`,
  `each`-closures, call-with-args) and fake declarations in comments and
  triple-quoted strings → zero symbols.
- breaking/: `remove_method`/`remove_property` pairs delete surface members →
  `diff.removed` non-empty → breaking fires. `control` removes a `private`
  method — surface unchanged → NOT breaking (exercises the visibility skip).
- modified/: same-name declaration changes kind (method→property,
  class→interface, trait→class) → X2 `modified` fires. `control` adds a
  statement inside a method body → symbols and signatures unchanged → not
  modified (by design).
- signature/: `change_param_type` alters a parameter type → canonical tuple
  changes → `modified` fires via signature. `rename_param` renames a
  parameter → tuple unchanged → NOT modified (control).
