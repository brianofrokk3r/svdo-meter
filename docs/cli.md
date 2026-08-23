# SVDO Meter CLI

## Commands

SVDO Meter currently exposes these functional commands:

```text
svdo-meter run
svdo-meter report
svdo-meter telemetry
```

Help is available through Clap:

```bash
svdo-meter --help
svdo-meter run --help
svdo-meter report --help
svdo-meter telemetry --help
svdo-meter telemetry sessions --help
svdo-meter telemetry runs --help
svdo-meter telemetry inspect --help
```

From source:

```bash
cargo run -p svdo-meter -- --help
cargo run -p svdo-meter -- run --help
cargo run -p svdo-meter -- report --help
cargo run -p svdo-meter -- telemetry --help
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
| `--workspace <PATH>` | No | Workspace directory passed to the harness and used as the base for `.svdo/meter/`. |
| `--session <SESSION_ID>` | No | Explicit provider session/thread override for this run. |
| `--model <MODEL>` | No | Harness-specific model configuration. For Codex this is passed as a Codex model argument. |

SVDO Meter reads `--prompt-file` before starting the harness. Missing, unreadable, or non-UTF-8 files fail fast with a path-aware CLI error.

## `svdo-meter report`

Generates a local SVDO Trace report from append-only JSONL telemetry.

```bash
svdo-meter report ENG-142
svdo-meter report --last 7d
svdo-meter report --label plan
svdo-meter report ENG-142 --format json
svdo-meter report --last 7d --format csv
svdo-meter report ENG-142 --pricing-file pricing.json
```

### Arguments

| Argument | Required | Description |
|---|---:|---|
| `<WORK>` | No | Optional work identifier. When omitted, results are grouped by work identifier. |
| `--workspace <PATH>` | No | Workspace containing `.svdo/meter/`. Defaults to the current directory. |
| `--last <DURATION>` | No | Include only telemetry observed within a recent duration such as `7d`, `12h`, or `30m`. |
| `--label <LABEL>` | No | Include only telemetry records with this label. |
| `--format <FORMAT>` | No | Output format. Supported values: `terminal`, `json`, `csv`. Defaults to `terminal`. |
| `--pricing-file <PATH>` | No | UTF-8 JSON file containing the model pricing map. Rates are cost per 1,000,000 tokens. |

Pricing JSON is keyed by exact model identifier. Each model can provide independent per-million rates for input, cached input, and output tokens:

```json
{
  "gpt-5": {
    "input_per_million": 1.25,
    "cached_input_per_million": 0.125,
    "output_per_million": 10.0
  },
  "gpt-5-mini": {
    "input_per_million": 0.25,
    "cached_input_per_million": 0.025,
    "output_per_million": 2.0
  }
}
```

Cost estimation uses the telemetry model identity, preferring `resolved_model` and falling back to `requested_model`. If telemetry references a model that is not configured in the supplied pricing JSON, cost for that model is reported as unavailable and no default rate is invented.

## `svdo-meter telemetry`

Inspects local SVDO Meter JSONL telemetry without modifying the append-only event log.

```bash
svdo-meter telemetry sessions --workspace ~/code/app
svdo-meter telemetry runs --workspace ~/code/app
svdo-meter telemetry inspect 018f6f1b-97f1-7c04-9a96-111111111111 --workspace ~/code/app
svdo-meter telemetry inspect sess-abc123 --workspace ~/code/app
```

### Subcommands

| Command | Description |
|---|---|
| `svdo-meter telemetry sessions` | Lists discovered session identifiers with work, label, harness, run, discovery source, first-seen timestamp, and record count context. |
| `svdo-meter telemetry runs` | Lists run identifiers with work, label, harness, session association, first/last event timestamps, record count, and token completeness status. |
| `svdo-meter telemetry inspect <ID>` | Shows ordered telemetry events for a matching run ID or session ID, including event line number, timestamp, type, work, run, session, harness, label, and concise payload details. |

### Arguments

| Argument | Required | Description |
|---|---:|---|
| `--workspace <PATH>` | No | Workspace containing `.svdo/meter/`. Defaults to the current directory. |
| `<ID>` | Yes for `inspect` | Run identifier or provider session identifier to inspect. |

Telemetry inspection reads:

```text
<workspace>/.svdo/meter/*.jsonl
```

Missing or empty telemetry files return clear non-error output. Malformed JSONL lines are reported with line numbers under `Diagnostics`, and valid records remain inspectable. Token-bearing events such as `usage.reported`, `run.completed`, and `run.failed` call out missing token fields when expected components are absent.

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
<workspace>/.svdo/meter/<run-id>.jsonl
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
