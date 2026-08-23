# SVDO Meter — CLI Interface and Architecture Proposal

## Overview

`svdo-meter` is a thin instrumentation layer for measuring agentic engineering work.

Its job is intentionally narrow:

1. Associate a unit of work with an agent CLI session.
2. Execute or resume that CLI.
3. Listen to the events exposed by the CLI.
4. Record objective execution metrics.
5. Persist those events in an append-only, ingestible format.
6. Generate reports from the recorded telemetry.

`svdo-meter` should **not** become an agent orchestration framework, ticketing system, or model abstraction layer.

The core principle is:

> Measure the work, preserve the events, and keep reporting separate from execution.

---

# Goals

The CLI should support:

- A ticket/work identifier
- A human-readable label
- A CLI/harness
- A session identifier when required for resumption
- A workspace when required by the CLI
- CLI-specific configuration, including model selection
- Multiple runs associated with the same ticket
- Multiple supported agent CLIs
- Append-only telemetry storage
- Reports by run, ticket, or period
- Future export/connectors to third-party systems

Example supported CLIs may include:

- Codex
- Claude
- Gemini
- Other agentic CLIs added later

Models are configuration owned by each CLI adapter.

For example:

```text
CodexAdapter
  └── CodexConfig
      └── model = "..."

ClaudeAdapter
  └── ClaudeConfig
      └── model = "..."
```

Do **not** create a separate adapter per model.

---

# Non-Goals

`svdo-meter` should not:

- Own the ticket
- Store story points as canonical ticket metadata
- Reimplement agent conversation state
- Reimplement the CLI's planning or tool execution
- Abstract all models behind a common model interface
- Require Jira, GitHub, SVDO, Linear, or any other work-management system
- Make remote connectors part of the critical execution path
- Depend on a live network service to record a run

The ticket identifier is the join key used later to enrich telemetry.

For example:

```text
SVDO Meter
  ENG-142
      │
      ├── observed agent time
      ├── observed tokens
      ├── runs
      └── files changed

Jira / SVDO
  ENG-142
      │
      ├── story points = 5
      └── estimated human hours = 8
```

A reporting or analytics layer can join those datasets later.

---

# CLI Interface

## Basic Run

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --workspace ~/code/app \
  "Implement the password reset flow described in ENG-142"
```

Conceptually, `svdo-meter` runs:

```bash
codex exec --json \
  -C ~/code/app \
  "Implement the password reset flow described in ENG-142"
```

while observing the process and recording telemetry.

---

# Automatic Session Association

On the first execution, the harness may create a session/thread identifier.

For example:

```text
ticket
ENG-142

        ↓

Codex thread
019c8a42-f72...
```

`svdo-meter` should capture this association automatically when the CLI exposes the session identifier.

The engineer should not normally need to know the underlying session ID.

A later command:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --workspace ~/code/app \
  "Add the missing forgot-password UI."
```

can resolve:

```text
ENG-142
  ↓
codex
  ↓
019c8a42-f72...
```

and invoke the equivalent of:

```bash
codex exec --json \
  -C ~/code/app \
  resume 019c8a42-f72... \
  "Add the missing forgot-password UI."
```

---

# Explicit Session Override

The CLI should also allow a session to be supplied explicitly:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --session 019c8a42-f72... \
  --workspace ~/code/app \
  "Fix the remaining tests"
```

This is useful when:

- importing an existing session
- reconstructing state
- overriding an incorrect association
- attaching telemetry to a session created before `svdo-meter`

---

# Harness-Specific Configuration

Models should be CLI-specific configuration rather than part of the top-level adapter hierarchy.

For example:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --harness codex \
  --model gpt-5 \
  --workspace . \
  "Implement ENG-142"
```

Internally:

```rust
HarnessConfig::Codex(CodexConfig {
    model: Some("gpt-5".into()),
    profile: None,
})
```

A Claude adapter may interpret model configuration differently:

```rust
HarnessConfig::Claude(ClaudeConfig {
    model: Some("claude-sonnet".into()),
    permission_mode: None,
})
```

The top-level execution engine should not need to understand the meaning of those options.

---

# Suggested CLI Commands

The initial CLI can stay extremely small:

```text
svdo-meter
├── run
├── report
└── export
```

