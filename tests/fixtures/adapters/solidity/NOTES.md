# Solidity adapter calibration corpus (adapter-200 item 10, rev 32)

Semantics: a Solidity contract's API is its ABI. Surface = `contract`/
`interface`/`library` declarations, `public`/`external` functions, `public`
state variables (they generate getters), `event`s, `error`s, `modifier`s,
`struct`s/`enum`s, and the special entry points (`constructor`, `fallback`,
`receive`). File-level free functions carry no visibility keyword and are
importable, so visibility-less function headers are surface too (modern
Solidity requires visibility on contract functions). `internal`/`private`
members are NOT surface. Explicit visibility model honored → confidence band
0.8. Function/event/error/constructor signatures are recorded as canonical
parameter-type tuples (`(address,uint256)`) — the tuple behind the Solidity
selector/topic0 — so parameter-name changes are invisible but parameter-type
changes register as modifications. Headers may span multiple lines; the
scanner accumulates to the `{`/`;` terminator at paren depth 0.

- positive/: contracts, an interface with `;`-terminated headers and unnamed
  parameters, a library with struct/enum/modifier/error plus a free function,
  and multi-line headers with fallback/receive → all must yield symbols.
- negative/: pragma/import/using-only file and fake declarations in `//`
  (incl. NatSpec `///`) and `/* */` comments → zero symbols. (Any compilable
  contract body has at least the contract declaration as surface, so the
  internal/private skip is asserted in unit tests and the breaking control
  instead of a negative file.)
- breaking/: `remove_function`/`remove_event` pairs delete ABI members →
  `diff.removed` non-empty → breaking fires. `control` removes an `internal`
  function — surface unchanged → NOT breaking (and exercises the visibility
  skip end-to-end).
- modified/: same-name declaration changes kind (function→public-variable,
  event→error — the real 0.8.4 custom-error refactor, struct→enum) → X2
  `modified` fires. `control` adds a statement inside a function body →
  symbols and signatures unchanged → not modified (by design).
- signature/: `change_param_type` alters a parameter type → selector tuple
  changes → `modified` fires via signature. `rename_param` renames a
  parameter — selector semantics say names are not API → signature unchanged
  → NOT modified (control).
