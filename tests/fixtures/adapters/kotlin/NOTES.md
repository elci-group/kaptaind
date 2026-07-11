# Kotlin adapter fixture notes

Source of truth: `src/diff/lang/adapters/kotlin.rs` (+ `src/diff/lang/adapters/common.rs`).
All expectations below are derived strictly from what the source does today, not from
ideal Kotlin semantics.

## File extensions (`detect_files`)

Matches files whose extension is exactly `kt` or `kts`. Anything else (`.java`, `.kts`
with extra suffix, extensionless) is ignored.

## Visibility / filtering in `parse_ast`

- Operates line-by-line on `line.trim()`.
- Skips a line (emits nothing for it) if it starts with `private `, `protected `, or
  `internal `. Everything else is treated as public (Kotlin's default) — but note this is
  a naive prefix test on the trimmed line (see known misses).
- There is NO awareness of comments, strings, nesting/scope, or modifiers other than the
  three skip-prefixes above.

## Public-symbol rules by `kind` (prefix matched, in source order)

The adapter uses `strip_prefix` in this order; first match wins and `name` is the entire
remainder of the trimmed line after the prefix (signature text included, no truncation):

| kind              | prefix (`strip_prefix`) | notes |
|-------------------|-------------------------|-------|
| `function`        | `fun `                  | does NOT also fire for `suspend fun ` (that starts with `suspend `) |
| `class`           | `class `                | plain classes only; the variants below do not start with `class ` so they reach their own branch |
| `data_class`      | `data class `           | |
| `sealed_class`    | `sealed class `         | |
| `object`          | `object `               | `companion object` does NOT match (starts with `companion `) |
| `interface`       | `interface `            | |
| `enum`            | `enum class `           | |
| `typealias`       | `typealias `            | |
| `property`        | `val ` or `var `        | both map to kind `property` |
| `suspend_function`| `suspend fun `          | listed after `fun ` but a `suspend fun` line never starts with `fun `, so it lands here only |
| `annotation`      | `annotation class `     | |

After the prefix chain, two annotation-line detectors run independently and can ADD extra
symbols on top of whatever the prefix chain emitted on the same or neighboring lines:

| kind         | trigger (line starts with)     |
|--------------|--------------------------------|
| `jvm_export` | `@JvmStatic` or `@JvmField`    |
| `composable` | `@Composable`                  |

Because these are separate lines from the declaration they annotate, a `@Composable` +
`fun` pair emits TWO symbols: one `composable` (name = the annotation line) and one
`function` (name = the fun remainder). Same for `@JvmStatic`/`@JvmField`.

## Breaking-change definition

`diff_ast` delegates to `basic_diff` (common.rs), which compares symbols by **name only**
(HashSet of `name` strings); `modified` is always empty. `detect_breaking_changes` returns
`!diff.removed.is_empty()`.

Consequence: breaking == "any previously-present public symbol whose exact name-string is
no longer present." Because `name` embeds the full signature text, a signature change
(e.g. adding a parameter) changes the name-string and therefore appears as a removal of
the old name (+ an addition of the new one) -> breaking=true. A body-only edit that keeps
the declaration line byte-identical is NOT breaking.

## Known misses / gaps (reported, not fixed)

- **No modifier handling.** Declarations carrying leading modifiers — `public fun`,
  `abstract class`, `open class`, `inline fun`, `final class`, `override fun`,
  `companion object` — do not start with the bare keyword, so the adapter emits NOTHING
  for them even when they are semantically public. (See `edge/modifiers.kt`.)
- **No string awareness.** Content inside a multiline raw string (`""" ... """`) that
  lands on its own line and starts with `fun `/`class `/etc. is parsed as a real symbol
  (false positive). (See `edge/multiline_string.kt`.)
- **No scope awareness.** Member `fun`/`val`/`var` inside a class body are flagged exactly
  like top-level declarations; there is no notion of "member vs top-level public API".
- **No comment awareness by design but safe by accident** for `//`, `/*`, `*`, KDoc lines
  because after `trim()` they start with `/` or `*`, never a keyword.
- **No generated-file handling.** A `*.generated.kt` / codegen file with public decls is
  treated as real API surface.

## Per-file expectations

### positive/
- `positive/functions.kt` -> 3 symbols: 2x `function` (`greet(...)`, `add(...)`),
  1x `suspend_function` (`loadUser(...)`). No separate `function` for the suspend line.
- `positive/classes.kt` -> `class` (`ApiClient...`), `data_class` (`User...`),
  `sealed_class` (`Result...`), `enum` (`Color...`). PLUS member `fun call()` inside
  `ApiClient` is ALSO flagged as `function` (no scope awareness), and the nested
  `data class Ok`/`data class Err` lines are ALSO flagged as `data_class`. Expect
  >= 4 with at least one each of `class`, `data_class`, `sealed_class`, `enum`, `function`.
- `positive/objects_interfaces.kt` -> `interface` (`Repository...`), `object`
  (`Config...`), `annotation` (`Serializable...`); member `fun fetch` inside Repository
  also flagged as `function`. Expect >= 3 incl. `interface`, `object`, `annotation`.
- `positive/properties.kt` -> 3x `property` (`appName`, `retryCount`, `MAX_RETRIES` via
  `const val` -> starts with `const ` NOT `val `, so `MAX_RETRIES` is a MISS; only
  `appName` and `retryCount` match `val `/`var `) and 1x `typealias` (`UserId`).
  Correction: `const val MAX_RETRIES` starts with `const `, so it is NOT detected.
  Expect exactly 3 symbols: 2x `property`, 1x `typealias`.
- `positive/compose.kt` -> `composable` (`@Composable` line) + `function` (`Greeting`),
  plus `object` (`Bridge`), `jvm_export` (`@JvmStatic`) + `function` (`fromJava`),
  `jvm_export` (`@JvmField`) + `property` (`DEFAULT`). Expect 7 symbols total incl. at
  least one `composable`, two `jvm_export`, and one `object` (for `Bridge`).

### negative/
- `negative/private_members.kt` -> 0 symbols (every decl line starts with
  `private `/`protected `/`internal ` and is skipped).
- `negative/comments.kt` -> 0 symbols (after trim, every keyword-looking line starts with
  `//`, `/*`, or `*`; none match a prefix).
- `negative/build_script.kts` -> 0 symbols (DSL lines like `plugins {`, `dependencies {`
  start with none of the prefixes; `.kts` IS matched by `detect_files` but yields no
  public symbols).

### breaking/ (each is a before/after pair; breaking = removed set non-empty)
- `breaking/remove_function` -> after removes `fun farewell(...)`; its name-string
  disappears -> removed non-empty -> breaking=true.
- `breaking/change_signature` -> `greet(name: String)` becomes
  `greet(name: String, prefix: String)`; the old name-string is gone (and a new one
  added) -> removed non-empty -> breaking=true (signature changes count as breaking
  because `name` embeds the signature).
- `breaking/remove_class` -> after removes `data class Config(...)`; name-string gone ->
  removed non-empty -> breaking=true.

### edge/
- `edge/modifiers.kt` -> 0 symbols per current source. `public fun`, `abstract class`,
  `open class`, `inline fun` all start with a modifier, not the bare keyword, so none are
  detected. Documents the modifier-prefixed-public-declaration miss.
- `edge/multiline_string.kt` -> the adapter emits: `property` for `val template = ...`,
  then FALSE-POSITIVE `function` for `fun ghost() {}` and FALSE-POSITIVE `class` for
  `class Phantom {}` (indented lines trim to start with the keyword), plus `function` for
  the real `fun real()`. Expect >= 4 symbols, 2 of which are string-content false
  positives -> documents the no-string-awareness gap.
- `edge/generics.kt` -> `function` (`identity`, name includes `<T>`), `function`
  (`maxOf`, name includes `<T : Comparable<T>>`), `class` (`Box<T>...`), `interface`
  (`Mapper<In, Out>...`), plus member `fun map` flagged as `function`. Generics ARE
  counted; because generic parameters are part of `name`, tightening a generic bound
  changes the name-string and would read as a removal (breaking). Expect >= 4 incl.
  `function`, `class`, `interface`.
