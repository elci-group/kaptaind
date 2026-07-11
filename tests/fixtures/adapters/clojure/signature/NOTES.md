# Clojure `signature/` corpus (rev 14)

Clojure emits a **bare-identifier** `name` for `defn` (`add`) via `parse_symbol_name` (first
token after `(defn `), with `kind = "defn"`; the argument vector `[a]` is not part of `name`,
so before rev 14 an arity change was invisible. rev 14 records `signatures[name]` as the
balanced **argument vector `[ … ]`** (NOT parens — the first `(` on a `defn` line is the body,
so the shared paren helper would capture body and false-modified). Body-independent: the scan
stops at the matching `]`, so the `(+ …)` body is not captured.

| pair | name | old signature -> new signature | breaking-policy hint |
|------|------|--------------------------------|----------------------|
| add_param | `add` | `[a]` -> `[a b]` | **yes** — Clojure arity is dispatched at call time; a new fixed arity breaks 1-arg callers. |

Notes:
- The argument vector is complete on the `defn` line, so the line-based adapter captures it
  cleanly; the body (`a` / `(+ a b)`) emits no symbol. Only the `add` vector change drives
  `modified`.
- `defmacro` uses the same `[…]` arg-vector shape and is left as an identical one-line
  follow-up; this corpus covers `defn`.
- Registers as `modified`, NOT `removed`; `ClojureAdapter::detect_breaking_changes` keys off
  `removed`, so it is intentionally not auto-breaking (gold-gated policy — see CALIBRATION.md).
