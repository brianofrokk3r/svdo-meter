# SVDO Meter CLI

## Commands

SVDO Meter currently exposes these functional commands:

```text
svdo-meter run
svdo-meter eval run
svdo-meter report
svdo-meter telemetry
```

Help is available through Clap:

```bash
svdo-meter --help
svdo-meter run --help
svdo-meter eval --help
svdo-meter eval run --help
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
cargo run -p svdo-meter -- eval --help
cargo run -p svdo-meter -- eval run --help
cargo run -p svdo-meter -- report --help
cargo run -p svdo-meter -- telemetry --help
```

Build and install instructions are in [compile.md](compile.md).

## 30-Second Example

Start with a repository workspace and a ticket or work id:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --workspace ~/code/app \
  "Implement the password reset flow described in ENG-142"

svdo-meter report ENG-142 --workspace ~/code/app
```

Expected result at a high level:

- telemetry is appended under `~/code/app/.svdo/meter/<run-id>.jsonl`
- `svdo-meter report` renders a local SVDO Trace grouped by work id
- future runs for the same ticket, harness, and workspace can reuse discovered sessions when available

Example terminal report output:

```text
SVDO Trace
────────────────────────────

Work
  ENG-142

Harness
  codex

Session
  019c8a-fixture

Runs
  2

Agent Time
  18m 42s

Tokens
  Input   120,000
  Output  42,000
  Cache   32,213
  Total   194,213
```

For recommended repository rollout patterns, including pre-commit, CI, and before-and-after alignment workflows, see [Applying SVDO Meter](adoption.md).

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

Emit the same canonical events to stdout as newline-delimited JSON while preserving durable local telemetry:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --harness codex \
  --workspace ~/code/app \
  --emit ndjson \
  "Implement ENG-142" | my-company-ingester
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

Codex-specific safe options:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --harness codex \
  --codex-profile default \
  --codex-sandbox workspace-write \
  --codex-config model_reasoning_effort=high \
  "Implement ENG-142"
```

Dangerous bypass mode:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --harness codex \
  --dangerous-bypass \
  "Prototype ENG-142 in an externally sandboxed environment"
```

### Arguments

Common options:

| Argument | Required | Description |
|---|---:|---|
| `--ticket <TICKET>` | Yes | External ticket/work identifier. SVDO Meter records this as the join key for future reports or enrichment. |
| `--harness <HARNESS>` | Yes | Agent harness. Supported values: `codex`, `claude`, `gemini`, `litellm`. |
| `<PROMPT>` | Yes, unless `--prompt-file` is used | Inline prompt or work instruction forwarded to the harness. Prompts are not persisted by default. |
| `--prompt-file <PATH>` | Yes, unless `<PROMPT>` is used | UTF-8 text file whose contents are forwarded to the harness as the prompt. Cannot be combined with an inline prompt. |
| `--label <LABEL>` | No | Human-readable label copied to canonical run events. |
| `--workspace <PATH>` | No | Workspace directory passed to the harness and used as the base for `.svdo/meter/`. |
| `--session <SESSION_ID>` | No | Explicit provider session/thread override for this run. |
| `--model <MODEL>` | No | Harness-specific model configuration passed to the selected harness. |
| `--dangerous-bypass` | No | Asks the selected harness to bypass approval and sandbox protections. Maps to Codex yolo behavior or Claude Code `bypassPermissions`. |
| `--sink <SINK>` | No | Event output sink. Repeatable. Supported values: `jsonl`, `stdout`. Durable `jsonl` telemetry remains enabled by default. |
| `--emit <FORMAT>` | No | Convenience event stream format. Supported value: `ndjson`, equivalent to enabling the stdout sink. |

Codex-specific options, valid only with `--harness codex`:

| Argument | Description |
|---|---|
| `--codex-profile <NAME>` | Passes `--profile <NAME>` to Codex. |
| `--codex-sandbox <MODE>` | Passes `--sandbox <MODE>` to Codex. Supported values: `read-only`, `workspace-write`, `danger-full-access`. |
| `--codex-approve-for-me` | Passes `--approve-for-me` to Codex. |
| `--codex-yolo` | Compatibility spelling for Codex's dangerous `--dangerously-bypass-approvals-and-sandbox` flag. Also records canonical `dangerous-bypass` execution permission telemetry. |
| `--codex-config <key=value>` | Passes a repeated `--config <key=value>` override to Codex. Keys and values must be non-empty. |

SVDO Meter reads `--prompt-file` before starting the harness. Missing, unreadable, or non-UTF-8 files fail fast with a path-aware CLI error.

### LiteLLM Harness

LiteLLM runs call the LiteLLM-compatible API directly. They do not invoke a local LiteLLM CLI or start a local proxy process.

Set `LITELLM_API_KEY` in the process environment before selecting `--harness litellm`:

```bash
export LITELLM_API_KEY

svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness litellm \
  --model gpt-5 \
  --workspace ~/code/app \
  "Implement the password reset flow described in ENG-142"
```

When targeting a LiteLLM-compatible API base other than the default, set `LITELLM_API_BASE` in the environment. Do not pass API keys in prompts, command arguments, fixtures, or telemetry examples.

### Event Output Sinks

`svdo-meter run` always writes durable per-run JSONL telemetry under `.svdo/meter/` unless a future explicit disable option is added and documented.

Sink selection rules:

| Selection | Behavior |
|---|---|
| No sink flags | Write durable JSONL only. |
| `--sink jsonl` | Explicitly select durable JSONL only. |
| `--sink stdout` | Write durable JSONL and emit one NDJSON event per stdout line. |
| `--emit ndjson` | Write durable JSONL and emit one NDJSON event per stdout line. |
| `--sink stdout --emit ndjson` | Write durable JSONL and emit one stdout NDJSON stream, not duplicate stdout records. |

Unsupported sink names such as `otel`, `http`, or custom connector names are rejected during CLI parsing. Sink emit failures fail the run command; the event bus attempts every selected sink for the event before returning the first sink error.

## `svdo-meter eval run`

Runs repository alignment evals defined by the selected workspace. This command evaluates and reports results only; it does not retry, review, merge, route work, or invoke an agent orchestration flow.

Eval definitions live under:

```text
<workspace>/.svdo/evals/
```

Referenced standards live under:

```text
<workspace>/.svdo/standards/
```

Run all eval definitions in the current repository:

```bash
svdo-meter eval run
```

Run a specific eval by id, file stem, or file name:

```bash
svdo-meter eval run add-account-endpoint
svdo-meter eval run add-account-endpoint.yaml
```

Run evals for a different repository:

```bash
svdo-meter eval run --workspace ~/code/app
svdo-meter eval run add-account-endpoint --workspace ~/code/app
```

Select an output format:

```bash
svdo-meter eval run --format terminal
svdo-meter eval run --format json
svdo-meter eval run --format csv
```

Run judge checks with an LLM judge:

```bash
svdo-meter eval run --harness codex --model gpt-5
svdo-meter eval run api-contract --harness codex --model gpt-5
svdo-meter eval run --harness claude --model sonnet
svdo-meter eval run --harness litellm --model gpt-5
```

LiteLLM judge runs use the same direct LiteLLM-compatible API access as `svdo-meter run`. Set `LITELLM_API_KEY` in the process environment before selecting `--harness litellm`; svdo-meter does not read LiteLLM credentials from persisted configuration.

### Arguments

| Argument | Required | Description |
|---|---:|---|
| `<EVAL>` | No | Eval id, file stem, or file name. When omitted, all `.yaml` and `.yml` eval definitions under `.svdo/evals/` run. |
| `--workspace <PATH>` | No | Repository workspace containing `.svdo/evals/`. Defaults to the current directory. |
| `--harness <HARNESS>` | No | Harness used for `type: judge` checks. Supported values: `codex`, `claude`, `gemini`, `litellm`. |
| `--model <MODEL>` | No | Model passed to the judge harness, such as `gpt-5`. Requires `--harness`. |
| `--judge-command <PROGRAM>` | No | Custom program used for `type: judge` checks. Receives the judge request JSON path as its final argument. |
| `--judge-arg <ARG>` | No | Extra argument passed to `--judge-command` before the judge request path. Repeat for multiple arguments. |
| `--format <FORMAT>` | No | Output format. Supported values: `terminal`, `json`, `csv`. Defaults to `terminal`. |

### Eval Definitions

Eval definitions are YAML files. Supported top-level fields:

| Field | Required | Description |
|---|---:|---|
| `id` | Yes | Stable eval identifier. A requested `<EVAL>` can match this value. |
| `task` | Yes | Human-readable task or objective being evaluated. |
| `checks` | Yes | Ordered list of command or judge checks. |
| `threshold` | No | Minimum aggregate score required to pass. Defaults to `1.0`. |

Supported check fields:

| Field | Required | Description |
|---|---:|---|
| `id` | Yes | Stable check identifier. |
| `type` | Yes | Check kind. Supported values: `command`, `judge`. |
| `command` | Yes for `command` | Shell command executed from the workspace directory. |
| `required` | No | When `true`, a failed check hard-fails the eval regardless of aggregate score. Defaults to `false`. |
| `weight` | No | Numeric weight used in the aggregate score. Defaults to `1.0`. |
| `standard` | No | Referenced standard id or file for judge checks. Resolved from `.svdo/standards/`. |

Example:

```yaml
id: add-account-endpoint

task: |
  Add GET /accounts/{account_id}.

