# Feature Specification: First-Class Repository Alignment Evals

## Overview

Add an `svdo-meter eval run` workflow for repository-defined alignment evaluations stored under `.svdo/evals/`. Developers can run a named eval or all evals from the CLI, using the same output format conventions already exposed by `svdo-meter`.

## User Stories

1. As a developer, I can run `svdo-meter eval run <eval>` to execute one eval definition by id or file name.
2. As a developer, I can run `svdo-meter eval run` to execute every eval definition configured under `.svdo/evals/`.
3. As a developer, I can request terminal, JSON, or CSV output using the existing `--format` convention.
4. As a repository maintainer, I can define deterministic command checks and judge-style checks in YAML.
5. As a repository maintainer, I can reference repository standards from `.svdo/standards/` for judge checks.

## Functional Requirements

### Eval Discovery

- Eval definitions are loaded from `.svdo/evals/` under the selected workspace.
- A specific eval argument resolves first by matching `id`, then by file stem, then by exact file name.
- Omitting the eval argument runs all discovered eval definitions.
- Supported eval definition file extensions are `.yaml` and `.yml`.

### Eval Schema

Each eval definition supports:

- `id`: stable eval identifier.
- `task`: human-readable task prompt or objective.
- `checks`: ordered list of checks.
- `threshold`: optional pass threshold, defaulting to `1.0`.

Each check supports:

- `id`: stable check identifier.
- `type`: `command` or `judge`.
- `command`: shell command for command checks.
- `required`: optional boolean, defaulting to `false`.
- `weight`: optional numeric score weight, defaulting to `1.0`.
- `standard`: optional referenced standard id or file for judge checks.

### Execution

- Command checks execute in the workspace directory.
- Command checks report success, failure, exit status, duration, and captured failure output.
- Required command check failures hard-fail the eval regardless of weighted score.
- Non-required weighted checks contribute to aggregate score.
- Judge checks are represented in the schema and result model. If no live judge harness is configured, they return a skipped result with a clear reason and do not block deterministic eval execution.

### Aggregation

- Each check produces an outcome and optional numeric score.
- Overall score is the weighted average of scored checks.
- An eval passes only when required checks do not fail and the overall score is at least the eval threshold.
- Multi-eval runs report each eval result and an aggregate pass/fail status.

### Output

Terminal output includes:

- Eval id.
- Pass/fail result.
- Overall score.
- Failed checks.
- Violations or failure reasons.
- Duration.

JSON output includes:

- Overall score.
- Pass/fail result.
- Individual check results.
- Violations or failure reasons.
- Duration.
- Token usage when available.
- Model or harness when available.
- Session ID when available.

CSV output is pipe-friendly and includes one row per check with repeated eval-level fields.

## Non-Goals

- `svdo-meter` does not orchestrate retries, reviews, merges, or routing.
- Live LLM judging is not required for deterministic command evals.
- Eval execution does not mutate repo state beyond whatever commands explicitly do.

## Acceptance Criteria

- `svdo-meter eval run <eval>` runs one matching eval.
- `svdo-meter eval run` runs all evals.
- Eval parsing supports `id`, `task`, `checks`, `required`, `weight`, referenced standards, and `threshold`.
- Command checks execute and report success/failure, duration, and failure output.
- Required check failures cause eval failure regardless of score.
- Weighted checks contribute to aggregate score.
- Terminal, JSON, and CSV output are supported.
- Five sample eval definitions exist and parse successfully.
- Tests cover CLI parsing, eval parsing, one eval, all evals, command checks, required hard fail, aggregation, output formats, and sample validity.
