# SVDO Meter

SVDO Meter is a thin telemetry harness for agentic coding CLI sessions.

It associates a ticket/work identifier with an agent CLI run, invokes or resumes the selected harness, normalizes objective events where possible, and writes durable append-only telemetry locally. It is not an orchestration framework, ticketing system, model router, or remote connector.

## Current Status

The v0.1 baseline centers on:

- `svdo-meter run`
- `svdo-meter telemetry`
- the `codex` harness
- local JSONL telemetry at `.svdo/meter.jsonl`
- rebuildable session association from `session.discovered` events
- fixture-based tests that do not require live Codex execution

## Help

The CLI is built with Clap, so command help is available from the binary:

```bash
svdo-meter --help
svdo-meter run --help
svdo-meter report --help
svdo-meter telemetry --help
```

If running from source:

```bash
cargo run -p svdo-meter -- --help
cargo run -p svdo-meter -- run --help
cargo run -p svdo-meter -- report --help
cargo run -p svdo-meter -- telemetry --help
```

## Compile

Prerequisites:

- Rust stable toolchain, preferably installed with `rustup`
- Cargo on `PATH`
- `codex` on `PATH` when running the Codex harness for real work

Build the debug binary:

```bash
cargo build -p svdo-meter
```

Run it from the build output:

```bash
./target/debug/svdo-meter --help
./target/debug/svdo-meter run --help
./target/debug/svdo-meter report --help
./target/debug/svdo-meter telemetry --help
```

Build the optimized release binary:

```bash
cargo build --release -p svdo-meter
```

Run the release binary:

```bash
./target/release/svdo-meter --help
```

Install it into Cargo's bin directory:

```bash
cargo install --path crates/svdo-meter
```

After install, make sure Cargo's bin directory is on `PATH`, then run:

```bash
svdo-meter --help
```

## Run Codex With Telemetry

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --workspace ~/code/app \
  "Implement the password reset flow described in ENG-142"
```

Conceptual Codex invocation:

```bash
codex exec --json -C ~/code/app "Implement the password reset flow described in ENG-142"
```

For longer or reusable instructions, read the prompt from a UTF-8 text file:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --workspace ~/code/app \
  --prompt-file prompts/eng-142.md
```

## Resume A Known Session

SVDO Meter records a `session.discovered` event when Codex exposes a session/thread ID. Later runs for the same ticket, harness, and workspace automatically use the latest known session when available.

To override automatic lookup:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --session 019c8a42-f72... \
  --workspace ~/code/app \
  "Fix the remaining tests"
```

## Select A Model

Models are harness-specific configuration, not separate adapters:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --harness codex \
  --model gpt-5 \
  --workspace . \
  "Implement ENG-142"
```

## Estimate Token Cost

`svdo-meter report` can estimate local token cost from a UTF-8 JSON pricing file supplied through the CLI. Rates are cost per 1,000,000 tokens and are keyed by exact model identifier:

```bash
svdo-meter report ENG-142 --pricing-file pricing.json
```

When telemetry references a model that is not present in the pricing JSON, the report marks that model's cost as unavailable instead of using a default price.

## Telemetry

By default, events are written to:

```text
.svdo/meter.jsonl
```

Each line is one immutable canonical JSON event. The log is the source of truth; session registries and reports are rebuildable projections.

Raw provider payloads are not persisted by default. This avoids storing prompts, model responses, command output, tool results, environment variables, secrets, and other sensitive content unless raw retention is explicitly enabled in code/configuration.

Inspect local telemetry without modifying `.svdo/meter.jsonl`:

```bash
svdo-meter telemetry sessions --workspace ~/code/app
svdo-meter telemetry runs --workspace ~/code/app
svdo-meter telemetry inspect 018f6f1b-97f1-7c04-9a96-111111111111 --workspace ~/code/app
svdo-meter telemetry inspect sess-abc123 --workspace ~/code/app
```

The telemetry inspection commands tolerate missing or empty telemetry files, report malformed JSONL lines with line numbers, and call out missing token fields on token-bearing records.

## More Documentation

See:

- [docs/compile.md](docs/compile.md) for build and install instructions
- [docs/cli.md](docs/cli.md) for command details, telemetry behavior, event types, and development notes
