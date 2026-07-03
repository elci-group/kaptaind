# Language Adapter Fidelity Matrix

This document describes the capabilities and limitations of each language adapter in kaptaind's API surface detection system.

## Overview

Kaptaind uses language-specific adapters to detect public API symbols. Each adapter has a **confidence level** indicating how reliably it detects changes. The matrix below is organized by confidence level, highest to lowest.

---

## High Confidence (1.0) — Full AST Parsing

These adapters use proper parsing libraries and detect APIs with high precision.

### Rust (Confidence: 1.0)

**Parser**: `syn` v2 crate (full Rust AST)

**What it detects**:
- `pub fn` — public functions with full signatures (e.g., `pub async fn load(path: &str) -> Result<T>`)
- `pub struct` — struct definitions and their `pub` fields (e.g., `User { pub id: u64, name: String }`)
- `pub enum` — enum definitions and all variants (e.g., `Color::Red`, `Color::Blue`)
- `pub trait` — trait definitions, methods, and associated types (e.g., `trait Iterator<T>` with `fn next(&mut self) -> Option<T>`)
- `impl` blocks — public methods on structs/enums/foreign types
- `pub const` — public constants
- `pub static` — public static variables
- `pub type` — public type aliases

**What it misses**:
- Macros (limited; `#[derive]` and procedural macros are not analyzed as APIs)
- Doc comments / semantic documentation
- Visibility modifiers in re-exports (`pub use`)

**Breaking Changes Detected**:
- Removal of any public symbol
- Signature changes on public functions (only removals counted; changes within body ignored)

**Last Updated**: v0.1.43 (Furnace integration added SHA256 caching)

---

### Go (Confidence: 1.0)

**Parser**: Regex-based heuristic scanning

**What it detects**:
- Exported identifiers (uppercase first letter per Go convention)
- `func CapitalName()` — exported functions
- `type CapitalName struct` — exported types and structs
- `const CapitalName` — exported constants
- `var CapitalName` — exported variables
- Interface definitions

**What it misses**:
- Method receivers (implied by function structure)
- Generic constraints (Go 1.18+)
- Struct field visibility (fields inherit package-level export)

**Breaking Changes Detected**:
- Removal of exported symbols

**Notes**: Go's uppercase-= exported convention makes detection simple and reliable.

---

### Swift (Confidence: 1.0)

**Parser**: Regex-based + manual keyword scanning

**What it detects**:
- `public func` — public functions
- `public class`, `public struct` — public classes and structs
- `public enum` — public enums
- `public protocol` — public protocols
- `open class` — open (subclassable) classes
- `@objc` — Objective-C exported symbols
- `public var` — public properties

**What it misses**:
- Method visibility modifiers (some impl details)
- Nested types (if not explicitly marked public)

**Breaking Changes Detected**:
- Removal of public symbols
- Changing `open` to `public` (more restrictive)

**Notes**: Useful for iOS/macOS projects. Well-defined visibility keywords make detection reliable.

---

### Kotlin (Confidence: 1.0)

**Parser**: Regex-based keyword scanning

**What it detects**:
- `fun functionName()` — public functions (default in Kotlin)
- `class ClassName` — public classes
- `data class` — data classes
- `sealed class` — sealed classes
- `object ObjectName` — singleton objects
- `interface InterfaceName` — interfaces
- `@Composable fun` — Compose functions (Jetpack Compose)
- `@JvmStatic` — static members exposed to Java

**What it misses**:
- Internal/private qualifiers (not in all contexts)
- Expect/actual declarations (multiplatform)

**Breaking Changes Detected**:
- Removal of public symbols
- Making symbols `internal` or `private`

**Notes**: Kotlin defaults to public, making opt-out visibility less common. Useful for Android projects.

---

## High-Medium Confidence (0.9) — Excellent Heuristics

### TypeScript (Confidence: 0.9)

**Parser**: Regex-based ES module + CommonJS scanning

**What it detects**:
- `export function` — ES6 named exports
- `export const`, `export let`, `export var` — constant/variable exports
- `export class`, `export interface`, `export type` — class/interface/type exports
- `export default` — default exports
- `export * from` — re-exports
- React hooks (`export function useX()`) — recognized as exportable
- Next.js route exports (`export const GET`, `POST`, etc.) — recognized as API
- Middleware exports (`export function middleware()`)

