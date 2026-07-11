# Python adapter — `modified` kind-change fixtures

The diff engine flags a symbol as `modified` when its **name** is unchanged but
its **kind** changes (same name, different kind).

Adapter facts (from `src/diff/lang/adapters/python.rs`):

- Detected extension: `.py` (`detect_files` keeps `extension() == "py"`).
- `name` = the entire rest of the line after the matched prefix keyword
  (`rest.to_string()`). It therefore includes the full signature and trailing
  colon, e.g. `def fetch(url: str) -> bytes:` -> name `fetch(url: str) -> bytes:`.
- Prefix check order is `def `, then `class `, then `async def `. (`async def`
  lines never start with `def `, so they fall through to the `async def ` arm.)
- `kind` strings emitted (copied verbatim): `"function"`, `"class"`,
  `"async_function"`, `"match_statement"`, `"type_alias"`.
- Only the first three are reachable here: the default `parse_ast` calls
  `python_parse(file, (3, 0))`, so the `match_statement` (>= 3.10) and
  `type_alias` (>= 3.12) arms are never entered for a normal diff. These
  fixtures therefore use only `function` / `class` / `async_function`.

## Pairs

### 1. function_to_async
- NAME held constant: `fetch(url: str) -> bytes:`
- `old_kind -> new_kind`: `function -> async_function`
- kind strings relied on: `"function"` (`def `) -> `"async_function"` (`async def `)
- BREAKING-POLICY HINT: **yes** — a sync `def` returning `bytes` becomes a
  coroutine; every caller must now `await` it, and the return type effectively
  changes from `bytes` to `Coroutine[..., bytes]`.
- UNCERTAINTY: low. `rest` after the keyword is byte-identical, so names match;
  cannot run the parser to confirm `basic_diff` keys solely on `name`.

### 2. class_to_function
- NAME held constant: `Widget():`
- `old_kind -> new_kind`: `class -> function`
- kind strings relied on: `"class"` (`class `) -> `"function"` (`def `)
- Note: `class Widget():` (empty base list) is valid Python and is what keeps
  the rest-of-line byte-identical to `def Widget():`.
- BREAKING-POLICY HINT: **yes** — `Widget()` went from constructing a `Widget`
  instance (subclassable, usable with `isinstance`) to a plain function
  returning `None`; instantiation, subclassing, and type checks all change.
- UNCERTAINTY: low-medium. Empty `()` on a class is unusual but legal; name
  bytes match exactly. Same caveat about not running the parser.

### 3. async_to_class
- NAME held constant: `reset():`
- `old_kind -> new_kind`: `async_function -> class`
- kind strings relied on: `"async_function"` (`async def `) -> `"class"` (`class `)
- BREAKING-POLICY HINT: **yes** — `await reset()` (coroutine) becomes
  `reset()` constructing an instance; await-based call sites break and the
  callable's semantics change from async to synchronous instantiation.
- UNCERTAINTY: medium. A lowercase class name (`class reset():`) is valid
  syntax but unconventional; chosen purely to keep the rest-of-line
  byte-identical to the `async def` form. Cannot confirm parser behavior.

### 4. control (negative)
- NAME held constant: `helper(x: int) -> int:`
- `same_kind (control)`: `function` on both sides
- kind strings relied on: `"function"` (`def `) both before and after
- Only the body changed (`return x + 1` -> `return x + 2`); declaration line
  is byte-identical, so name and kind are unchanged -> must yield NO modified
  symbol (guards against over-firing).
- BREAKING-POLICY HINT: **no** — no kind change; behavior tweak only.
- UNCERTAINTY: low. Body lines (`return ...`) match no prefix, so they emit no
  extra symbols and cannot introduce a spurious kind change.
