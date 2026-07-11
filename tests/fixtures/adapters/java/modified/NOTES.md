# Java adapter — `modified` kind-change fixtures

Shared diff signal under test: a symbol is flagged `modified` when its
`name` is unchanged but its `kind` changes between the before/after parse.

Adapter facts relied on (from `src/diff/lang/adapters/java.rs`):
- `detect_files` matches extension `.java`.
- Type name extraction: `extract_type_name` takes the first token after
  `public class ` / `public interface ` / `public enum `, splitting on
  `{`, ` `, or `<` (`rest.split(['{', ' ', '<']).next()`). The held-constant
  NAME below is that first token and is byte-identical across each pair.
- Method lines (`is_public_method_line`) require `public ` prefix, no
  ` class `/` interface `/` enum ` substring, and a `(`. In the kind-change
  pairs the method line is kept byte-identical so it stays
  `method -> method` and does NOT contribute a spurious modified signal.

Kind strings copied verbatim from the adapter: `"class"`, `"interface"`,
`"enum"`, `"method"`.

## Pairs

### 1. `class_to_interface` — NAME `Service`
- `old_kind -> new_kind`: `class -> interface`
- kind strings used: `"class"`, `"interface"`
- breaking-policy hint: `yes` — in Java, changing a `class` to an
  `interface` breaks every `new Service()` instantiation, any
  `extends Service`, and any concrete member access; consumers must
  `implements` instead and cannot construct it.
- uncertainty: low. The line swaps only the keyword `class`/`interface`
  before the identical first token `Service`, so the adapter's prefix
  branches (`strip_prefix("public class ")` vs `strip_prefix("public interface ")`)
  clearly emit different kinds for the same name.

### 2. `interface_to_enum` — NAME `Status`
- `old_kind -> new_kind`: `interface -> enum`
- kind strings used: `"interface"`, `"enum"`
- breaking-policy hint: `yes` — switching an `interface` to an `enum`
  removes the ability to `implements` it and to define anonymous/proxy
  implementations; call sites depending on subtyping or reflection over
  implementors break.
- uncertainty: low-medium. The `enum` after body adds constants and a
  method, but the declaration line is `public enum Status` with `Status`
  as the first token, distinct from `public interface Status`. The method
  `label` line is byte-identical (`public String label()`) so it stays
  `method -> method`. Slight residual: enum constants are not emitted as
  symbols by this adapter, so they cannot accidentally add a same-name
  symbol — confirmed by inspection of the parser (it only emits types and
  `public` method lines).

### 3. `enum_to_class` — NAME `Level`
- `old_kind -> new_kind`: `enum -> class`
- kind strings used: `"enum"`, `"class"`
- breaking-policy hint: `yes` — changing an `enum` to a `class` drops the
  fixed constant set (`values()`, `valueOf`, `switch` exhaustiveness) and
  makes it instantiable/inheritable; any `switch` over the enum or use of
  `Level.LOW`-style constants breaks.
- uncertainty: low. Same-token `Level`, only the `enum`/`class` keyword
  changes; both branches extract `Level` via the same `extract_type_name`
  split. The method `rank` line is byte-identical in both files.

### 4. `control` — NAME `Repo` (control)
- `same_kind (control)`: `class -> class` (no kind change)
- kind strings used: `"class"` (also emits a `method` symbol `save`,
  identical on both sides)
- breaking-policy hint: `no` — only an in-body comment changed; the
  public API surface (name + kind set) is identical, so no `modified`,
  `added`, or `removed` symbol should fire.
- uncertainty: low. The only textual delta is an added `//` comment line
  inside a method body; the parser skips `//` lines and the `Repo` class
  line plus `public void save()` method line are byte-identical. The
  `private void persist()` line is never emitted (not `public`), and is
  unchanged anyway.
