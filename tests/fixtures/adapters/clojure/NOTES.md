# Clojure adapter — fixture expectations

Source of truth: `src/diff/lang/adapters/clojure.rs`. All expectations below are
derived strictly from what that file does today, NOT from ideal Clojure semantics.
Where the two diverge, the divergence is called out under "Known misses / gaps".

## Extensions matched (`detect_files`)

Case-sensitive match on the path extension: `clj`, `cljs`, `cljc` only.
Anything else (`.cljd`, `.CLJ`, `.edn`, `.bb`, `.cljx`, no extension) is NOT
matched. `parse_ast` itself is extension-agnostic; the extension gate lives
entirely in `detect_files`.

## Public-symbol rules (`parse_ast` → kinds)

The scanner is line-based. For each line it `trim()`s, skips the line if the
trimmed form starts with `;` (comment), then checks these prefixes IN ORDER and
emits at most one symbol per line:

| Prefix        | kind          | Name token                                          |
|---------------|---------------|-----------------------------------------------------|
| `(defn `      | `defn`        | first whitespace token after the prefix             |
| `(defmacro `  | `defmacro`    | first whitespace token after the prefix             |
| `(defprotocol`| `defprotocol` | first whitespace token after the prefix             |
| `(def `       | `def`         | first whitespace token after the prefix             |

Name extraction (`parse_symbol_name`): takes the first whitespace-delimited
token after the prefix, then trims leading/trailing `( ) [ ] { }` ONLY. An empty
result is dropped. The trailing space in each prefix is mandatory — `(defn-`,
`(defonce`, `(defmulti`, etc. do NOT match `(defn ` / `(def ` because the next
char is not a space.

`extract_api` returns every parsed symbol as `public_symbols` (no further
filtering), so "public" == "whatever `parse_ast` emitted".

What is deliberately or accidentally ignored:
- `;`-prefixed comment lines (after trim).
- `(defn- …)` private functions — the `(defn ` prefix (with space) does not
  match `(defn-`, and `(def ` does not match either, so they fall through.
- Everything not in the four prefixes: `ns`, `defonce`, `defmulti`,
  `defmethod`, `deftype`, `defrecord`, `definterface`, protocol METHOD lines
  (`(greet [this])`), nested/body forms, string literals, data literals.

## Breaking definition

`diff_ast` = `basic_diff`, which compares symbol sets BY NAME ONLY (HashSet of
names): `added` = names new in `new`, `removed` = names gone from `old`,
`modified` is ALWAYS empty. `detect_breaking_changes` = `!diff.removed.is_empty()`.

Therefore breaking == "a previously-known public symbol NAME is no longer
present." Purely removal-based. A rename is one removal + one addition →
breaking (driven by the removal). Signature/arity/body/kind changes that keep
the same name produce NO removal → NOT breaking.

## Known misses / gaps (suspected bugs — reported, not fixed)

1. **`^:private` metadata not honored (false public + wrong name).**
   `(defn ^:private hidden [x] …)` → first token after `(defn ` is `^:private`,
   which is emitted as a `defn` literally named `^:private`. The adapter only
   recognizes `defn-`, not metadata privacy. See `edge/private_metadata.clj`.
2. **`(comment …)` blocks are not understood (false public).** The scanner is
   line-based and only skips `;` lines, so a `(defn …)` on its own line inside a
   `(comment …)` form IS emitted. See `edge/comment_block.clj`.
3. **Signature/arity/body/kind changes are invisible.** Because diffing keys on
   name only, changing `(defn connect [host])` → `(defn connect [host port])`,
   or changing a `defn` to a `defmacro` with the same name, is neither modified
   nor removed → not breaking. See `edge/arity_change_*`.
4. **Several genuinely-public forms are not tracked at all:** `defmulti`,
   `defmethod`, `deftype`, `defrecord`, `defonce`, `definterface`, and protocol
   methods. See `negative/defonce_etc.clj` (expect 0 by source, even though a
   Clojure reader would consider some of these API).
5. **Extension match is case-sensitive** (`.CLJ` would not be detected).

## Per-file expectations

### positive/ (should be flagged public)
- `positive/defn.clj` → 2 symbols, both kind `defn` (`add`, `square`); the `ns`
  form is ignored.
- `positive/def.clj` → 2 symbols, both kind `def` (`pi`, `default-timeout-ms`).
- `positive/defmacro.clj` → 1 symbol kind `defmacro` (`when-let*`). (Macros ARE
  counted as public by this adapter.)
- `positive/defprotocol.clj` → exactly 1 symbol kind `defprotocol` (`Greeter`);
  the method lines `(greet …)` / `(farewell …)` are NOT emitted.
- `positive/cljs_defn.cljs` → ≥1 symbol kind `defn` (`render`); demonstrates
  `.cljs` is matched by `detect_files`.
- `positive/cljc_shared.cljc` → 2 symbols: `version` kind `def`, `greet` kind
  `defn`; demonstrates `.cljc` is matched.

### negative/ (must NOT be flagged public → expect 0 symbols)
- `negative/private_defn.clj` → 0 (`defn-` does not match `(defn `).
- `negative/comments.clj` → 0 (every line starts with `;` after trim).
- `negative/defonce_etc.clj` → 0 (`defonce`/`defmulti`/`defmethod`/`deftype`/
  `defrecord` match none of the four prefixes).
- `negative/string_and_data.clj` → 0 (top-level string and vector do not start
  with a matched prefix; the `"(defn not-real …)"` string is data, not a form).

### breaking/ (before/after pairs — TRUE breaking per adapter rules)
- `breaking/remove_defn` → after removes `remove-me` → `removed={remove-me}` →
  breaking = true.
- `breaking/remove_protocol` → after removes `Storage` → `removed={Storage}` →
  breaking = true (`helper` retained).
- `breaking/rename_is_breaking` → `old-name` removed, `new-name` added →
  `removed={old-name}` → breaking = true (rename counts as a removal).

### edge/ (hard cases — expectations reflect actual source behavior)
- `edge/private_metadata.clj` → 2 symbols emitted, BOTH named `^:private`
  (kinds `defn` and `def`). Known miss #1: private metadata is treated as a
  public symbol with a garbage name. (Ideal Clojure: 0 public.)
- `edge/comment_block.clj` → 3 symbols emitted: `inside-comment` (`defn`),
  `also-commented` (`def`), and `real-public` (`defn`). The first two sit inside
  `(comment …)` and are FALSE positives — known miss #2. (Ideal: only
  `real-public`.)
- `edge/arity_change` (pair) → same name `connect` before/after →
  `added={}`, `removed={}`, `modified={}` → breaking = FALSE, despite the arity
  change being semantically breaking. Known miss #3.
