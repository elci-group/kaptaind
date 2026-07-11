# kotlin `modified` fixture notes

The Kotlin adapter (`src/diff/lang/adapters/kotlin.rs`) parses line-by-line. For each
matched declaration, the emitted `name` is the ENTIRE remainder of the trimmed line
after the matched keyword prefix (`strip_prefix("<kw> ")`), and `kind` is a fixed string
per keyword. So a same-name/different-kind pair is produced by holding the post-keyword
suffix byte-identical while swapping only the leading keyword.

Detected extensions: `.kt`, `.kts` (these fixtures use `.kt`).
Private/protected/internal-prefixed lines are skipped; all fixtures use default (public)
visibility. The `package` line and `}`/`comment` lines match no prefix and emit no symbol,
so each pair emits exactly one symbol per side (clean single-`modified` diffs).

Kind strings emitted by the adapter (copied from source): `"function"`, `"class"`,
`"data_class"`, `"sealed_class"`, `"object"`, `"interface"`, `"enum"`, `"typealias"`,
`"property"`, `"suspend_function"`, `"annotation"`, `"jvm_export"`, `"composable"`.

## Pair 1 — `class_to_interface`
- NAME held constant: `Repository {` (line `class Repository {` vs `interface Repository {`)
- old_kind -> new_kind: `class` -> `interface`
- Breaking-policy hint: **yes** — call sites that instantiate (`Repository()`) or subclass it
  as a class break; an interface cannot be constructed and is binary/source incompatible.
- Kind strings relied on: `"class"`, `"interface"`.
- Uncertainty: low. Empty-body `class X {}` and `interface X {}` are valid Kotlin; the parser
  only inspects the declaration line, so the brace-bearing suffix `Repository {` is identical
  and only the leading keyword changes. Cannot run the parser to confirm (per task rules).

## Pair 2 — `data_class_to_class`
- NAME held constant: `User(val id: Long, val name: String)`
- old_kind -> new_kind: `data_class` -> `class`
- Breaking-policy hint: **yes** — dropping `data` removes generated `copy()`, `componentN()`,
  `equals/hashCode/toString`; destructuring and `copy(...)` call sites fail to compile.
- Kind strings relied on: `"data_class"`, `"class"`.
- Uncertainty: low. Note the adapter checks `class ` BEFORE `data class ` in its if/else chain,
  but a `data class ...` line does NOT start with `class ` (it starts with `data class `), so it
  correctly falls through to the `data class ` branch; conversely `class User(...)` matches the
  bare `class ` branch and stops. Both sides valid (single-line, no body needed). Cannot run
  the parser to confirm (per task rules).

## Pair 3 — `function_to_suspend`
- NAME held constant: `fetch(url: String): String = url`
- old_kind -> new_kind: `function` -> `suspend_function`
- Breaking-policy hint: **yes** — adding `suspend` changes the calling convention; existing
  non-suspend callers cannot invoke it without a coroutine/suspend context (source/binary
  incompatible).
- Kind strings relied on: `"function"`, `"suspend_function"`.
- Uncertainty: low. Same ordering subtlety as Pair 2: `fun ` is checked first, but a
  `suspend fun ...` line starts with `suspend fun ` (not `fun `), so it falls through to the
  `suspend fun ` branch; a plain `fun ...` matches `fun ` and stops. Expression body keeps both
  sides valid. Cannot run the parser to confirm (per task rules).

## Pair 4 — `control` (no modified symbol expected)
- NAME held constant: `Service {` (line `class Service {` identical on both sides)
- old_kind -> new_kind: `class` -> `class` (**same_kind (control)**)
- Breaking-policy hint: **no** — same declaration/kind; only a `// no-op marker` comment was
  added inside the body, which the parser ignores entirely.
- Kind strings relied on: `"class"` (both sides).
- Uncertainty: low. The only changed line is a comment, which matches no prefix, so the emitted
  symbol set is identical (one symbol: name `Service {`, kind `class`); the diff engine flags a
  symbol as `modified` only on same-name/different-kind, so this must yield ZERO modified
  symbols. Cannot run the parser to confirm (per task rules).
