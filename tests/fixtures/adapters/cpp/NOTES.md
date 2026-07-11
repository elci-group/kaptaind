# C++ adapter fixture notes

Source of truth: `src/diff/lang/adapters/cpp.rs` (helpers in
`src/diff/lang/adapters/common.rs`). All expectations below are derived from
the source as it reads today, not from an ideal C++ API model.

## Extensions matched (`detect_files`)
`cpp`, `cc`, `cxx`, `hpp`, `h`. Anything else (e.g. `hxx`, `inl`, `tpp`, `c`,
`hh`) is NOT matched.

## How `parse_ast` works
- Reads lines with `read_lines_safe` (file must be <= 5 MiB; stops at the first
  non-UTF-8 line). Line based — no real parser, no multi-line joining.
- Per line: `trimmed = line.trim()`. Skip empty lines and lines starting with
  `//`. NOTE: block comments `/* ... */` and `*` continuation lines are NOT
  skipped (only `//` is).
- Then a single `if/else if` chain, first match wins, at most ONE symbol per
  line, in this order: class -> struct -> namespace -> function.

## Public-symbol rules by `kind`
- `class` (`extract_class`): line starts with `class `; name = text after
  `class ` up to the first of whitespace/`{`/`:`/`;`/`<`. No visibility check,
  so forward declarations `class Foo;` ARE emitted (name stops at `;`).
- `struct` (`extract_struct`): same as class but for `struct ` prefix.
- `namespace` (`extract_namespace`): line starts with `namespace `; name = text
  up to whitespace/`{`/`;`. Anonymous namespace `namespace {` yields an empty
  name and is dropped. Inline `namespace A::B {` yields name `A::B`.
- `function` (`extract_function_definition`): skipped if the line starts with
  any of `if/if(/for/for(/while/while(/switch/switch(/return/return(/static_assert/
  assert(/using/namespace/class/struct/enum/typedef/template/extern/#`. Must
  contain `(` and must NOT end with `;` (so prototypes `void f();` are excluded;
  only definitions with a body count). Name = last whitespace token before the
  first `(`, with trailing `*`/`&` stripped; empty or `>` -> dropped.

There is NO notion of public/private/protected. The class/struct/namespace and
any function *definition* (including a method defined inline inside a class, a
`static` free function, a constructor, destructor, or operator overload) are all
emitted as public symbols.

## Breaking definition (`detect_breaking_changes`)
`!diff.removed.is_empty()`. `diff_ast` = `basic_diff`, which keys purely on
symbol `name` (a `HashSet<&str>`): a name present in old but absent in new is
`removed`; a name only in new is `added`. `modified` is ALWAYS empty.
Consequence: breaking == "any previously-seen symbol name is now gone"
(removal or rename). A signature/parameter/return-type change that keeps the
same identifier is invisible and is NOT breaking.

## Known misses / over-detections (source-derived)
1. No visibility: `private:`/`protected:` methods that have a body are emitted
   as public `function`s; the enclosing class/struct is always emitted.
2. Forward declarations `class Foo;` / `struct Foo;` are emitted as real symbols
   (over-detection).
3. Signature changes are not breaking: function `name` is just the identifier,
   and `basic_diff` never populates `modified`; overloads collapse to one name.
4. `enum` / `enum class` are never emitted (no `enum` kind); the `enum ` prefix
   is only used to skip the function extractor.
5. `static` free functions (internal linkage) are emitted as public.
6. Constructors/destructors/operator overloads are emitted as `function`
   (destructor name keeps its `~`, e.g. `~Foo`).
7. `extern "C" void f() {` lines start with `extern ` and are skipped (miss).
8. `template <...>` lines are ignored; the following `class`/`struct` is still
   detected on its own line (acceptable), but a template-only function whose
   body never opens on a non-template line is missed.
9. Multi-line signatures (the `(` not on the declarator line) are missed.
10. Block comments are not stripped, so a `/* class Foo { */` line that does not
    start with `//` would be misread.

## Per-file expectations

positive/
- class_basic.hpp -> 1 symbol: `Widget` kind `class`. (`void run();` /
  `int value() const;` end with `;` -> not functions.)
- struct_basic.hpp -> 1 symbol: `Point` kind `struct`.
- namespace_basic.hpp -> symbols `math` kind `namespace` and `add` kind
  `function` (>=1 namespace, >=1 function).
- function_free.cpp -> symbols `add` and `greet`, both kind `function`
  (>=2 function).
- method_definition.hpp -> `Counter` kind `class` and `increment` kind
  `function` (inline method definition is emitted as a function).

negative/ (each: expect 0 public symbols)
- comments_and_strings.hpp -> 0 (real constructs only inside `//` comments or a
  string literal; the string line has no `(` so it is not a function).
- declarations_only.hpp -> 0 (`#pragma` skipped; prototypes end with `;`;
  `extern`/`using`/`typedef` lines skipped).
- control_flow.cpp -> 0 (`if`/`for`/`while`/`return` lines are in the skip list;
  no class/struct/namespace).
- macros_and_includes.cpp -> 0 (`#include`/`#define` start with `#`;
  `static_assert` is in the skip list).

breaking/ (each pair: diff by name; removed non-empty -> breaking=true)
- remove_class: before has `Widget`(class)+`helper`(function); after keeps only
  `helper` -> `Widget` removed -> breaking=true.
- rename_function: before `add`; after `sum` -> `add` removed, `sum` added ->
  breaking=true (rename == removal+addition).
- remove_namespace: before `io`(namespace)+`write`(function); after only
  `write` -> `io` removed -> breaking=true.

edge/
- templates.hpp -> `Box` kind `class` and `get` kind `function`; the
  `template <typename T>` line emits nothing.
- forward_declarations.hpp -> 3 symbols: `Widget`(class), `Point`(struct),
  `Real`(class) — forward declarations are emitted (over-detection).
- constructors_operators.hpp -> `Foo`(class) plus functions `Foo`, `~Foo`,
  `operator==` (ctor/dtor/operator all emitted as functions).