**What it misses**:
- Dynamic exports (`module.exports = something` in runtime-computed ways)
- Namespace exports (`export namespace`)
- Barrel files exporting re-exports are counted as new exports (not just pass-throughs)

**Breaking Changes Detected**:
- Removal of exported symbols
- Changing default export to named export

**Notes**: Works well for modern JavaScript/TypeScript. Covers 90% of real-world patterns.

---

## Medium-High Confidence (0.85) — Framework-Specific

### Vue (Confidence: 0.85)

**Parser**: Regex-based component scanning

**What it detects**:
- `defineProps()` — component props (removing props is breaking)
- `defineEmits()` — component events (removing emits is breaking)
- `defineExpose()` — exposed public interface
- `export default` — default export (component definition)
- `<script setup>` — composition API properties

**What it misses**:
- Implicit slots (props/emits not explicitly defined)
- Runtime property additions
- Render functions (JSX)

**Breaking Changes Detected**:
- Removal of props or emits (component contract)
- Removing `defineExpose()` values

**Notes**: Vue components have explicit props/emits; this makes them analyzable. Changes to these are high-impact.

---

### Svelte (Confidence: 0.85)

**Parser**: Regex-based top-level scanning

**What it detects**:
- `export let` — exported component props (Svelte 4)
- `$props()` — Svelte 5 runes-based props
- `$emit()` — event emission (Svelte 5+)
- `<script>` exports — anything exported from script block
- Reactive variables (tracked for changes)

**What it misses**:
- Implicit slots
- Event handlers without explicit emit
- Store subscriptions

**Breaking Changes Detected**:
- Removing exported props
- Removing store exports
- Signature changes on store functions

**Notes**: Svelte's top-level exports are explicit, making detection straightforward.

---

### Astro (Confidence: 0.85)

**Parser**: Regex-based frontmatter + props scanning

**What it detects**:
- Frontmatter exports (`export const GET`, `POST`, etc.) — API routes
- Astro props — component interface
- `Astro.props` — runtime props access
- Component default export

**What it misses**:
- Dynamic route segments (hard to detect without execution)
- Middleware (not always explicit)

**Breaking Changes Detected**:
- Removing route handlers
- Removing expected props

**Notes**: Astro is file-system router based; route detection is file-path-based.

---

## Medium Confidence (0.8) — Heuristic-Heavy

### Python (Confidence: 0.8)

**Parser**: Regex-based class/function + `__all__` scanning

**What it detects**:
- `def function_name():` — module-level functions (recognized as public)
- `class ClassName:` — class definitions
- `__all__ = [...]` — explicit public interface (if defined)
- `@property` — properties
- `@classmethod`, `@staticmethod` — special methods