`export` can be added after the core measurement/reporting path is stable.

---

# `svdo-meter run`

Starts or resumes measured work.

Example:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Password reset" \
  --harness codex \
  --workspace . \
  "Implement the ticket"
```

Suggested inputs:

```text
--ticket       required
--label        optional
--harness      required
--workspace    optional
--session      optional
--model        optional / harness-specific
```

Additional harness-specific arguments can later be namespaced or passed through.

For example:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --harness codex \
  --model gpt-5 \
  -- \
  "Implement ENG-142"
```

---

# `svdo-meter report`

Reports should be generated entirely from persisted telemetry.

Single ticket:

```bash
svdo-meter report ENG-142
```

Recent work:

```bash
svdo-meter report --last 7d
```

JSON output:

```bash
svdo-meter report ENG-142 --format json
```

Local token cost estimation:

```bash
svdo-meter report ENG-142 --pricing-file pricing.json
```

Pricing files are UTF-8 JSON maps keyed by exact model identifier. Rates are expressed as cost per 1,000,000 tokens with independent fields for input, cached input, and output tokens:

```json
{
  "gpt-5": {
    "input_per_million": 1.25,
    "cached_input_per_million": 0.125,
    "output_per_million": 10.0
  }
}
```

If telemetry references a model without configured pricing, the report should mark cost as unavailable rather than inventing defaults.

Possible future formats:

```text
terminal
json
csv
html
```

The canonical aggregation logic should remain independent from the presentation format.

---

# `svdo-meter export`

Future connectors should read from the durable event log rather than participate directly in agent execution.

Examples:

```bash
svdo-meter export --connector bigquery
```

```bash
svdo-meter export --connector datadog
```

```bash
svdo-meter export --connector custom-http
```

The important rule is:

```text
BAD

Agent CLI
   ↓
SVDO Meter
   ↓
Remote Connector
   ↓
Local persistence
```

A connector outage could break execution or lose telemetry.

Instead:

```text
GOOD

Agent CLI
   ↓
SVDO Meter
   ↓
Local append-only log
   │
   ├── reports
   └── connectors/export
```

Local durable persistence should always happen first.

---

# Runtime Architecture

The runtime should remain thin:

```text
Engineer
   │
   ▼
svdo-meter
   │
   ├── resolve work/session
   ├── start or resume harness
   ├── listen to events
   ├── normalize known metrics
   ├── persist events
   └── forward CLI output
   │
   ▼
Agent CLI
   │
   ▼
Workspace
```

The adapter does not manage the agent.

It measures the agent.

---

# Repository Architecture

A reasonable initial Cargo workspace:

```text
svdo-meter/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── deny.toml
│
├── crates/
│   ├── meter-core/
│   │   ├── ids
│   │   ├── events
│   │   ├── metrics
│   │   └── domain types
│   │
│   ├── meter-engine/
│   │   ├── run lifecycle
│   │   ├── session association
│   │   └── ports / traits
│   │
│   ├── meter-adapters/
│   │   ├── harnesses/
│   │   │   ├── codex
│   │   │   ├── claude
│   │   │   └── gemini
│   │   ├── storage/
│   │   │   └── jsonl
│   │   └── connectors/
│   │
│   ├── meter-report/
│   │   ├── reducers
│   │   ├── run reports
│   │   └── ticket reports
│   │
│   └── svdo-meter/
│       ├── CLI
│       ├── config
│       └── dependency wiring
│
└── tests/
    └── fixtures/
        ├── codex/
        ├── claude/
        └── gemini/
```

Do not create a separate crate for every harness initially.

Start with harness modules under `meter-adapters`.

Split adapters into separate crates later only if justified by:

- heavy dependencies
- compile-time impact
- platform-specific code
- independent ownership
- substantial implementation complexity

---

# Core Domain Types

The core should describe concepts owned by SVDO Meter rather than provider-specific concepts.

Example:

```rust
struct WorkRef {
    ticket_id: TicketId,
    label: Option<String>,
}

struct RunId(Uuid);

struct SessionRef {
    harness: HarnessKind,
    provider_session_id: String,
}

struct RunContext {
    run_id: RunId,
    work: WorkRef,
    session: Option<SessionRef>,
    workspace: Option<PathBuf>,
}
```

