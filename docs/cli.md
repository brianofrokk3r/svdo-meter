# SVDO Meter CLI

## Commands

SVDO Meter currently exposes one functional command:

```text
svdo-meter run
```

Help is available through Clap:

```bash
svdo-meter --help
svdo-meter run --help
```

From source:

```bash
cargo run -p svdo-meter -- --help
cargo run -p svdo-meter -- run --help
```

Build and install instructions are in [compile.md](compile.md).

## `svdo-meter run`

Starts or resumes measured agent CLI work and appends canonical telemetry locally.

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --workspace ~/code/app \
  "Implement the password reset flow described in ENG-142"
```

Prompt-file form:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --workspace ~/code/app \
  --prompt-file prompts/eng-142.md
```

### Arguments

| Argument | Required | Description |
|---|---:|---|
| `--ticket <TICKET>` | Yes | External ticket/work identifier. SVDO Meter records this as the join key for future reports or enrichment. |
| `--harness <HARNESS>` | Yes | Agent CLI harness. v0.1 supports `codex`. |
| `<PROMPT>` | Yes, unless `--prompt-file` is used | Inline prompt or work instruction forwarded to the harness. Prompts are not persisted by default. |
| `--prompt-file <PATH>` | Yes, unless `<PROMPT>` is used | UTF-8 text file whose contents are forwarded to the harness as the prompt. Cannot be combined with an inline prompt. |
| `--label <LABEL>` | No | Human-readable label copied to canonical run events. |
| `--workspace <PATH>` | No | Workspace directory passed to the harness and used as the base for `.svdo/meter.jsonl`. |
| `--session <SESSION_ID>` | No | Explicit provider session/thread override for this run. |
| `--model <MODEL>` | No | Harness-specific model configuration. For Codex this is passed as a Codex model argument. |

SVDO Meter reads `--prompt-file` before starting the harness. Missing, unreadable, or non-UTF-8 files fail fast with a path-aware CLI error.

## Codex Harness

The Codex adapter invokes the CLI with explicit process arguments, not a shell command string.

First run shape:

```text
codex exec --json -C <workspace> <prompt>
```

When `--prompt-file <path>` is used, SVDO Meter reads the file and passes the resolved text as `<prompt>`.

Resume shape when a session is known:

```text
codex exec --json -C <workspace> resume <session_id> <prompt>
```

Model shape:

```text
codex exec --json -C <workspace> --model <model> <prompt>
```

## Session Behavior

SVDO Meter emits `session.discovered` when it learns a provider session ID. Session lookup is based on:

```text
ticket_id + harness + workspace
```

An explicit `--session` value wins over automatic lookup for that run and is also recorded as a session association.

The event log remains canonical. Any local session registry is a rebuildable projection from `session.discovered` events.

## Telemetry Storage

Default telemetry path:

```text
<workspace>/.svdo/meter.jsonl
```

If `--workspace` is omitted, the current directory is used as the base.

Each JSONL line is one canonical event with common metadata such as:

- `schema_version`
- `event_id`
- `event_type`
- `occurred_at`
- `observed_at`
- `run_id`
- `ticket_id`
- `label`
- `harness`
- `requested_model`
- `resolved_model`
- `session_id`
- `workspace`
- `payload`

## Canonical Event Types

The v0.1 event model includes:

- `run.started`
- `session.discovered`
- `harness.event`
- `usage.reported`
- `command.started`
- `command.completed`
- `files.changed`
- `tool.started`
- `tool.completed`
- `run.completed`
- `run.failed`

Unknown provider events are tolerated and do not fail the run. Raw provider payloads are only retained when explicit raw retention is enabled.

## Metrics

SVDO Meter records objective metrics where the harness exposes them:

- wall time
- active turn or agent time
- command/tool time
- turn count
- provider event count
- input tokens
- cached input tokens
- cache-write tokens
- output tokens
- reasoning tokens
- commands executed
- failed commands
- files changed
- tool calls
- errors
- run success/failure

Elapsed durations are measured with `Instant` inside the engine rather than by subtracting wall-clock timestamps.

## Privacy Defaults

By default, SVDO Meter avoids persisting:

- prompts
- model responses
- shell output
- tool results
- environment variables
- secrets
- raw provider payloads

The durable log is intended to preserve objective telemetry and safe structural metadata by default.

## Development Checks

Expected checks when the Rust toolchain is available:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

Normal tests use fixtures under `tests/fixtures/` and do not require live Codex execution.
