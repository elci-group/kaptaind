# Contributing to kaptaind

First off, thank you for considering contributing to kaptaind! We value community contributions and strive to make the process as straightforward as possible.

## Project Structure & Architecture

If you are new to the codebase, we highly recommend reading through:
- **`README.md`**: For a high-level overview of the background architecture and daemon lifecycle.
- **`AGENTS.md`**: For an in-depth breakdown of the code layout, module responsibilities, scoring heuristics, and daemon behavior. It is designed for LLMs but is extremely useful for human developers.
- **`docs/planning/`**: For historical context on why certain features (like TUI, notifications, and MVP architecture) were built the way they are.

## How to contribute

1. **Fork the repository** on GitHub.
2. **Clone the fork** to your local machine.
3. **Create a branch** for your feature or bugfix (`git checkout -b feature/my-new-feature`).
4. **Make your changes**. 
5. **Run the tests** locally using `cargo test`.
6. **Commit your changes**. Note that `kaptaind` is designed to automate versioning and commit message structuring, but for your manual branches, follow standard conventional commits if possible!
7. **Push your branch** to your fork.
8. **Open a Pull Request** against the main repository.

## Development Environment

To work on kaptaind, you'll need:

- Rust (edition 2021) and Cargo installed.
- OpenSSL development packages (often required by `git2` under the hood depending on your system).

### Building and Testing

```bash
cargo build
cargo test
```

## Development Workflow

### Running Tests

Run all tests:

```bash
cargo test
```

Run a single test:

```bash
cargo test test_name -- --nocapture
```

For example, to run the Rust adapter tests with output:

```bash
cargo test rust_adapter -- --nocapture
```

Run tests in a specific module:

```bash
cargo test -p kaptaind --lib diff::lang::adapters
```

### Building and Testing Locally

Build the main daemon and CLI:

```bash
cargo build --release
```

Test the CLI in your current repository:

```bash
./target/release/kaptaind-cli analyze
./target/release/kaptaind-cli status
```

Test the daemon in foreground mode (for debugging):

```bash
./target/release/kaptaind
# Ctrl+C to stop
```

### Example Pull Request Workflow

1. Fork the repository and clone your fork.
2. Create a feature branch:
   ```bash
   git checkout -b feature/support-new-language
   ```
3. Make your changes and add tests:
   ```bash
   cargo test  # Verify all tests pass
   ```
4. Commit with descriptive message (kaptaind will auto-version if you have the daemon running, or write manually):
   ```bash
   git commit -m "feat: add TypeScript enum support to AST parser"
   ```
5. Push and open a Pull Request:
   ```bash
   git push origin feature/support-new-language
   ```

## Adding a New Language Adapter

To add support for a new language:

### 1. Add the Adapter Implementation

Create a new file at `src/diff/lang/adapters/my_language.rs` and add a struct:

```rust
pub struct MyLanguageAdapter;

impl LanguageAdapter for MyLanguageAdapter {
    fn parse_ast(&self, path: &Path) -> Result<AstRepresentation> {
        // 1. Read file
        let content = std::fs::read_to_string(path)?;
        
        // 2. Parse the file (use a parser crate like `tree-sitter`, `syn`, etc.)
        // 3. Extract public symbols (functions, classes, constants, etc.)
        // 4. Return AstRepresentation with symbols list
        
        let symbols = vec![
            Symbol { name: "MyFunction".to_string(), kind: "function".to_string() },
            Symbol { name: "MyClass".to_string(), kind: "class".to_string() },
        ];
        
        Ok(AstRepresentation {
            symbols,
            structure_hash: compute_hash(&content),
        })
    }
}
```

### 2. Register the Adapter

Edit `src/diff/lang/adapters/mod.rs` and add your adapter to `register_builtin_adapters`:

```rust
pub use my_language::MyLanguageAdapter;

pub fn register_builtin_adapters(registry: &mut AdapterRegistry) {
    // ... existing adapters ...
    registry.register(Box::new(MyLanguageAdapter));
}
```

### 3. Add Tests

In `src/diff/lang/adapters/my_language.rs`, add unit tests for your adapter:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn my_language_detects_functions() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.my_ext");
        std::fs::write(&file, "public fun my_function() {}").unwrap();

        let adapter = MyLanguageAdapter;
        let result = adapter.parse_ast(&file).unwrap();
        
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "my_function");
        assert_eq!(result.symbols[0].kind, "function");
    }
}
```

### 4. Update Language Matrix

Update `LANGUAGE_MATRIX.md` with your new adapter's confidence level, detection capabilities, and limitations.

## Writing Integration Tests

Integration tests live in `tests/cli_integration.rs` and test full CLI commands:

```rust
#[test]
fn analyze_detects_new_api() {
    let dir = tempdir().unwrap();
    // Create a fake repo with changes
    std::fs::write(dir.path().join("main.rs"), "pub fn hello() {}").unwrap();
    
    // Run analyze and check output
    let output = Command::new("cargo")
        .args(&["run", "--", "analyze"])
        .current_dir(dir.path())
        .output()
        .expect("analyze failed");
    
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("api_touches"));
}
```

## Code Style & Conventions

- **Formatting**: Use `cargo fmt` before committing.
- **Linting**: Run `cargo clippy` and fix warnings.
- **Module layout**: Each module file exports its public API in `mod.rs`.
- **Error handling**: Use `anyhow` for application errors, `git2::Error` for git-specific errors.
- **Logging**: Use `tracing` crate (info!, warn!, debug!, error! macros).
- **Tests**: Colocate unit tests in modules under `#[cfg(test)]`; use `tempfile` for filesystem tests.

Example formatting check:

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

## Performance Considerations

When adding new features:

1. **Caching**: Use SHA256 file hashing to avoid re-parsing unchanged files. See `src/diff/cache.rs` for the pattern.
2. **Async**: Filesystem watching and daemon scheduling use async Tokio. Keep blocking operations short.
3. **Regex**: For heuristic-based adapters, compile regexes once (use lazy_static or const fns).
4. **Memory**: Large repos can have thousands of events. Avoid unbounded allocations in loops.

## Bug Reports and Feature Requests

If you encounter any bugs or have ideas for new features, please open an issue in the GitHub issue tracker. Please provide as much detail as possible, including:

- Your operating system.
- The version of `kaptaind` you are using.
- A minimal reproduction of the issue if applicable.
- For API detection issues, the language(s) affected and file examples.

## Documentation

If your change affects user-facing behavior:

1. Update `README.md` (high-level overview).
2. Update `AGENTS.md` (technical details for agents).
3. Update `LANGUAGE_MATRIX.md` (if adding/changing language adapters).
4. Update `CHANGELOG.md` (document the change).

## License

All contributions to kaptaind are licensed under the MIT License. See LICENSE file in the repository.

Thank you for contributing!