checks:
  - id: tests
    type: command
    command: pytest
    required: true

  - id: lint
    type: command
    command: ruff check .
    required: true

  - id: architecture
    type: judge
    standard: api-architecture
    weight: 0.4

threshold: 0.85
```

Command checks report success or failure, exit status, duration, and captured failure output. Non-required command checks contribute to the weighted score. Required command check failures cause the eval to fail even when the weighted score is above the threshold.

Judge checks are represented in the schema and result model. Without `--harness` or `--judge-command`, judge checks resolve their referenced standards and report a skipped result with a clear reason. Skipped judge checks do not block deterministic command checks from running.

When `--harness codex` is set, each judge check sends the eval task and resolved standard contents to `codex exec --json`, asks the model to return only a JSON score, and reads the JSON score from the Codex output stream. When `--harness claude` is set, the same judge request is sent through `claude -p` with `--output-format stream-json`. When `--harness litellm` is set, the judge request is sent directly to the LiteLLM-compatible API using `LITELLM_API_KEY`.

`--judge-command` remains available for custom judge integrations. Each judge check writes a temporary request JSON file and invokes the configured program directly:

```text
<PROGRAM> <JUDGE_ARG>... <REQUEST_JSON_PATH>
```

The same path is also available as `SVDO_METER_JUDGE_REQUEST`. The request includes the eval id, task, check id, selected workspace, referenced standard name/path, and standard contents when a standard is configured. The judge program must print a JSON object to stdout:

```json
{
  "score": 0.9,
  "passed": true,
  "violations": [],
  "model": "gpt-5",
  "harness": "local-judge"
}
```

`score` must be between `0.0` and `1.0`. `passed` is optional; when omitted, only `score: 1.0` is treated as a passing check. `violations`, `output`, `token_usage`, `model`, `harness`, and `session_id` are optional. A non-zero judge exit status fails the check and includes captured output in the eval report.

### Results

Each eval result includes:

- Overall score
- Pass/fail result
- Individual check scores or outcomes
- Violations or failure reasons
- Duration
- Token usage, when available
- Model or harness, when available
- Session ID, when available

Terminal output is intended for humans and highlights pass/fail status, score, failed checks, and violations. JSON output includes the full structured result model. CSV output is pipe-friendly and emits one row per check with repeated eval-level fields.

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
codex exec --json -C <workspace> [--model <model>] [--profile <profile>] [--sandbox <mode>] [--approve-for-me] [--dangerously-bypass-approvals-and-sandbox] [--config <key=value>...] <prompt>
```

When `--prompt-file <path>` is used, SVDO Meter reads the file and passes the resolved text as `<prompt>`.

Resume shape when a session is known:

```text
codex exec --json -C <workspace> [codex options...] resume <session_id> <prompt>
```

Model shape:

```text
codex exec --json -C <workspace> --model <model> <prompt>
```

SVDO Meter validates Codex-specific flags before spawning Codex. Codex-specific flags are rejected with non-Codex harnesses. The provider-neutral `--dangerous-bypass` flag maps to the same Codex argv flag as `--codex-yolo`.

## Claude Code Harness

The Claude adapter invokes the local `claude` CLI with explicit process arguments and sets the child process current directory to `--workspace` when supplied. Install and authenticate Claude Code separately, and confirm `claude --help` works before running real measured Claude sessions.

New non-interactive run shape:

```text
claude -p <prompt> --output-format stream-json --verbose
```

Model shape:

```text
claude -p <prompt> --output-format stream-json --verbose --model <model>
```

Generic session override shape:

```text
claude -p <prompt> --output-format stream-json --verbose --resume <session_id>
```

Continue and explicit resume shapes:

```text
claude -p <prompt> --output-format stream-json --verbose --continue
claude -p <prompt> --output-format stream-json --verbose --resume <session_id_or_name>
claude -p <prompt> --output-format stream-json --verbose --resume <session_id_or_name> --fork-session
```

Supported Claude-specific options:

| SVDO Meter option | Claude Code flag |
|---|---|
| `--claude-continue` | `--continue` |
| `--claude-resume <SESSION>` | `--resume <SESSION>` |
| `--claude-session-id <UUID>` | `--session-id <UUID>` |
| `--claude-fork-session` | `--fork-session` |
| `--claude-permission-mode <MODE>` | `--permission-mode <MODE>` |
| `--claude-allowed-tool <RULE>` | `--allowed-tools <RULE>` |
| `--claude-disallowed-tool <RULE>` | `--disallowed-tools <RULE>` |
| `--claude-add-dir <PATH>` | `--add-dir <PATH>` |
| `--claude-mcp-config <PATH_OR_JSON>` | `--mcp-config <PATH_OR_JSON>` |
| `--claude-strict-mcp-config` | `--strict-mcp-config` |
| `--claude-settings <PATH_OR_JSON>` | `--settings <PATH_OR_JSON>` |
| `--claude-setting-sources <SOURCES>` | `--setting-sources <SOURCES>` |
| `--claude-system-prompt <TEXT>` | `--system-prompt <TEXT>` |
| `--claude-system-prompt-file <PATH>` | `--system-prompt-file <PATH>` |
| `--claude-append-system-prompt <TEXT>` | `--append-system-prompt <TEXT>` |
| `--claude-append-system-prompt-file <PATH>` | `--append-system-prompt-file <PATH>` |
| `--claude-max-turns <TURNS>` | `--max-turns <TURNS>` |
| `--claude-max-budget-usd <USD>` | `--max-budget-usd <USD>` |