Use typed identifiers instead of passing raw strings everywhere.

---

# Harness Adapter Boundary

Each supported CLI implements the same high-level capability:

```rust
trait HarnessAdapter {
    async fn run(
        &self,
        request: HarnessRunRequest,
        events: EventSender,
    ) -> Result<HarnessRunResult, HarnessError>;
}
```

The adapter owns:

- CLI invocation
- CLI-specific arguments
- model configuration
- session creation
- session resumption
- provider event parsing
- provider-specific metrics extraction

The engine owns:

- run lifecycle
- durable event persistence
- work/session association
- reporting-compatible canonical events

---

# Models Are CLI Configuration

Avoid:

```text
CodexGpt5Adapter
CodexGpt4Adapter
ClaudeSonnetAdapter
ClaudeOpusAdapter
```

Prefer:

```text
CodexAdapter
  + CodexConfig

ClaudeAdapter
  + ClaudeConfig

GeminiAdapter
  + GeminiConfig
```

Example:

```rust
enum HarnessConfig {
    Codex(CodexConfig),
    Claude(ClaudeConfig),
    Gemini(GeminiConfig),
}
```

This avoids a combinatorial adapter hierarchy.

---

# Event Architecture

The event stream is the most important interface in the system.

The preferred model is:

```text
Provider CLI
     │
     ▼
Harness Adapter
     │
     ├── preserve source event
     └── normalize understood metrics
     │
     ▼
Canonical Meter Event
     │
     ▼
Append-only JSONL
```

The log should contain enough information to:

- reconstruct every run
- reconstruct session associations
- aggregate ticket metrics
- replay historical telemetry
- support new reporting logic later
- export to external systems

---

# Why JSONL

The default storage format should be append-only JSON Lines:

```text
.svdo/
└── meter.jsonl
```

JSONL is useful because it is:

- append-friendly
- streamable
- grep-friendly
- easy to replay
- easy to parse
- easy to load into BigQuery
- easy to ingest into Postgres
- easy to ship to object storage
- easy to ingest into log/observability systems

Each line should represent one immutable event.

---

# Event Envelope

Every event should contain common metadata.

Example:

```json
{
  "schema_version": "1.0",
  "event_id": "evt_0192...",
  "event_type": "run.started",
  "occurred_at": "2026-08-20T20:10:00Z",

  "run_id": "run_0192...",
  "ticket_id": "ENG-142",
  "label": "Add password reset flow",

  "harness": "codex",
  "session_id": "019c8a42-f72...",
  "workspace": "/Users/example/code/app"
}
```

Important dimensions should be repeated on events where practical.

This makes downstream ingestion much easier than requiring every consumer to reconstruct joins before querying basic facts.

---

# Suggested Event Types

Initial canonical events:

```text
run.started
session.discovered
harness.event
usage.reported
command.started
command.completed
files.changed
tool.started
tool.completed
run.completed
run.failed
```

Not every provider needs to support every normalized event.

Unknown provider events should still be preservable.

---

# Raw Provider Events

When a harness exposes structured events, SVDO Meter should be able to preserve them.

Example envelope:

```json
{
  "schema_version": "1.0",
  "event_id": "evt_83912",
  "event_type": "harness.event",
  "received_at": "2026-08-20T20:14:32.421Z",

  "ticket_id": "ENG-142",
  "run_id": "run_0192",
  "session_id": "019c8a42-f72...",

  "harness": "codex",
  "source_event": "item.completed",

  "payload": {
    "...": "provider event"
  }
}
```

However, raw content may contain:

- source code
- prompts
- agent messages
- command output
- tool results
- filesystem paths
- secrets

Therefore raw provider payload retention should be configurable and should **not** necessarily be enabled by default.

A safer default is:

> Record objective telemetry and structural events by default; preserve raw content only when explicitly enabled.

---

# Unknown Events

Provider adapters should be forward-compatible.

Do not fail because a CLI introduces a new event type.

Prefer loose transport parsing:

```rust
#[derive(Debug, Deserialize, Serialize)]
struct ProviderEvent {
    #[serde(rename = "type")]
    event_type: String,

    #[serde(flatten)]
    data: serde_json::Value,
}
```

Then:

