# Python adapter fixture notes

Source of truth: `src/diff/lang/adapters/python.rs` (+ `src/diff/lang/adapters/common.rs`
for `read_lines_safe`, `basic_diff`, `calculate_hash`). Expectations below describe what
the adapter does **today**, not what is ideal.

## Extensions matched (`detect_files`)

- Only files whose extension is exactly `py` (`.extension() == "py"`).
- NOT matched: `.pyi` stub files, `.pyw`, or anything else. Confidence in
  `normalize()` is out of scope here but the matcher itself is extension-only.

## Public-symbol rules (`parse_ast` -> `python_parse(file, (3, 0))`)

The default parse uses version `(3, 0)`. The file is read line by line (5 MB cap via
`read_lines_safe`; non-UTF8 lines terminate iteration). Each line is `trim()`ed, then
tested in an `if / else if` chain by prefix. `Symbol.name` is the **entire trimmed
remainder after the keyword** — it INCLUDES the parameter list, base classes and the
trailing colon. No indentation, nesting, visibility, comment, or string awareness.

| kind              | matched trimmed line prefix | name stored (examples)                |
|-------------------|-----------------------------|---------------------------------------|
| `function`        | `def `                      | `def foo(a, b):` -> `foo(a, b):`      |
| `class`           | `class `                    | `class Dog(Animal):` -> `Dog(Animal):`|
| `async_function`  | `async def `                | `async def f(u):` -> `f(u):`          |

Order matters: `async def ...` does NOT start with `def `, so it falls through to the
`async def ` branch and is `async_function`, never `function`.

Version-gated kinds are **INACTIVE at the default `(3, 0)`** and only fire under
`parse_ast_versioned` with a high enough version:

- `match_statement` (prefix `match `) requires `ver >= (3, 10)`.
- `type_alias` (prefix `type `) requires `ver >= (3, 12)`.

These two checks are independent `if`s (outside the def/class chain) and are NOT covered
by these default-parse fixtures.

## Known misses / gaps (source-derived, not fixed)

- **No visibility filtering.** `_leading`, `__dunder__`, and `__name_mangled` defs/classes
  all match their prefix and are emitted as public symbols. (Python §8 visibility.)
- **No nesting awareness.** Lines are `trim()`med, so methods and nested functions are
  emitted as separate `function` symbols IN ADDITION to their enclosing `class`.
- **No string/docstring filtering.** A bare `def`/`class` line inside a triple-quoted
  docstring is emitted as a real symbol (false positive). Full-line `#` comments are safe
  only because `#` breaks the prefix.
- **No re-export / barrel handling.** `__all__ = [...]` and `from x import y` re-exports
  are ignored entirely (no symbol emitted). (Python §8 re-exports.)
- **No dynamic-API handling.** `def __getattr__(...)` / `def __dir__(...)` are emitted as
  ordinary `function` symbols; `setattr`-defined attributes are invisible. (§8 dynamic.)
- **Signature is part of `name`.** Because the whole signature text is stored, any
  parameter change (add / remove / reorder / default change) changes the `name` and is
  therefore treated as a removal + addition (see breaking below).

## Breaking definition (`detect_breaking_changes`)

`diff_ast` is `basic_diff`: it compares symbol **names only** (HashSet). `modified` is
always empty; `added` = names in new not in old; `removed` = names in old not in new.

`detect_breaking_changes(diff) = !diff.removed.is_empty()`.

Consequences:
- Removing a `def`/`class` -> its name disappears -> `removed` non-empty -> **breaking**.
- Renaming -> old name removed + new name added -> **breaking**.
- Signature change -> `name` text changes -> old removed + new added -> **breaking**
  (even adding an optional param with a default).
- Body-only edit with identical def/class line -> name unchanged -> **not breaking**.
- Pure addition (nothing removed) -> `removed` empty -> **not breaking**.

## Per-file expectations

### positive/
- `functions.py` -> 2 symbols, both kind `function`: `add(a, b):`, `multiply(a, b):`.
- `classes.py` -> 2 symbols, both kind `class`: `Animal:`, `Dog(Animal):`.
- `async_functions.py` -> 2 symbols, both kind `async_function`: `fetch(url):`,
  `send(url, data):`. (Must NOT be `function`.)
- `decorated.py` -> 2 `function` symbols: `log(fn):`, `greet(name):`. The `@log` line
  emits nothing.
- `mixed.py` -> 1 `class` (`Service:`) + 3 `function` (`start(self):`, `stop(self):`,
  `build():`). Methods are flagged alongside the class.

### negative/
- `comments.py` -> 0 public symbols (`# def` / `# class` lines do not match the prefix).
- `imports_and_constants.py` -> 0 public symbols. Also documents `__all__` is ignored.
- `lambdas.py` -> 0 public symbols (assignments/lambdas carry no `def`/`class` prefix).
- `prose_docstring.py` -> 0 public symbols (docstring prose has no bare keyword line;
  contrast with `edge/docstring_false_positive.py`).

### breaking/ (before -> after)
- `remove_function` -> after removes `function` `deprecated_api():` -> `removed`
  non-empty -> breaking = true.
- `rename_class` -> `class` `OldName:` gone, `NewName:` added -> `removed` non-empty ->
  breaking = true.
- `change_signature` -> `connect(host, port):` becomes `connect(host, port, timeout):`;
  `name` text changes -> old removed + new added -> breaking = true (signature is part of
  the name).

### edge/
- `nested_methods.py` -> 1 `class` (`Handler:`) + 2 `function` (`handle(self, event):`,
  `validate(self, event):`). Demonstrates indented methods are emitted as separate
  symbols (no nesting awareness).
- `docstring_false_positive.py` -> >=1 `function`: `documented_but_not_real():`. This is a
  FALSE POSITIVE — the line lives inside a docstring, but string content is not filtered.
- `private_and_dunder.py` -> 1 `class` (`_Internal:`) + `function`s `__init__(self):`,
  `_helper():`, `__mangled():`. All flagged public despite underscore / dunder /
  name-mangling (no visibility filtering).