**What it misses**:
- Private-by-convention (`_leading_underscore`)
- Name mangling (`__double_leading__)
- Duck typing and dynamic attributes

**Breaking Changes Detected**:
- Removal of functions/classes listed in `__all__`
- Removal of top-level functions (if `__all__` not defined, assumes all are public)

**Notes**: Python's dynamic nature makes strict API detection hard. Heuristics assume "top-level = public" unless `__all__` is defined.

---

## Lower Confidence (0.7) — JavaScript Heuristics

### JavaScript (Confidence: 0.7)

**Parser**: Regex-based CommonJS + ES6 hybrid scanning

**What it detects**:
- `module.exports = ...` — CommonJS exports
- `exports.name = ...` — CommonJS named exports
- `export function` — ES6 exports (less common in CJS-heavy codebases)
- React hooks (`useX`) — recognized as potentially exportable

**What it misses**:
- Implicit exports (files that are imported as modules)
- Dynamic `require()` patterns
- Barrel file handling (re-exports)

**Breaking Changes Detected**:
- Removal of exported functions/classes
- Changing export format (CJS → ESM)

**Notes**: JavaScript's mixed module ecosystem (CJS + ESM) makes reliable detection harder. Confidence drops to 0.7 accordingly.

---

## Moderate Confidence (0.5) — CSS-Like Languages

### SCSS / Sass / Less (Confidence: 0.5)

**Parser**: Regex-based variable/mixin scanning

**What it detects**:
- `$variable: value` — SCSS variables
- `@variable: value` — Less variables
- `@mixin name { ... }` — mixins
- `@forward` — SCSS forwarding
- CSS custom properties (`--var-name: value`)

**What it misses**:
- Mixin parameters (can be complex)
- Computed values
- Dynamic imports

**Breaking Changes Detected**:
- Removal of widely-used variables or mixins
- Changes to custom property names (for CSS modules)

**Notes**: Treating style resources as "API" is unconventional but useful for design systems. Confidence is lower because style changes are often backward-compatible by accident.

---

### HTML / CSS (Confidence: 0.4)

**Parser**: Regex-based class/ID + custom property scanning

**What it detects**:
- CSS class selectors (`.className`)
- CSS ID selectors (`#idName`)
- CSS custom properties (`--theme-primary: #000`)
- Tailwind utility classes (tracked as design tokens)

**What it misses**:
- Pseudo-selectors (`:hover`, `::before`)
- Keyframe animations
- Media query semantics
- CSS Grid/Flex layouts

**Breaking Changes Detected**:
- Removing CSS custom properties (if used by JS)
- Removing class selectors (if used by templates)

**Notes**: HTML/CSS "APIs" are weak signals; they're included for design system tracking but should not be weighted heavily.

---

## Fallback (Confidence: 0.0) — Line-Based Heuristics

For any file type **not matched by a language adapter**, kaptaind falls back to line-based signature scanning:

**What it detects**:
- Lines starting with keywords: `export`, `function`, `class`, `def`, `func`, `public`

**What it misses**:
- Multi-line signatures (only first line examined)
- Context (doesn't know if it's actually public)
- Language semantics

**Confidence: 0.0** — This is a best-effort fallback. It detects *some* changes but is unreliable.

**Recommendation**: If you're using an unsupported language, either:
1. Disable API scoring (`a = 0.0` in weights) and rely on structural/dependency scoring.
2. Open an issue with language-specific examples; consider contributing an adapter.

---

## Confidence Scoring in Diff Analysis

Each language's confidence is multiplied by the raw API score to produce the final weighted score:

```
weighted_api_score = raw_api_score * confidence
```

For example:
- A Rust file with 5 new public functions: `raw = 0.5` → `weighted = 0.5 * 1.0 = 0.5`
- A JavaScript file with 5 new exports: `raw = 0.5` → `weighted = 0.5 * 0.7 = 0.35`

This means **breaking changes in Rust are weighted more heavily than breaking changes in JavaScript**, reflecting the difference in detection reliability.

---

## Adding a New Language

To add a new language adapter:

1. Create a new adapter in `src/diff/lang/adapters/my_language.rs`
2. Implement the `LanguageAdapter` trait:
   ```rust
   pub trait LanguageAdapter {
       fn parse_ast(&self, path: &Path) -> Result<AstRepresentation>;
   }
   ```
3. Register in `src/diff/lang/adapters/mod.rs`
4. Add tests for your adapter
5. Update this matrix with the confidence level and detection capabilities

---

## Troubleshooting Detection Issues

### "Kaptaind missed my API change"

Check which adapter matched your file:

```bash
# File: src/my_module.rs (Rust)
# Expected adapter: RustAdapter (syn-based, confidence 1.0)

# File: utils.js (JavaScript)
# Expected adapter: JavaScriptAdapter (regex-based, confidence 0.7)
```

If the wrong adapter matched, file an issue or adjust your file extension/path.

### "False positives in API detection"

Some adapters (especially lower-confidence ones) may flag non-breaking changes as new APIs. This is expected—adjust your `[weights]` to reduce the impact of the API dimension:

```toml
[weights]
a = 0.1  # Reduce API weight if you don't trust detection
```

Or use `kaptaind-cli analyze` (dry-run) to preview the score before committing.

### "My language isn't supported"

If your primary language isn't listed:

1. Check if there's a fallback (line-based) detection (confidence 0.0).
2. Open an issue with example files so we can understand patterns.
3. If you're interested, contribute an adapter! See CONTRIBUTING.md.

