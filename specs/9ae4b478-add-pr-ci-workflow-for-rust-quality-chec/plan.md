# Implementation Plan: Add PR CI Workflow for Rust Quality Checks

## Context

The repository is a Rust workspace with a checked-in `Cargo.lock`, a `rust-toolchain.toml` selecting the stable channel, and a `deny.toml` dependency policy.

## Approach

1. Create a GitHub Actions workflow in `.github/workflows/`.
2. Trigger the workflow on pull requests and mainline pushes for early feedback.
3. Install the stable Rust toolchain with `rustfmt` and `clippy` components.
4. Run formatting, linting, tests, and dependency policy checks as separate named steps.
5. Use locked dependency resolution for Cargo build/test commands and for installing `cargo-deny`.
6. Install `cargo-deny` with `cargo install cargo-deny --locked` before running `cargo deny check`.

## Validation

- Inspect the generated workflow YAML.
- Confirm the workflow includes all required commands and pull request trigger.
- Check repository status for the expected added files only.