```rust
match event.event_type.as_str() {
    "turn.completed" => normalize_turn_completed(&event),
    "item.completed" => normalize_item_completed(&event),
    _ => {
        // Preserve event if configured.
        // Do not fail the run.
    }
}
```

The principle is:

> Measure what we understand today and preserve compatibility with what we do not understand yet.

---

# Example First Run

Command:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --workspace ~/code/app \
  "Implement the password reset flow"
```

First event:

```json
{
  "schema_version": "1.0",
  "event_type": "run.started",
  "occurred_at": "2026-08-20T20:10:00Z",
  "run_id": "run_01",
  "ticket_id": "ENG-142",
  "label": "Add password reset flow",
  "harness": "codex",
  "workspace": "/Users/example/code/app"
}
```

Codex reports its thread/session:

```json
{
  "schema_version": "1.0",
  "event_type": "session.discovered",
  "occurred_at": "2026-08-20T20:10:01Z",
  "run_id": "run_01",
  "ticket_id": "ENG-142",
  "harness": "codex",
  "session_id": "019c8a42-f72..."
}
```

Usage event:

```json
{
  "event_type": "usage.reported",
  "run_id": "run_01",
  "ticket_id": "ENG-142",
  "session_id": "019c8a42-f72...",
  "occurred_at": "2026-08-20T20:18:01Z",
  "data": {
    "input_tokens": 82214,
    "cached_input_tokens": 65536,
    "output_tokens": 6241,
    "reasoning_output_tokens": 2831
  }
}
```

Completion event:

```json
{
  "event_type": "run.completed",
  "run_id": "run_01",
  "ticket_id": "ENG-142",
  "session_id": "019c8a42-f72...",
  "occurred_at": "2026-08-20T20:18:02Z",
  "success": true,
  "metrics": {
    "wall_time_ms": 482000,
    "command_time_ms": 54000,

    "input_tokens": 82214,
    "cached_input_tokens": 65536,
    "output_tokens": 6241,
    "reasoning_output_tokens": 2831,

    "files_changed": 4,
    "command_executions": 6,
    "failed_commands": 2,
    "turns": 1
  }
}
```

---

# Multiple Runs Per Ticket

A ticket may require multiple interactions with the same session.

Example:

```text
ENG-142
│
├── session 019c8a42...
│
├── run_01
│   ├── 8m 02s
│   ├── 88K tokens
│   └── 4 files
│
└── run_02
    ├── 3m 41s
    ├── 42K tokens
    └── 3 files
```

Each interaction receives its own `run_id`.

The session may remain the same.

This distinction is important:

```text
Ticket
  └── one or more Sessions
          └── one or more Runs
```

---

# Session Registry

A small local projection may accelerate session lookup:

```json
{
  "ENG-142": {
    "harness": "codex",
    "session_id": "019c8a42-f72...",
    "workspace": "/Users/example/code/app"
  }
}
```

However, the registry should be treated as a rebuildable index.

The append-only event log should remain the canonical source of truth.

If the registry is deleted, it should be possible to replay:

```text
session.discovered
```

events and reconstruct the mapping.

---

# Metrics

SVDO Meter should record objective measurements rather than subjective productivity conclusions.

Useful metrics include:

```text
wall time
active turn time
command time
tool time
turn count
provider event count

input tokens
cached input tokens
cache-write tokens
output tokens
reasoning tokens

commands executed
failed commands
files changed
tool calls
errors
run success
```

Provider adapters should record the richest token breakdown exposed by that CLI.

Do not collapse all usage into a single token number if the CLI exposes separate counters.

---

# Time Metrics

At minimum, distinguish:

```text
Wall Time
process/run start → run end

Agent/Turn Time
time associated with active agent turns

Tool Time
time spent executing commands/tools
```

Example:

```text
37 minutes wall clock

├── 18m agent/model activity
├── 11m tests / shell commands
├──  3m other tools
└──  5m idle / user interaction
```

The exact precision will vary by harness.

The core event model should support richer timelines without requiring every adapter to provide them.

---

# Reporting Architecture

Reports should be deterministic projections of the event log.

```text
meter.jsonl
    │
    ▼
Event Parser
    │
    ▼
RunReducer
    │
    ▼
RunSummary
    │
    ▼
TicketReducer
    │
    ▼
TicketSummary
    │
    ▼
