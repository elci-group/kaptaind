# Swift adapter fixture notes

Source of truth: `src/diff/lang/adapters/swift.rs`. Expectations below are derived
strictly from the current source behavior, not from "correct" Swift semantics.

## detect_files
- Matches paths whose extension is exactly `swift` (`e == "swift"`). Nothing else.

## parse_ast — public-symbol rules (line scanner; `line.trim()` per line; no comment/string awareness)
A symbol is emitted ONLY when the trimmed line `starts_with("public ")` or `starts_with("open ")`,
then the prefix is stripped and the remainder is matched against a fixed keyword list. The captured
`name` is the entire remainder of the line after the matched keyword (no truncation of params/body).

| remainder prefix | emitted `kind` |
|------------------|----------------|
| `func `          | `function`     |
| `class `         | `class`        |
| `struct `        | `struct`       |
| `enum `          | `enum`         |
| `protocol `      | `protocol`     |
| `var `           | `property`     |
| `let `           | `property`     |
| `typealias `     | `typealias`    |

Independent of the above, ANY trimmed line `starts_with("@objc")` emits a symbol whose
`name` is the FULL trimmed line and `kind` = `objc_export` (even if the member is private, and
in ADDITION to any function/class/etc. symbol the same line also yields).

The two checks are separate `if`s (not else-if), so one line can emit two symbols.

## Deliberately / accidentally ignored
- `private`, `fileprivate`, `internal`, and unmodified declarations: not flagged.
- `public`/`open` lines whose remainder does not start with one of the 8 keywords are silently
  dropped — e.g. `actor`, `extension`, `init`, `subscript`, `associatedtype`, `case`, `macro`.
- Any declaration with a modifier BETWEEN the visibility and the keyword is dropped because the
  remainder no longer starts with the keyword: `public static func`, `public final class`,
  `public override func`, `public class func` (note: `public class func` is misparsed as a
  `class` whose `name` is `func ...`).
- No comment/string handling: a bare `public func` line inside a `/* */` block (no leading `*`)
  or inside a `"""` multiline string IS still flagged (false positive). Lines prefixed by
  `//`, `///`, or ` * ` are NOT flagged only because the trimmed line no longer starts with
  `public `/`open `.

## detect_breaking_changes
- `!diff.removed.is_empty()` — ONLY removals are breaking.
- Diff is `basic_diff`, keyed by `name` ONLY (kind ignored); `modified` is never populated.
- Consequence: a rename or a signature change alters the captured `name` string, so the old name
  counts as removed -> breaking=true, even though it is also an addition. Pure additions with no
  name loss -> breaking=false. Body-only edits that keep the declaration line identical -> not breaking.

## Per-file expectations

positive/functions.swift       -> 3 symbols, all kind 'function' (incl. one `open func`).
positive/types.swift           -> 5 symbols: 2 'class' (public+open), 1 'struct', 1 'enum', 1 'protocol'.
positive/properties.swift      -> 3 symbols: 2 'property' (var + let), 1 'typealias'.
positive/objc.swift            -> 2 symbols, both kind 'objc_export'; 0 function/property (not public).

negative/private_members.swift -> 0 public symbols (private/fileprivate/internal/unmodified).
negative/comments.swift        -> 0 public symbols (every keyword line is `//`/`///`/` * ` prefixed).
negative/strings.swift         -> 0 public symbols (keywords not at line start; single-line strings).

breaking/remove_func           -> after removes public `greet() {}` -> removed non-empty -> breaking=true.
breaking/remove_type           -> after removes public protocol -> removed non-empty -> breaking=true.
breaking/signature_change      -> param added changes the captured name -> old name removed -> breaking=true
                                  (documents name-based removal rule, not a "modified" notion).

edge/visibility_modifier_order.swift     -> 0 symbols. KNOWN MISS: `public static func`,
                                            `public final class`, `public override func` dropped
                                            (modifier between visibility and keyword). Real-world public API.
edge/block_comment_false_positive.swift  -> 2 symbols: 'function' `phantom() {}` (FALSE POSITIVE from
                                            inside the block comment) + 'property' `real = 1`.
                                            Adapter is comment-unaware.
edge/actor_extension_init.swift          -> 0 symbols. KNOWN MISS: `public actor` and `public extension`
                                            are not in the keyword list (init/subscript/associatedtype likewise).

## Suspected bugs / gaps (reported, not fixed)
1. Modifier-ordering false negatives: any modifier between visibility and keyword
   (`static`/`final`/`override`/`class`/`dynamic`/`mutating`/`nonisolated`...) breaks detection.
2. No comment/string awareness -> false positives for `public ...` lines inside block comments or
   `"""` multiline strings that lack a leading comment marker on that line.
3. Keyword coverage gaps: `actor`, `extension`, `init`, `subscript`, `associatedtype`, `case`, `macro`
   are never emitted, even when `public`/`open`.
4. `public class func X()` is misclassified as kind 'class' with name `func X()`.
5. `@objc` emits `objc_export` unconditionally (including `@objc private ...`) and double-counts a line
   that is both `@objc` and `public` (two symbols for one declaration).
6. Breaking = removals only; no notion of signature modification (a change is seen as remove+add).
   Acceptable given the name-based diff, but `modified` is dead and `kind` never participates.
7. No handling of generated/codegen `.swift` files; they are parsed like hand-written source.
