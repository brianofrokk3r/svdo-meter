# Implementation Plan: First-Class Repository Alignment Evals

## Context

`svdo-meter` is a Rust Clap binary with existing `run`, `report`, and `telemetry` command groups. Report output supports `terminal`, `json`, and `csv` through a shared `ReportFormat` enum. Integration tests invoke the compiled binary against temporary fixture workspaces.

The repo standards index requested by `AGENTS.md` is missing at `.joule/standards.md`; the existing constitution at `.specify/memory/constitution.md` was read and preserved.

## Design

1. Extend `crates/svdo-meter/src/cli.rs`.
   - Add `eval` command group with `run [eval]`.
   - Reuse `ReportFormat` for `--format`.
   - Add `--workspace` matching report and telemetry conventions.

2. Add `crates/svdo-meter/src/eval.rs`.
   - Define YAML-backed eval schema.
   - Load `.yaml` and `.yml` definitions from `.svdo/evals/`.
   - Resolve one eval by id, stem, or file name, or load all evals.
   - Execute command checks in the workspace.
   - Represent judge checks as skipped when no judge harness is available.
   - Aggregate weighted scores and enforce required hard-fail semantics.
   - Render terminal, JSON, and CSV output.

3. Wire `main.rs`.
   - Dispatch `Commands::Eval`.
   - Print rendered output.
   - Exit non-zero when any eval fails.

4. Add sample eval definitions.
   - Place five examples under `.svdo/evals/`.
   - Include deterministic command checks that work without external services.
   - Include judge-style schema examples guarded by skipped behavior.
   - Add sample standards under `.svdo/standards/` for referenced judge checks.

5. Add tests.
   - Unit tests for CLI parsing and eval parser/aggregation/rendering.
   - Integration tests using temporary `.svdo/evals/` fixture workspaces.
   - Test sample eval validity by loading repo samples.

## Dependencies

- Add `serde` and `serde_yaml` to `svdo-meter` dependencies. `serde` already exists in the workspace dependency set; `serde_yaml` will be added to workspace dependencies if absent.

## Risks

- Command checks can run arbitrary commands from eval definitions. This is intentional local developer behavior and should be clearly scoped to the selected workspace.
- Judge checks need future live harness integration. The initial schema/result support keeps deterministic evals usable without credentials.

## Verification

- Run `cargo fmt`.
- Run `cargo test -p svdo-meter`.
- Run the new CLI manually against sample evals where feasible.
