# Perl adapter — fixture expectations (source-derived)

Source: `src/diff/lang/adapters/perl.rs`. Expectations below describe what the
adapter **does today**, not necessarily what is semantically ideal.

## Extensions matched (`detect_files`)

- `.pl` and `.pm` only. Anything else (`.t`, `.cgi`, `.pod`, no extension) is
  not detected.

## Public-symbol rules by `kind` (`parse_ast`)

Lines are read with `read_lines_safe` (UTF-8, ≤5 MB). Each line is `trim()`d,
then matched by `else if` — at most one symbol per line. No comment/string/POD
skipping, no visibility filter.

- `kind = "package"`: trimmed line starts with `package `. Name = first
  whitespace-delimited token, trailing `;` stripped. Empty name → not emitted.
- `kind = "sub"`: trimmed line starts with `sub `. Name = first token split on
  whitespace / `(` / `{` / `;` / `:`, then trimmed of trailing `({;:`. Empty
  name (anonymous `sub {`) → not emitted.
- `kind = "constant"`: trimmed line starts with `use constant `. Name = first
  whitespace token, trimmed of trailing `({;,`. The hash form
  `use constant { A => 1, ... }` yields an empty name (first token is `{`) →
  **0 constants emitted** (known miss).

`extract_api` clones `ast.symbols` verbatim; `structure_hash` = hash of symbols.

## Breaking definition (`detect_breaking_changes` / `basic_diff`)

`basic_diff` keys on symbol **name only** (a `HashSet<&name>`); `modified` is
always empty. `detect_breaking_changes` = `!diff.removed.is_empty()`.

- A symbol whose name disappears → `removed` non-empty → **breaking = true**.
- Rename (old name gone, new name added) → old is `removed` → **breaking = true**.
- Signature / body change with the **same** name → name present in both →
  neither added nor removed → **breaking = false**.
- Pure addition → `removed` empty → **breaking = false**.

## Known misses / false positives (gaps)

- `_leading` underscore convention (Perl "private") is **not** honored → private
  subs are flagged public (`edge/underscore_private.pm`).
- No POD / heredoc / multi-line string skipping → a line beginning with `sub `
  / `package ` / `use constant ` inside POD/heredoc is falsely emitted
  (`edge/pod_false_positive.pm`).
- Dynamic APIs invisible: typeglob aliases (`*x = \&y`), `eval`-defined subs,
  methods generated via `AUTOLOAD`. A literal `sub AUTOLOAD` is emitted, but the
  methods it synthesizes are not (`edge/dynamic_autoload.pm`).
- `use constant { ... }` hash form → 0 constants (see rule above).
- Inline strings whose keywords are not at line start are correctly **not**
  flagged (match is anchored to the trimmed line start) — see `negative/`.

## Per-file expectations

positive/
- `package_and_subs.pm` → expect kind `package` "Demo::Math" (1), `constant`
  "PI" (1), `sub` names incl. "add","multiply" (≥2).
- `constants.pm` → expect `package` "Config::Limits" (1), `constant`
  "MAX_RETRIES","TIMEOUT_SEC","DEFAULT_HOST" (3), `sub` "limit_for" (1).
- `script.pl` → `.pl` detected; expect `sub` "greet","main" (≥2); no `package`.
- `sub_with_signature.pm` → expect `package` (1), `sub` "with_signature",
  "as_method", "plain" (3) — splitter handles `($self)` and `:method`.
- `namespaced_package.pm` → expect `package` "Acme::Widget::Factory::Builder"
  (1), `constant` "VERSION_TAG" (1), `sub` "build" (1).

negative/ (each → expect **0 public symbols**)
- `comments.pm` → keyword lines start with `#`; no prefix match → 0.
- `variables_only.pm` → only `my`/`our`/`local`/`print` lines → 0.
- `inline_strings.pm` → keywords inside string values, lines start with
  `my`/`print` → anchored prefix misses → 0.

breaking/ (before→after, breaking = `removed` non-empty)
- `remove_sub` → after removes `sub public_endpoint` → removed=[public_endpoint]
  → **breaking = true**.
- `remove_constant` → after removes `constant MAX_RETRIES` →
  removed=[MAX_RETRIES] → **breaking = true**.
- `rename_sub` → `old_name`→`new_name`: old name removed → **breaking = true**
  (name-keyed diff treats rename as removal+addition).

edge/ (expectations reflect today's behavior, incl. gaps)
- `underscore_private.pm` → expect `sub` "public_api","_private_helper",
  "_another_internal" ALL emitted (no underscore filter → private-by-convention
  flagged public).
- `pod_false_positive.pm` → expect `sub` "real_api" AND "frobnicate" emitted;
  "frobnicate" is a verbatim-POD false positive (POD not skipped).
- `dynamic_autoload.pm` → expect `package` "Dyno", `sub` "real_thing" and
  "AUTOLOAD"; typeglob alias `generated_thing` is NOT in symbols (dynamic API
  missed).
