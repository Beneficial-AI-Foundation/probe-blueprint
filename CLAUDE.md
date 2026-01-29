# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

probe-blueprint is a Rust CLI tool that generates call graph data and analyzes Blueprint verification results for Lean 4 projects. It has four subcommands:
- **stubify**: Extract mathematical stubs from Blueprint LaTeX files (theorem, lemma, definition, etc.)
- **atomize**: Generate call graph atoms with accurate line numbers
- **specify**: Extract function specifications from atoms.json
- **verify**: Run Blueprint verification and analyze results

## Build and Test Commands

```bash
# Build
cargo build                    # Debug build
cargo build --release          # Optimized release build
cargo install --path .         # Install locally

# Test
cargo test                     # All tests
cargo test --lib --verbose     # Unit tests only

# Code quality (all enforced in CI)
cargo fmt --all                # Format code
cargo clippy --all-targets -- -D warnings  # Lint (no warnings allowed)

# Development workflow
cargo fmt && cargo clippy --all-targets && cargo test
```

## Project Structure

```
src/
├── main.rs           # CLI entry point with subcommand routing
├── lib.rs            # Core data structures and parsing
└── commands/         # Subcommand implementations
    ├── mod.rs
    ├── stubify.rs
    ├── atomize.rs
    ├── specify.rs
    └── verify.rs
```

## When Changing Features

When features are added, edited, or deleted:
- Update the README.md with any CLI or usage changes
- Update relevant documentation in docs/
- Update or add tests to cover the changes

## Before Committing

Always run fmt and clippy before committing and pushing:

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings
```

## Commit Message Style

Use conventional commits: `feat(module):`, `fix(module):`, `perf(module):`, `refactor(module):`

Examples:
- `feat(atomize): add support for Lean 4 syntax`
- `fix(verify): handle Blueprint error messages correctly`
- `refactor(specify): simplify specification extraction`
