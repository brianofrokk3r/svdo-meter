# Compile SVDO Meter

This project is a Rust workspace. The runnable binary package is `svdo-meter` under `crates/svdo-meter`.

## Prerequisites

Install the stable Rust toolchain with `rustup`:

```bash
rustup toolchain install stable
rustup default stable
```

Confirm the required tools are available:

```bash
rustc --version
cargo --version
```

The repository includes `rust-toolchain.toml`, so Cargo should use the pinned stable toolchain when invoked from the repo.

For real measured Codex runs, install and authenticate the Codex CLI separately and ensure `codex` is on `PATH`:

```bash
codex --help
```

For real measured Claude Code runs, install and authenticate Claude Code separately and ensure `claude` is on `PATH`:

```bash
claude --help
claude auth status
```

Fixture tests and normal compilation do not require live Codex or Claude Code execution.

## Build Debug Binary

From the repository root:

```bash
cargo build -p svdo-meter
```

The debug binary is written to:

```text
target/debug/svdo-meter
```

Run help from the debug binary:

```bash
./target/debug/svdo-meter --help
./target/debug/svdo-meter run --help
```

## Run From Source

You can run the binary without installing it:

```bash
cargo run -p svdo-meter -- --help
cargo run -p svdo-meter -- run --help
```

Example measured run:

```bash
cargo run -p svdo-meter -- run \
  --ticket ENG-142 \
  --harness codex \
  --workspace . \
  "Implement ENG-142"
```

Example measured run with a prompt file:

```bash
cargo run -p svdo-meter -- run \
  --ticket ENG-142 \
  --harness codex \
  --workspace . \
  --prompt-file prompts/eng-142.md
```

Example Claude Code measured run:

```bash
cargo run -p svdo-meter -- run \
  --ticket ENG-142 \
  --harness claude \
  --model sonnet \
  --workspace . \
  --claude-max-turns 3 \
  "Implement ENG-142"
```

## Build Release Binary

For a faster standalone binary:

```bash
cargo build --release -p svdo-meter
```

The release binary is written to:

```text
target/release/svdo-meter
```

Run it directly:

```bash
./target/release/svdo-meter --help
```

## Install Locally

Install the binary into Cargo's bin directory:

```bash
cargo install --path crates/svdo-meter
```

Cargo typically installs binaries to:

```text
~/.cargo/bin
```

Make sure that directory is on `PATH`, then verify:

```bash
svdo-meter --help
svdo-meter run --help
```

## Expected Quality Checks

Run these before committing or releasing:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

If `cargo-deny` is installed, also run:

```bash
cargo deny check
```

Install `cargo-deny` when needed:

```bash
cargo install cargo-deny
```

## Troubleshooting

If `cargo` is not found, Rust is not installed or Cargo's bin directory is not on `PATH`.

If `svdo-meter` is not found after `cargo install`, add Cargo's bin directory to `PATH`.

If `svdo-meter run --harness codex ...` fails to start Codex, confirm the Codex CLI is installed, authenticated, and available as `codex` on `PATH`.

If `svdo-meter run --harness claude ...` fails to start Claude Code, confirm the Claude Code CLI is installed, authenticated, and available as `claude` on `PATH`. SVDO Meter runs Claude Code in print mode with `--output-format stream-json --verbose`; unsupported combinations such as conflicting resume/session flags fail before launch.

Telemetry is written under the selected workspace:

```text
<workspace>/.svdo/meter/<run-id>.jsonl
```

If `--workspace` is omitted, the current directory is used.
