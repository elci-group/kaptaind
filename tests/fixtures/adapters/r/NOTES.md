# R adapter calibration corpus (adapter-200 item 10, rev 35)

Semantics: R defines functions by *assigning* a `function(...)` value, so
surface = top-level (brace depth 0) definitions: `name <- function(params)`
(incl. `=`, `<<-`, and glued `name<-function` forms), R6 classes `Name <-
R6Class("Name", ...)`, and S4 `setClass("Name", ...)` / `setGeneric("name",
...)`. R has no language-level visibility — package API is the NAMESPACE
export list (documented follow-up) — so the dot-prefix internal convention
gates surface (`.helper` is not emitted). Confidence band 0.8. Signatures are
canonical parameter-NAME tuples (`(x,factor,...)` — defaults stripped):
R callers bind arguments by name, so a parameter rename or addition IS an API
change, while a default-value change is not. Headers complete on balanced
parens; a whole multi-line `R6Class(...)` call accumulates. Exclusions:
right-assignment (`function(x) -> name`), S3 methods, `setMethod`, plain
variable assignments, definitions below brace depth 0. Born-correct `#`
comment handling (R has no block comments; Roxygen `#'` included).

- positive/: function assignments in all operator forms, R6 + S4 classes and
  generics, multi-line parameter lists, and defaults/`...` signatures → all
  must yield symbols.
- negative/: plain script statements (assignments, loops, anonymous-function
  calls) and fake definitions in `#`/`#'` comments and string literals →
  zero symbols.
- breaking/: `remove_function`/`remove_class` pairs delete surface members →
  `diff.removed` non-empty → breaking fires. `control` removes a dot-internal
  function — surface unchanged → NOT breaking.
- modified/: same-name definition changes kind (function→R6 class,
  S4 class→generic, function→generic) → X2 `modified` fires. `control` adds
  a statement inside a function body → symbols and signatures unchanged →
  not modified (by design).
- signature/: `rename_param` renames a parameter and `add_param` adds one →
  name tuple changes → `modified` fires via signature. `change_default_value`
  alters only a default → tuple unchanged → NOT modified (control) — names,
  not values, are the contract.
