# Java adapter fixture corpus — detection rules (source-derived)

Source: `src/diff/lang/adapters/java.rs` (helpers in `src/diff/lang/adapters/common.rs`).
All expectations below describe what the adapter does **today**, line-for-line — not
what it ideally should do.

## 1. `detect_files` — extensions matched

- Matches a path iff `Path::extension()` equals the literal `java` (lowercase).
- Comparison is case-sensitive: `Foo.JAVA` is **not** matched.
- Only the single extension `java`; no shebang/filename rules; `.class`/`.jar` ignored.

## 2. `parse_ast` — public-symbol rules (per trimmed source line, in order)

The file is read with `read_lines_safe` (files > 5 MiB are skipped; bad lines dropped).
Each line is `trim()`ed, then matched by this exact `if / else if` chain (first match wins):

| kind        | rule (on the trimmed line)                                              | name extraction |
|-------------|-------------------------------------------------------------------------|-----------------|
| `class`     | `strip_prefix("public class ")` succeeds                                | `extract_type_name` |
| `interface` | `strip_prefix("public interface ")` succeeds                            | `extract_type_name` |
| `enum`      | `strip_prefix("public enum ")` succeeds                                 | `extract_type_name` |
| `method`    | `is_public_method_line`: starts with `public `, contains `(`, and contains **none** of ` class ` / ` interface ` / ` enum ` | `extract_method_name` |

- `extract_type_name(rest)` = first token of `rest` split on `{`, space, or `<`.
  `Foo<T> {` → `Foo`; `Foo implements Bar {` → `Foo`.
- `extract_method_name(line)` = last whitespace token before the first `(`.
  `public static void main(String[] a)` → `main`; `public List<String> list()` → `list`.

### What is deliberately / incidentally NOT flagged public

- Anything not starting (after trim) with `public ` → ignored: `private`,
  `protected`, package-private (`void foo()`), `class`/`interface` without `public`.
- Public **fields** (`public int x = 0;`) — no `(`, so not a method → ignored.
- Comments and string literals — a `public ...` token only matches at the **start of the
  trimmed line**, so `// public class X`, block comments, javadoc (`* public class X`),
  and `"public class X"` inside a string do not match.
- Interface method *declarations* without the `public` keyword (`void greet(String n);`)
  are not flagged; only the `interface` symbol itself is emitted.
- Constructors (`public Foo(int x)`) **are** flagged — they satisfy the method rule and
  are emitted as `kind = "method"` with the class name as the symbol name.

## 3. Known misses / gaps (per current source — reported, not fixed)

- **Modifier ordering.** The prefixes are literal (`public class `, etc.). Reordered
  modifiers are missed: `public abstract class`, `public final class`,
  `public sealed interface` → 0 symbols (see `edge/modifier_order.java`).
- **Multiline signatures.** Detection is per-line. A method whose name and `(` are on a
  different line than the `public` keyword is missed (see `edge/multiline_method.java`).
- **Constructors reported as methods.** `public Foo(...)` is emitted as `kind="method"`.
- **No generated-code heuristic.** Files produced by annotation processors / codegen are
  parsed identically to hand-written code (see `edge/generated.java`).
- **Case-sensitive extension.** `*.JAVA` is not detected.
- `basic_diff` keys on **name only**; `modified` is never populated.

## 4. `detect_breaking_changes` — definition

`!diff.removed.is_empty()`, where `diff` = `basic_diff(old, new)`.

- Breaking iff at least one previously-public symbol **name** is absent in the new set.
- Purely removal-based. A **rename** is treated as a removal of the old name (+ add of
  the new) → breaking = `true`.
- **Not** breaking: signature/parameter changes, return-type changes, body changes,
  kind changes that keep the same name, and pure additions — the name is unchanged so it
  is neither `removed` nor `added`.

## 5. Per-file expectations

### positive/
- `positive/classes.java` -> >=1 `class` (`Service`) and methods `greet`, `add` (3 symbols).
- `positive/interface.java` -> 1 `interface` (`Greeter`); the `void greet`/`String defaultMessage` decls (no `public`) are **not** methods (1 symbol).
- `positive/enum.java` -> 1 `enum` (`Status`) (1 symbol).
- `positive/methods.java` -> 1 `class` (`Repo`) + methods `main`, `list`, `save` (4 symbols; `main`/`save` show `static` and `throws` still match).
- `positive/generics.java` -> 1 `class` (`Box`, `<T>` stripped) + methods `echo`, `get` (3 symbols).

### negative/  (all -> expect 0 public symbols)
- `negative/private.java` -> package-private `class Internal` + private/protected/package methods -> 0.
- `negative/comments.java` -> all `public ...` are inside `//`, `/* */`, javadoc, or a string literal; real `class Holder` is package-private -> 0.
- `negative/package_private.java` -> package-private `class Helper` and methods -> 0.
- `negative/fields_only.java` -> package-private `class Config`; its `public` fields have no `(` so they are not methods -> 0.

### breaking/  (each pair -> breaking = true)
- `breaking/remove_method` -> `after` drops public method `greet` -> name removed -> breaking=true.
- `breaking/remove_class` -> `after` removes the whole public `Widget` type -> `Widget`/`render` removed -> breaking=true.
- `breaking/rename_method` -> `compute` renamed to `computeV2` -> `compute` name removed -> breaking=true (rename-as-removal).

### edge/
- `edge/modifier_order.java` -> `public abstract class` / `public final class` / `public sealed interface` match no literal prefix -> expect 0 symbols (modifier-order miss).
- `edge/generated.java` -> codegen header is ignored; `GeneratedModel` + `getId`/`setId` are parsed normally -> expect 3 symbols (no generated down-weight).
- `edge/multiline_method.java` -> `class Formatter` and method `size` detected; the `format` method (name/`(` split from `public`) is **not** detected -> expect 2 symbols, no `format`.
