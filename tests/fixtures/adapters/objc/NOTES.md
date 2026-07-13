# Objective-C adapter calibration corpus (adapter-200 item 10, rev 36)

Semantics: an Objective-C API is its *runtime* surface — `@interface` /
`@implementation` (kind `class`), `@protocol`, `@property`, Apple's
`NS_ENUM`/`NS_OPTIONS` type macros (kind `enum`), and methods identified by
their **selector**: the full keyword name with colons
(`setName:age:active:`), which is the stable identity used by message
dispatch and `@selector`. The selector is the symbol name directly, so
renaming any keyword segment registers as a removal/addition (breaking) —
matching ObjC semantics. Parameter types and names are dispatch-invisible
and dropped; a keyword is an identifier immediately followed by `:(` (the
colon is fused to the parameter-type paren, so the scan runs at character
level). Objective-C has no method visibility — the header/implementation
split is a convention the line scanner cannot reconstruct (`.h` is owned by
the C adapter; this adapter claims `.m`/`.mm`) — so the Apple
underscore-prefix internal convention gates surface (`_helper` is not
emitted). Confidence band 0.7. No signatures are recorded (parameter types
are not part of ObjC dispatch). Method headers may span lines (one keyword
segment per line); the scanner accumulates to the `;`/`{` terminator at
paren depth 0. Born-correct comment handling (rev-24/26 discipline): `//`
line comments and `/* ... */` block comments.

- positive/: classes with properties and methods (class + instance), a
  protocol with required/optional methods, multi-segment selectors
  including a multi-line header, and `NS_ENUM`/`NS_OPTIONS` → all must
  yield symbols.
- negative/: plain call sites (`NSLog`, assignments, `[obj doThing]`
  message sends) and fake declarations in `//` and `/* ... */` comments →
  zero symbols.
- breaking/: `remove_method`/`remove_property` pairs delete surface members
  → `diff.removed` non-empty → breaking fires. `control` removes an
  underscore-internal method — surface unchanged → NOT breaking.
- modified/: same-name declaration changes kind (method→property,
  property→method, class→protocol) → X2 `modified` fires. `control` changes
  only a method body inside `@implementation` → symbols unchanged → not
  modified (by design).
- signature/: none — ObjC records no signatures (0/0 by design).
