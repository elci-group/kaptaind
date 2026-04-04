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

## Bug Reports and Feature Requests

If you encounter any bugs or have ideas for new features, please open an issue in the GitHub issue tracker. Please provide as much detail as possible, including:

- Your operating system.
- The version of `kaptaind` you are using.
- A minimal reproduction of the issue if applicable.

Thank you!
