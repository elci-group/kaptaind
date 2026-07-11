# Clojure adapter — `modified` kind-change fixtures

The shared `modified` diff signal fires when a symbol keeps the SAME `name` but
its `kind` changes. This directory holds same-name / different-kind before/after
pairs plus one same-kind control.

## Adapter facts (from `src/diff/lang/adapters/clojure.rs`)

- Detected extensions: `clj`, `cljs`, `cljc` (these fixtures use `.clj`).
- `kind` strings emitted (copied verbatim): `"defn"`, `"defmacro"`,
  `"defprotocol"`, `"def"`.
- Name extraction: `parse_symbol_name` takes the FIRST whitespace-separated token
  after the declaration prefix (e.g. the token right after `(defn `) and trims
  trailing `()[]{}`. That token is held byte-identical across each pair.
- Prefix check order is `(defn ` → `(defmacro ` → `(defprotocol ` → `(def `, so a
  line like `(defn- ...)` does NOT match `(defn ` (private fns are not emitted).

## Pairs

### 1. `defn_to_defmacro`
- NAME held constant: `greet`
- kind transition: `defn` -> `defmacro`
- breaking-policy hint: **yes** — a function becomes a macro; callers that pass
  the value as a function, map over it, or rely on runtime evaluation break, and
  arguments are no longer evaluated at the call site.
- kind strings relied on: `"defn"`, `"defmacro"`
- uncertainty: low. The `(defmacro greet ...)` line matches the `(defmacro `
  prefix; the syntax-quoted body `` `(str ... ~name) `` does not start with any
  declaration prefix, so only one symbol is emitted per side.

### 2. `defprotocol_to_def`
- NAME held constant: `Greeter`
- kind transition: `defprotocol` -> `def`
- breaking-policy hint: **yes** — a protocol (type + polymorphic dispatch) is
  replaced by a plain var holding a map; `extend`/`satisfies?` consumers and
  anything relying on protocol dispatch stop working.
- kind strings relied on: `"defprotocol"`, `"def"`
- uncertainty: low. Before body line `(greet [this])` matches no prefix. After
  body uses `(fn ...)`, which does NOT start with `(defn `, so no stray symbol is
  emitted; only `Greeter` appears on each side.

### 3. `def_to_defn`
- NAME held constant: `answer`
- kind transition: `def` -> `defn`
- breaking-policy hint: **yes** — a value var becomes a zero-arg function;
  consumers must change call-site form from `answer` to `(answer)`.
- kind strings relied on: `"def"`, `"defn"`
- uncertainty: low. Single top-level form each side; no nested declarations.

### 4. `control`
- NAME held constant: `add`
- kind transition: `same_kind (control)` — `defn` -> `defn`
- breaking-policy hint: **no** — identical name and kind; only the body changes
  (`(+ a b)` -> `(+ a b 0)`), which is invisible to the name/kind-only diff and
  must NOT produce a `modified` symbol.
- kind strings relied on: `"defn"`
- uncertainty: low. Body changes do not affect `name` or `kind`; this guards
  against over-firing of the `modified` signal.

## General note

None of these files were run through the parser (cargo/git/formatter were
intentionally not invoked). Expectations above are derived from reading the
adapter source and mirroring the proven `tests/fixtures/adapters/clojure/positive/`
files, swapping only the kind-bearing keyword while holding the name token fixed.