Validation rules:

- Choose only one resume mode: `--session`, `--claude-resume`, or `--claude-continue`.
- `--claude-session-id` must be a valid UUID, starts or associates that specific new conversation, is recorded as a local `session.discovered` event even if Claude Code does not echo it, and cannot combine with resume or continue.
- `--claude-fork-session` requires a resume or continue mode.
- `--claude-system-prompt` and `--claude-system-prompt-file` are mutually exclusive.
- `--claude-strict-mcp-config` requires at least one `--claude-mcp-config`.
- `--claude-max-turns` must be greater than zero.

Permission-sensitive behavior is direct. SVDO Meter never enables `bypassPermissions` or equivalent behavior automatically; if `--claude-permission-mode bypassPermissions` or provider-neutral `--dangerous-bypass` is supplied, it maps directly to Claude Code `--permission-mode bypassPermissions`.

Out of scope for this harness version: Claude Code background mode, cloud/environment dispatch, worktree/tmux management, plugin management, input stream driving, prompt suggestions, hook events, partial message streaming, subagent transcript forwarding, arbitrary passthrough flags, and interactive TUI sessions.

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

This durable JSONL sink remains active when stdout event streaming is enabled.

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

### JSONL Event Shape

Every line is a complete JSON object. The common envelope is intended to be practical and durable for local reporting:

| Field | Description |
|---|---|
| `schema_version` | Integer schema marker for the serialized event. Current emitted version is `1`. |
| `event_id` | Unique event identifier. |
| `event_type` | Canonical event name such as `run.started` or `usage.reported`. |
| `occurred_at` | UTC timestamp for when the event occurred. |
| `observed_at` | UTC timestamp for when SVDO Meter observed the event. |
| `run_id` | SVDO Meter run identifier shared by events from one measured run. |
| `ticket_id` | Required work identifier supplied with `--ticket`; reports group by this value. |
| `label` | Optional human-readable label copied from `--label`. |
| `harness` | Harness name, currently `codex` or `claude`. |
| `requested_model` | Optional model requested through the CLI. |
| `resolved_model` | Optional provider-resolved model name when the harness reports one. |
| `session_id` | Optional provider session/thread id. |
| `workspace` | Optional workspace path used as the telemetry base. |
| `payload` | Event-specific payload envelope. |

The payload uses a tagged shape:

```json
{
  "payload": {
    "type": "usage_reported",
    "data": {
      "input_tokens": 120000,
      "cached_input_tokens": 32213,
      "output_tokens": 42000
    }
  }
}
```

The payload `type` is the snake_case form of the canonical `event_type`. For example, `run.started` uses `run_started`, and `session.discovered` uses `session_discovered`.

Token fields are optional. A missing token component means the harness did not report that component; it is distinct from an explicit `0`.

For `run.started`, the payload includes `prompt_recorded` and may include `execution_permission` when SVDO Meter can determine the requested execution posture. Current `execution_permission` values are `standard` and `dangerous-bypass`.

Terminal events use `run_completed` or `run_failed` payloads with `metrics`. Metrics currently include wall time, active time, command/tool time, turn count, provider event count, command counts, file-change counts, tool calls, errors, and token usage.

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

### Event Payload Guide

| Event type | Typical payload data |
|---|---|
| `run.started` | `prompt_recorded`, optional `execution_permission`. |
| `session.discovered` | `source`, with `session_id` in the common envelope. |
| `harness.event` | Provider event name and whether raw payload retention was enabled. |
| `usage.reported` | Optional token fields: input, cached input, cache write, output, and reasoning tokens. |
| `command.started` | Optional command id and command kind. |
| `command.completed` | Optional command id, success flag, optional exit code, optional duration. |
| `files.changed` | Count of changed files. |
| `tool.started` | Optional tool id and tool name. |
| `tool.completed` | Optional tool id/name, success flag, optional duration. |
| `run.completed` | Final metrics and optional process exit code. |
| `run.failed` | Final metrics, failure reason, and optional process exit code. |

Consumers should prefer the canonical fields above and tolerate missing optional fields. The exact provider-specific source event names inside `harness.event` are adapter details and may vary as Codex or Claude Code change their streams.

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
