# Ruby adapter fixture notes

Source of truth: `src/diff/lang/adapters/ruby.rs` (+ `common.rs`: `read_lines_safe`,
`basic_diff`, `calculate_hash`). All expectations below are derived strictly from that
source as it behaves today, not from ideal Ruby semantics.

## Extensions matched (`detect_files`)
`.rb`, `.rake`, `.gemspec` — extension-only, case-sensitive (`matches!(e, "rb" | "rake" | "gemspec")`).
Note: matching an extension does NOT imply any symbols are emitted (see `negative/*.gemspec`,
`negative/*.rake`).

## Public-symbol rules (`parse_ast`), line-oriented, prefix-based, no scope tracking
Each line is `trim()`ed, then matched in an `if / else if` chain (first match wins; the
`constant` branch is the catch-all for any non module/class/def line):

- kind `module`: line starts with `module `. Name = first whitespace-delimited token, then the
  first segment of a `::` split. `module Api::V2` -> name `Api` (nested tail `V2` is dropped).
- kind `class`: line starts with `class `. Same name rule. `class Admin < User` -> `Admin`
  (superclass ignored). `class << self` -> name `<<` (singleton-class quirk).
- kind `method`: line starts with `def `. Name = chars before the first `(`, ` `, or `;`.
  `def self.foo` -> `self.foo` (`.` is NOT a separator). Setters/operator/predicate names keep
  their trailing `=`, `[]`, `?` (e.g. `name=`, `[]`, `valid?`).
- kind `constant`: any remaining line; text before the first `=` (or the whole line if none) is
  trimmed and emitted iff it is non-empty and EVERY char is ASCII uppercase or `_`.
  Consequence: a bare all-caps word on its own line (no `=`) is also emitted as a constant.

## What is NOT filtered (known misses / false positives)
- Visibility: `private` / `protected` / `private_class_method` are ignored; every `def` is
  emitted as public regardless of the active visibility. (`edge/visibility.rb`)
- Metaprogramming/dynamic API: `attr_reader`/`attr_accessor`/`attr_writer`, `define_method`,
  `public_send`-defined methods are NOT detected (no `def ` prefix). `def method_missing` IS
  emitted as a plain method named `method_missing`. (`edge/dynamic.rb`)
- String/heredoc bodies are parsed as code: a `class`/`def`/all-caps line inside a heredoc is
  falsely emitted. (`edge/heredoc.rb`)
- Comments are only incidentally safe: a leading `#` defeats the `module`/`class`/`def` prefix
  match and defeats the all-uppercase constant test, so comment lines yield nothing.
- No awareness of nesting/scope; all names are flattened (a method inside a singleton class is
  reported the same as a top-level method).

## Breaking definition (`detect_breaking_changes`)
`!diff.removed.is_empty()` over `basic_diff`, which compares symbol SETS BY NAME ONLY.
- Breaking == at least one previously-present symbol NAME is absent in the new version.
- NOT breaking: pure additions; in-place edits that keep the same name (e.g. changing a
  method's parameters/body, or a constant's value, leaves the name present -> not removed ->
  breaking=false). `modified` is always empty (name-only diff has no modification concept).
- A rename == one removal + one addition -> breaking=true (the old name is removed).

## Per-file expectations

positive/
- `classes.rb` -> symbols: class `User`, class `Admin`. (>=2 kind `class`; 0 module/method/constant)
- `modules.rb` -> symbols: module `Billing`, module `Api`. (`module Api::V2` collapses to `Api`; expect 0 symbol named `V2`)
- `methods.rb` -> methods `greet`, `self.configure`, `name=`, `[]` (4 kind `method`)
- `constants.rb` -> constants `MAX_SIZE`, `VERSION`, `DEFAULT_OPTS` (3 kind `constant`)
- `public_api.rb` -> module `Auth`, class `Session`, constant `TTL`, method `valid?`

negative/  (all expect exactly 0 public symbols)
- `comments.rb` -> 0 symbols (`#` prefix defeats module/class/def; `# SECRET = 42` fails the all-uppercase test)
- `strings.rb` -> 0 symbols (single-line string assignments; text before first `=` is lowercase; code-like text after `=` is never inspected)
- `library.gemspec` -> 0 symbols (extension matched, but every assignment is `s.xxx = ...` lowercase -> not constant)
- `tasks.rake` -> 0 symbols (extension matched; `task`/`namespace` DSL has no `def`/`class`/`module`/`=`)

breaking/  (each pair: after removes a symbol name -> breaking=true)
- `remove_method` -> before has method `fetch`; after drops it -> removed={fetch} -> breaking=true
- `remove_constant` -> before has constant `API_VERSION`; after drops it -> removed={API_VERSION} -> breaking=true
- `rename_method` -> before method `connect`; after method `open_connection` (no `connect`) -> removed={connect}, added={open_connection} -> breaking=true (rename == removal under name-only diff)

edge/
- `visibility.rb` -> class `Account` + methods `public_profile`, `secret_token`, `guarded`. ALL three methods are emitted despite `private`/`protected` (documents: visibility ignored).
- `dynamic.rb` -> class `Widget`, class `<<` (from `class << self`), methods `method_missing`, `factory`. NOT emitted: `attr_reader :name`, `attr_accessor :state`, `define_method(:computed)` (documents: dynamic/DSL definitions missed; singleton class yields bogus name `<<`).
- `heredoc.rb` -> constant `HELP_TEXT` (real) PLUS spurious class `Fake` and spurious method `fake` parsed out of the heredoc body (documents: string/heredoc content parsed as code).
