# SVDO Repository Standards

The active repository standards are:

1. [Rust quality](standards/1-rust-quality/SKILL.md) — implementation quality, error handling, testing, dependency hygiene, and CI gates.
2. [Rust architecture](standards/2-rust-architecture/SKILL.md) — crate boundaries, harness adapters, canonical telemetry, durable persistence, and reporting.

Read every active standard that applies before changing the repository. A change that affects both implementation quality and telemetry or reporting architecture must follow both standards.

For Rust changes, run the required repository gates:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

`cargo deny check` requires the `cargo-deny` subcommand to be installed in the validation environment.