Report Renderer
```

Example:

```rust
struct RunSummary {
    run_id: String,
    ticket_id: String,
    session_id: Option<String>,
    harness: String,

    wall_time_ms: u64,

    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_tokens: u64,

    files_changed: u32,
    commands: u32,
    failed_commands: u32,

    success: bool,
}
```

Ticket aggregation:

```rust
struct TicketSummary {
    ticket_id: String,
    label: Option<String>,

    runs: u32,
    sessions: u32,

    total_wall_time_ms: u64,
    total_tokens: u64,

    files_changed: u32,
    failed_commands: u32,
}
```

---

# Example Ticket Report

```text
ENG-142 — Password Reset

Execution
─────────────────────────────
Harness                    Codex
Sessions                       1
Runs                           2

Agent Time                11m 43s

Input Tokens             118,316
Cached Input              93,646
Output Tokens             12,075
Estimated Cost             $0.47

Commands                      10
Failed Commands                2
Files Changed                  7
```

This report contains observed SVDO Meter facts. Estimated cost is a local report projection that exists only when the user supplies pricing configuration.

---

# External Ticket Enrichment

Story points and estimated human hours should come from external work systems.

Example enrichment:

```json
{
  "ticket_id": "ENG-142",
  "story_points": 5,
  "estimated_human_hours": 8
}
```

The join is:

```text
meter.ticket_id == external.ticket_id
```

After enrichment, a report can derive:

```text
Agent Minutes / Story Point
Tokens / Story Point
Cost / Story Point
Human Estimated Hours / Agent Hours
Execution Productivity
```

Example:

```text
ENG-142 — Password Reset

Estimated Work
─────────────────────────────
Story Points                    5
Human Estimate                  8h

Observed Agent Work
─────────────────────────────
Agent Runs                      2
Agent Time                 11m 43s
Tokens                    130,391
Files Changed                   7

Derived
─────────────────────────────
Agent Minutes / SP           2.35
Tokens / SP                26,078
Execution Productivity       41.0x
```

These derived metrics should live in reporting/analytics rather than in the execution adapter.

---

# Effort Classification

Effort classification should also be a report-layer concern.

SVDO Meter records facts such as:

```text
runs
agent minutes
tokens
files changed
commands
failed commands
directories touched
tool calls
```

A later classifier can derive:

```json
{
  "ticket_id": "ENG-142",
  "observed_effort": {
    "class": "medium",
    "score": 0.47,
    "scope": 0.42,
    "complexity": 0.38,
    "rework": 0.21,
    "uncertainty": 0.53
  }
}
```

Do not make the execution adapter decide that a ticket is "5 story points."

Story points are an external estimate.

Observed effort is a derived measurement.

---

# Connector Architecture

Future connectors should operate against canonical events or report projections.

Possible destinations:

```text
BigQuery
Postgres
Datadog
OpenTelemetry
S3 / GCS
Custom HTTP APIs
SVDO Cloud
```

Connector interface:

```rust
trait Connector {
    async fn export(
        &self,
        events: &[MeterEvent],
    ) -> Result<ExportResult, ConnectorError>;
}
```

Connectors should support idempotency using:

```text
event_id
```

A connector can safely retry without creating duplicate telemetry.

---

# Connector Reliability

Remote connectors must not be part of the measured run's critical path.

Preferred flow:

```text
Agent
  ↓
SVDO Meter
  ↓
meter.jsonl
  ↓
Connector Worker / Export Command
  ↓
Remote Destination
```

If BigQuery is unavailable, the run still completes.

If Datadog is unavailable, the run still completes.

Telemetry can be exported again later.

---

# Security Principles

SVDO Meter runs in developer environments and may observe highly sensitive data.

Potential inputs include:

- source code
- agent-generated text
- command arguments
- command output
- environment variables
- filesystem paths
- API keys
- tokens
- tool arguments/results

Therefore:

## Do not shell-interpolate commands

Prefer:

```rust
Command::new("codex")
    .arg("exec")
    .arg("--json")
    .arg(prompt);
```

Avoid:

```rust
Command::new("sh")
    .arg("-c")
    .arg(format!("codex exec --json '{}'", prompt));
