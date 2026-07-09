# Trawler module

The trawler discovers codebases under a directory tree and prepares them for
kaptaind. It is used by `kaptaind-cli trawl` and by the (optional) daemon
`[trawl]` configuration.

## What it does

1. **Walk** a directory tree root-down using `ignore::WalkBuilder`.
   - Respects `.gitignore`, `.ignore`, global gitignore, and hidden-directory rules.
   - Skips the built-in blacklist (`target/`, `node_modules/`, `.git/`, etc.).
   - Applies an optional user blacklist (`--blacklist` or `[trawl].blacklist`).
2. **Detect** projects by language-specific markers with confidence scoring.
   - Rust projects are gated on a valid `Cargo.toml`: the manifest must contain
     `[package]` and/or `[workspace]`. Stray or empty `Cargo.toml` files are ignored.
3. **Reduce** root-down: the outermost valid project wins.
   - Cargo workspace roots report their member crates as separate entries.
   - Nested standalone `Cargo.toml` files under a non-workspace package are not
     reported as separate roots.
4. **Initialize** discovered projects with `kaptaind.toml`, `.kaptainignore`,
   `VERSION`, and the `.kaptaind/` state directory.
   - Workspace members are reported but not initialized unless
     `--expand-workspaces` is set.

## CLI usage

```bash
# Discover everything under the current directory
kaptaind-cli trawl

# Discover only Rust/Go projects, don't initialize anything
kaptaind-cli trawl --type rust,go --dry-run

# Skip custom directories in addition to the built-in list
kaptaind-cli trawl --blacklist scratch,vendor/*

# Surface projects inside gitignored directories
kaptaind-cli trawl --no-ignore
```

## Configuration

The `[trawl]` block in `kaptaind.toml` supports:

```toml
[trawl]
root = "~/projects"
max_depth = 3
skip_initialized = true
require_git = false
auto_register = true
blacklist = ["scratch", "vendor/*"]
respect_ignore_files = true
expand_workspaces = false
```

## Module structure

| File | Responsibility |
|------|----------------|
| `src/trawler/mod.rs` | Public re-exports. |
| `src/trawler/project.rs` | Language detection, confidence scoring, Cargo manifest inspection, workspace member resolution, skip-list/blacklist helpers. |
| `src/trawler/engine.rs` | `trawl()` orchestration, `ignore::WalkBuilder` traversal, root-down reduction, project initialization/reporting. |

## Tests

Trawler tests are colocated in `src/trawler/project.rs` and
`src/trawler/engine.rs`. They cover language detection, manifest validation,
workspace member expansion, blacklist/ignore behavior, max-depth filtering, and
root-down reduction.

Run them with:

```bash
cargo test -p kaptaind --lib trawler
```
