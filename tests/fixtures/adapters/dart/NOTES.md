# Dart adapter fixture notes

Source of truth: `src/diff/lang/adapters/dart.rs`. All expectations below are
derived strictly from that source as it behaves today, even where the behavior
is questionable (those are flagged under "Known misses / gaps").

## Extensions matched by `detect_files`

- Only paths whose `extension()` is exactly `dart` (i.e. `*.dart`).
- No special handling of generated suffixes: `*.g.dart`, `*.freezed.dart`,
  `*.gen.dart`, `*.part.dart` all still match because their extension is `dart`.

## Top-level-only filter (applied to every kind)

Per line, in this order, the adapter:

1. Drops the line if `trimmed` is empty or starts with `//` (this also catches
   `///` doc comments, since they start with `//`). `/* ... */` block comments
   are NOT skipped — see edge case below.
2. Drops the line if the ORIGINAL (un-trimmed) line starts with a space or tab.
   Only column-0 declarations are considered, so ALL class/enum/mixin/extension
   members (indented) are invisible, even when declared `public` in intent.
3. Tries class, then enum, then extension, then mixin, then top-level function;
   the FIRST match wins (else-if chain). A line can therefore emit at most one
   symbol.

## Public-symbol rules by `kind`

- `class` — `extract_class_name`: strips one of `abstract class `,
  `final class `, `sealed class `, `base class `, `interface class `,
  `mixin class `; otherwise requires a bare `class ` prefix. Takes the first
  whitespace token, rejects names starting with `_` or equal to `{`, then runs
  `clean_identifier` (drops anything from `<` onward and trims a trailing `{`).
  Note: `mixin class Foo` is captured as a `class` (never a `mixin`) because the
  class branch runs first and `mixin class ` is in its modifier list.

- `enum` — `extract_enum_name`: requires `enum ` prefix, first token, rejects
  `_`-prefixed, `clean_identifier`.

- `extension` — `extract_extension_name`: requires `extension ` prefix, first
  token, rejects the token `on` (so unnamed `extension on String {}` is skipped)
  and `_`-prefixed. IMPORTANT: it returns the raw token WITHOUT
  `clean_identifier`, so a generic extension keeps its `<T>` in the name.

- `mixin` — `extract_mixin_name`: returns `None` for `mixin class ` (handled as
  class above); otherwise requires `mixin ` prefix, first token, rejects
  `_`-prefixed, `clean_identifier`.

- `function` — `extract_top_level_function_name`: rejects lines starting with
  any of `class `/`abstract class `/`final class `/`sealed class `/
  `base class `/`interface class `/`mixin class `/`enum `/`extension `/
  `mixin `/`import `/`export `/`part `/`library `/`typedef `/`//`/`@`, and any
  line that does not contain `(`. Rejects assignment/call statements where an
  `=` appears before the first `(` (e.g. `final x = foo()`). The name is the
  last whitespace token before the first `(`, then `clean_identifier`. Rejects
  empty/`_`-prefixed/`{`.

  Consequences: typedefs are NEVER emitted (excluded prefix); getters without
  `()` (e.g. `String get x => ...`) are NOT emitted (no `(`); top-level setters
  (`set x(T v)`) ARE emitted as `function` because they have `()` and `set ` is
  not an excluded prefix.

## Breaking definition

- `diff_ast` = `basic_diff`, which compares symbol sets by NAME ONLY
  (HashSet of names). `added` = names in new not in old; `removed` = names in
  old not in new; `modified` is NEVER populated.
- `detect_breaking_changes` returns `!diff.removed.is_empty()`. Therefore
  "breaking" == at least one previously-public symbol NAME disappeared.
  - A removal is breaking.
  - A rename counts as one removal + one addition -> breaking (removed non-empty).
  - A signature/body/parameter/generics change that KEEPS the same name produces
    NEITHER add nor remove -> NOT breaking (silent).
  - A kind change that keeps the name (e.g. `class Foo` -> `mixin Foo`) is also
    invisible to the diff (names equal) -> NOT breaking.

## Known misses / gaps (source-derived; reported, not fixed)

1. `/* ... */` block comments are not filtered. A single-line block comment
   containing a `(` such as `/* class Fake() {} */` is misparsed as a public
   `function` named `Fake`. (Only `//` and `///` are skipped.) -> FALSE POSITIVE.
2. `extension` names are not run through `clean_identifier`; generic extensions
   keep `<T>` in the symbol name (inconsistent with class/mixin/function).
3. Top-level setters are reported as `function`; getters without `()` are
   missed entirely. Neither has its own kind.
4. `typedef` declarations are never reported (explicitly excluded) — type-alias
   API surface is invisible.
5. Generated files (`*.g.dart`, `*.freezed.dart`, `*.gen.dart`) are parsed like
   any other `.dart`; there is no generated-code down-weighting or skipping.
6. Diff is name-only: signature/parameter/return-type/generic-constraint and
   kind changes that preserve the symbol name are NOT detected as breaking.
7. All indented (member) declarations are dropped, so method-level API changes
   are never observed; only top-level declarations constitute the surface.

## Per-file expectations

positive/classes.dart -> expect 7 symbols, all kind 'class'
  (User, Repository, Shape, ImmutablePoint, Engine, Drawable, MixinClass).
  `mixin class MixinClass` is reported as a CLASS, not a mixin (see rules).
positive/enums.dart -> expect 2 symbols kind 'enum' (Status, TrafficLight).
positive/extensions.dart -> expect 2 symbols kind 'extension' (StringHelpers, IntX).
positive/mixins.dart -> expect 2 symbols kind 'mixin' (Logging, Jsonable).
positive/functions.dart -> expect 5 symbols kind 'function'
  (greet, add, fetch, generic, multiLine).
positive/library_public.dart -> expect class 'Service' + function 'version';
  NO '_Internal' (private), NO 'run' (indented member), NO symbols from
  library/import lines.

negative/private.dart -> expect 0 public symbols (all declarations `_`-prefixed).
negative/members_only.dart -> expect 0 public symbols (only top-level decl is
  the private `_Box`; everything else is indented and dropped).
negative/directives.dart -> expect 0 public symbols (library/import/export/part/
  typedef are all excluded prefixes; typedefs never emitted).
negative/comments.dart -> expect 0 public symbols (every line starts with `//`
  or `///`, all skipped).

breaking/remove_function -> before {function greet, class User},
  after {class User}; 'greet' removed -> breaking=true.
breaking/remove_class -> before {class Api, class Helper, function ping},
  after {class Helper, function ping}; 'Api' removed -> breaking=true.
breaking/rename_function -> before {function compute, class Config},
  after {function computeV2, class Config}; name 'compute' removed
  (and 'computeV2' added) -> removed non-empty -> breaking=true.

edge/block_comment_false_positive.dart -> expect 2 symbols kind 'function':
  'realFn' (legitimate) AND 'FakeClass' (FALSE POSITIVE parsed out of the
  `/* ... */` block comment). Demonstrates gap #1.
edge/generic_extension.dart -> expect exactly 1 symbol kind 'extension' whose
  name is literally "Foo<T>" (generic NOT stripped; gap #2). The unnamed
  `extension on String` emits nothing (token `on` rejected).
edge/setter_top_level.dart -> expect exactly 1 symbol kind 'function' named
  'label' (the top-level setter is counted as a function; gap #3). The getter
  `String get readOnly => 'x'` has no `(` and emits nothing.