```

---

## Treat Provider Events as Untrusted

Provider JSON must not be assumed safe.

Protect against:

- malformed JSON
- extremely large events
- unexpected event schemas
- terminal escape sequences
- invalid UTF-8 where relevant
- malicious command output

Unknown events should not crash the run.

---

## Do Not Log Sensitive Content by Default

Do not automatically persist:

- prompts
- model responses
- shell output
- environment variables
- tool results
- secrets

Default telemetry should be metadata and objective metrics.

Raw-event/content retention should be explicit configuration.

---

## Bound Memory Usage

Do not:

```rust
read_to_end()
```

on an unbounded agent process stream.

Prefer line/event streaming with bounded channels.

---

## Filesystem Safety

Use:

```text
Path
PathBuf
OsString
```

instead of assuming filesystem paths are UTF-8 `String`s.

Avoid writing sensitive local state into repository paths without considering repository trust and version control.

---

## Connector Security

Connectors should:

- use TLS
- verify certificates
- keep credentials outside telemetry
- never debug-print secrets
- use explicit redaction policies
- use parameterized SQL where applicable
- escape CSV/HTML output correctly
- support least-privilege credentials

---

# Testing Strategy

Each harness should have captured event fixtures.

Example:

```text
tests/
└── fixtures/
    ├── codex/
    │   ├── successful_run.jsonl
    │   ├── failed_command.jsonl
    │   ├── resumed_session.jsonl
    │   └── unknown_event.jsonl
    ├── claude/
    └── gemini/
```

Adapter tests should replay fixtures rather than requiring live agent CLIs during normal CI.

Important cases:

- first run
- session discovery
- session resume
- failed run
- command failure
- malformed provider event
- unknown provider event
- missing token usage
- partial process termination
- large event
- interrupted run
- multiple runs for one ticket

---

# Implementation Principles

Prefer:

| Use | Instead of |
|---|---|
| One adapter per CLI | One adapter per model |
| CLI-specific typed config | Generic config maps everywhere |
| Typed IDs | Raw strings throughout the codebase |
| `PathBuf` / `OsString` | `String` for paths |
| `thiserror` in libraries | String-based errors |
| `anyhow` at application boundaries | `anyhow` everywhere |
| Bounded Tokio channels | Unbounded queues |
| `Instant` for elapsed duration | Wall-clock subtraction |
| `Command` with explicit args | `sh -c` |
| Append-only local persistence | Remote writes before local durability |
| Deterministic reducers | Mutable analytics scattered through adapters |
| Fixture-based adapter tests | Live provider calls in standard CI |
| Unknown-event tolerance | Exhaustive provider enums that break on new events |
| Objective telemetry | Story-point estimation inside the meter |

---

# Recommended Initial Scope

## v0.1

Implement:

```text
svdo-meter run
```

with:

- Codex adapter
- ticket identifier
- label
- workspace
- session capture/resumption
- model configuration
- append-only JSONL
- run timing
- token measurements
- command/file metrics where exposed
- unknown event tolerance

---

## v0.2

Implement:

```text
svdo-meter report ENG-142
svdo-meter report --last 7d
```

with:

- run aggregation
- ticket aggregation
- terminal output
- JSON output

---

## v0.3

Add additional harnesses:

```text
Claude
Gemini
```

without changing the canonical event/report contract.

---

## v0.4

Add external enrichment:

```text
story points
estimated human hours
team
epic
project
```

through files or ticket-system integrations.

---

## v0.5

Add connector/export support:

```text
BigQuery
Postgres
Datadog
OpenTelemetry
SVDO Cloud
```

---

# Final Architectural Principle

The intended architecture is:

```text
                SVDO METER

ticket + label + harness + workspace
                    │
                    ▼
               Agent Session
                    │
                    ▼
              Harness Adapter
                    │
          ┌─────────┴──────────┐
          │                    │
      Source Events       Measurements
          │                    │
          └─────────┬──────────┘
                    ▼
             Append-only JSONL
                    │
           ┌────────┴─────────┐
           ▼                  ▼
        Reports           Connectors
```

The core rule is:

> Provider-specific data enters through a CLI adapter, becomes durable telemetry, and everything else is a replayable projection of that log.

This keeps `svdo-meter` small enough to remain a utility while still supporting multiple CLIs, CLI-specific model configuration, rich measurement, reporting, and future third-party integrations.
