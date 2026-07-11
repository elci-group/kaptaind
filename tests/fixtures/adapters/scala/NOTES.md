# Scala adapter fixture notes

Source: `src/diff/lang/adapters/scala.rs` (read in full). Shared helpers in
`src/diff/lang/adapters/common.rs` (`read_lines_safe`, `basic_diff`,
`calculate_hash`). All expectations below are derived strictly from what the
source does today, not from ideal Scala semantics.

## Extensions matched (`detect_files`)

Only `*.scala` and `*.sc` (case-sensitive, via `Path::extension`). Anything else
(`.java`, `.sbt`, `.sc~`, no extension) is ignored.

## Public-symbol rules (`parse_ast`)

Line-by-line scanner, max 5 MB (`read_lines_safe`). For each physical line:

1. `trim`; skip empty lines and lines starting with `//`.
2. Strip a single-line `/* ... */` fragment if both delimiters are on the line.
3. Skip the line if it begins with a non-public access modifier: `private[`,
   `protected[`, `private`, `protected` (matched only when followed by EOL,
   whitespace, or `[`).
4. Strip leading whole-word modifiers, looping: `abstract final sealed lazy
   implicit inline opaque transparent open override`.
5. Match the FIRST of these prefixes (order matters) and emit a `Symbol`:

| Prefix matched | `kind`        | Name extraction (`extract_identifier`)                          |
|----------------|---------------|-----------------------------------------------------------------|
| `case class `  | `case_class`  | first token up to whitespace / `[` `(` `{` `:` `=`              |
| `class `       | `class`       | same                                                            |
| `object `      | `object`      | same                                                            |
| `trait `       | `trait`       | same                                                            |
| `def `         | `def`         | same                                                            |

`case class` is tested before `class`, so case classes get kind `case_class`.
Generics are fine because the name stops at `[`/`(` (e.g. `class Box[T]`,
`def map[A, B]`, `trait Functor[F[_]]` -> `Box`/`map`/`Functor`).

## Known misses (source emits 0 symbols for these public constructs)

- `case object` (ADT leaf): no `case object` rule and `case` is not a stripped
  modifier -> the line starts with `case object` and matches nothing.
- Public `val` / `var` members: no `val`/`var` prefix handled.
- `type` aliases: no `type` prefix handled.
- A declaration placed on the closing line of a multi-line block comment
  (`*/ class Foo`) is missed (only single-line `/* */` is stripped).

## Known over-detection (false public positives)

- No brace/scope tracking: a `def`/`class`/`object`/`trait` physically nested
  inside a method body is still flagged as public.
- No string tracking: a keyword line inside a triple-quoted string is flagged.

## Breaking definition (`detect_breaking_changes` + `basic_diff`)

`basic_diff` keys on `name` only (NOT `kind`, NOT signature):
`removed = old.names \ new.names`, `added = new.names \ old.names`,
`modified` is always empty. `detect_breaking_changes` returns
`!diff.removed.is_empty()` — i.e. **removal-only and name-based**.

Consequences:
- Removing/renaming a public symbol -> breaking = true.
- Changing a `def` signature/body while keeping the same name -> NOT breaking.
- Changing kind (e.g. `class` -> `object`) while keeping the same name ->
  NOT breaking (name unchanged, so neither removed nor added).

## Per-file expectations

### positive/
- `positive/classes.scala` -> 5 symbols, all kind `class`: Service,
  FinalService, Status, Box, Base. (`sealed abstract`/`final`/`abstract` stripped;
  `Box[T]` -> `Box`.)
- `positive/case_classes.scala` -> 3 symbols, all kind `case_class`: User,
  Point, Id. (Confirms `case class` wins over plain `class`; `final` stripped.)
- `positive/objects.scala` -> 3 symbols: Config (`object`), Math (`object`),
  pi (`def`). (`val defaultPort` NOT detected.)
- `positive/traits.scala` -> 5 symbols: Logger (`trait`), log (`def`),
  Shape (`trait`), Functor (`trait`), map (`def`). (`F[_]`/`[A, B]` -> name ok.)
- `positive/defs.scala` -> 4 symbols: greet (`def`), identity (`def`),
  Impl (`class`), log (`def`). (`override` stripped.)
- `positive/modifiers.scala` -> 5 symbols: Animal (`class`), Living (`class`),
  Util (`object`), OpenBase (`class`), twice (`def`).

### negative/  (all expect exactly 0 public symbols)
- `negative/private_members.scala` -> 0 (public container made `private`;
  inner `private`/`protected` members skipped).
- `negative/scoped_access.scala` -> 0 (`private[this]`/`private[example]`/
  `protected[this]`/`protected[example]` all skipped via the two-step prefix
  check: `private[`/`protected[` fail the whole-word test, then bare
  `private`/`protected` + `[` matches).
- `negative/comments.scala` -> 0 (`//` lines skipped; single-line `/* */`
  stripped to empty).
- `negative/strings.scala` -> 0 (keywords live inside `val` string literals;
  lines start with `val`, never a decl keyword).

### breaking/  (before/after pairs; all true-breaking by adapter rules)
- `breaking/remove_method` -> before {Math, add, sub}, after {Math, add};
  `sub` removed -> breaking = true.
- `breaking/remove_class` -> before {Service, Repo}, after {Repo};
  `Service` removed -> breaking = true.
- `breaking/rename_case_class` -> before {User, Order}, after {UserV2, Order};
  `User` removed (rename == remove+add) -> breaking = true.

### edge/
- `edge/case_object.scala` -> 1 symbol: Status (`trait`). Active/Inactive
  (`case object`) NOT detected (known miss of the ADT-leaf idiom).
- `edge/no_scope_tracking.scala` -> 4 symbols: Demo (`object`), outer (`def`),
  inner (`def`, FALSE POSITIVE: local def inside method), Phantom (`class`,
  FALSE POSITIVE: decl inside triple-quoted string).
- `edge/public_val_type.scala` -> 1 symbol: Api (`object`). version (`val`),
  counter (`var`), UserId (`type`) NOT detected (known misses).
