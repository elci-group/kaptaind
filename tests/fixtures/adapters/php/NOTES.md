# PHP adapter — fixture expectations (source-derived)

Source: `src/diff/lang/adapters/php.rs`. Expectations below describe what the
adapter does **today**, not what it ideally should. Parsing is purely line-based:
each line is `trim()`med and matched by prefix; there is no real AST, no scope
tracking, and no awareness of `<?php ?>` tags, strings, or block comments.

## Extensions matched (`detect_files`)
- Only extension exactly `php` (`p.extension() == "php"`).
- NOT matched: `.phtml`, `.php4`/`.php5`, `.phar`, `.inc`, files with no extension.

## Public-symbol rules (by `kind`)
Line must be non-empty and not start with `//` or `#` (those are skipped). First
matching rule wins; the type-declaration loop is last.

- `namespace`: line starts with `namespace ` → name = first whitespace token after
  the keyword, with a trailing `;` stripped.
- `function`: line starts with `function ` → name = text up to the first `(`, then
  its first whitespace token. (Catches top-level functions; also catches any
  method declared without an explicit visibility keyword, mislabelled as
  `function`.)
- `method`: line starts with `public ` (optional `static ` stripped) then
  `function ` → name parsed like `function`.
- `class_constant`: `public ` (optional `static`) then `const ` → first token.
- `property`: `public ` (optional `static`) then anything else → **last**
  whitespace token, trailing `;` stripped, emitted only if it starts with `$`.
- `class` / `interface` / `trait` / `enum`: line starts with that keyword + space
  → first whitespace token after it.

## Deliberately ignored / known misses
- `private`/`protected` members: never matched (only `public ` is).
- Lines starting with `//` or `#`: skipped (so PHP 8 attributes `#[...]` are
  skipped too — harmless since attributes carry no symbol themselves).
- Enum `case` values: not emitted (only the `enum` itself).
- `public $x = <default>;`: **MISSED.** Property parsing takes the *last* token,
  which is the default value, not `$name`, so it fails the `starts_with('$')`
  check (see `edge/promoted_and_defaults.php`).
- Constructor property promotion `__construct(public int $id)`: promoted `$id`
  not emitted; only `__construct` (as `method`) is.
- Keyword order: `abstract public function`, `final public function`,
  `final class`, `readonly public` (when `readonly` leads) are missed because the
  line does not start with `public `/`function `/a type keyword.
- Bare global `const FOO = 1;` (no `public`): not matched.
- Backed enum `enum Status: string {`: name extracted as `Status:` (trailing
  colon) — only the first whitespace token is taken.
- Braced global namespace `namespace {`: emits name `{`.
- Diff/breaking compares by **name only** (`basic_diff`); `modified` is always
  empty. Kind changes or signature changes that keep the same name are invisible.
- Block comments `/* ... */` are NOT skipped, but inner lines start with `*` so
  they do not match any prefix (safe in practice).
- Strings/heredocs are not recognized: a line beginning `public function …`
  inside a heredoc is a false positive (see `edge/docblock_heredoc.php`).
- `<?php ?>` tags ignored: a `function …` line outside PHP tags still matches.

## Breaking definition (`detect_breaking_changes`)
`!diff.removed.is_empty()` — **removals only**, by name. A rename is removal +
addition → breaking. Additions are never breaking. Signature/body/kind changes
that preserve the name → not breaking (gap).

## Per-file expectations

### positive/ (must be flagged)
- `classes.php` → `namespace` "App"; `class` "User"; `class_constant` "ROLE_ADMIN";
  `property` "$name"; `method` "save" (5 symbols).
- `interface_trait.php` → `namespace` "App"; `interface` "Logger"; `method` "log";
  `trait` "Timestampable"; `property` "$createdAt"; `method` "now" (6).
- `enum.php` → `namespace` "App"; `enum` "Status" only; `case` values NOT emitted.
- `functions.php` → `namespace` "App\Helpers"; `function` "format"; `function` "parse".
- `static_members.php` → `class` "Config"; `class_constant` "VERSION";
  `property` "$instance"; `method` "get".

### negative/ (must not be flagged public)
- `private_protected.php` → exactly 1 symbol: `class` "Account". Zero
  `method`/`property`/`class_constant` from the private/protected members.
- `comments_strings.php` → 0 symbols (line/hash/block comments and strings).
- `namespace_use.php` → 0 symbols (`use`/`use function`/`use const` are imports).

### breaking/ (true breaking per adapter — name removed)
- `remove_method`: before {class User, method save, method delete}; after drops
  "save" → removed={save} → breaking=true.
- `remove_class`: before {namespace App, class User}; after {namespace App} →
  removed={User} → breaking=true.
- `rename_property`: before property "$displayName"; after "$fullName" →
  removed={"$displayName"}, added={"$fullName"} → breaking=true (rename = removal).

### edge/ (hard cases; §8 visibility, comments/encodings, multi-language files)
- `docblock_heredoc.php` → `class` "Demo"; `method` "real"; `method` "sample";
  **false-positive** `method` "insideHeredoc" (line inside heredoc matches).
  Docblock `public function fake()` is mid-line → not emitted.
- `promoted_and_defaults.php` → `class` "Service"; `method` "__construct" only.
  `property` "$label" MISSED (default-value last-token bug); promoted "$id" MISSED.
- `attributes_and_mixed_html.php` → `class` "Controller"; `method` "index";
  `function` "outsidePhpTags" (tags ignored → matched outside `<?php`).
  `#[Route]`/`#[Get]` skipped (`#`); `<p>…public function inHtml()</p>` mid-line
  → not emitted.
