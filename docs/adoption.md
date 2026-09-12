# Applying SVDO Meter

SVDO Meter fits best as a lightweight operating layer for AI-assisted repository work. It is not a replacement for CI, ticketing, code review, or an agent orchestrator. Use it to make agent work measurable, repeatable, and easier to evaluate against repository expectations.

## Recommended Layers

### Level 1: Observe

Wrap meaningful AI-agent work with `svdo-meter run` so each session is tied to a ticket or work id and writes local append-only telemetry:

```bash
svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --workspace . \
  "Implement the password reset flow described in ENG-142"
```

Review the work trace afterward:

```bash
svdo-meter report ENG-142
svdo-meter telemetry runs
svdo-meter telemetry sessions
```

This layer answers questions such as:

- Which agent worked on this ticket?
- Which session or run produced the work?
- How much time and token usage did the work involve?
- Can the run history be inspected later from local telemetry?

### Level 2: Align

Add repository-specific eval definitions under `.svdo/evals/` and standards under `.svdo/standards/`. Run them before and after agent work to establish a baseline and verify that the final state still matches the repository's expectations:

```bash
svdo-meter eval run repo-alignment

svdo-meter run \
  --ticket ENG-142 \
  --harness codex \
  --workspace . \
  "Implement ENG-142"

svdo-meter eval run repo-alignment
```

This is the best place to encode checks that answer "does this work still fit this repository?" rather than only "did the tool finish?"

An alignment eval can combine ordinary command checks with optional judge checks:

```yaml
id: repo-alignment

task: |
  Verify that the current repository state satisfies the implementation task
  while following the repository's architecture and documentation standards.

checks:
  - id: tests
    type: command
    command: npm test
    required: true

  - id: lint
    type: command
    command: npm run lint
    required: true

  - id: architecture
    type: judge
    standard: architecture
    weight: 0.4

threshold: 0.85
```

### Level 3: Govern

Promote stable evals into CI once the checks are trusted:

```bash
svdo-meter eval run --format json
```

For pull requests, use deterministic evals as blocking CI gates and publish JSON or CSV output as build artifacts when useful. Start LLM judge checks as advisory reports, then make them blocking only after the team is comfortable with the standards, prompts, and failure behavior.

## Pre-Commit

Use pre-commit hooks for short deterministic evals only, such as formatting, linting, schema checks, or fast unit tests:

```bash
svdo-meter eval run precommit
```

Avoid putting slow command checks or LLM judge checks directly in pre-commit. A hook should protect the developer feedback loop, not turn every commit into a long review cycle.

## CI

CI is the right place for broader deterministic evals:

```bash
svdo-meter eval run --format json
```

A practical rollout is:

1. Run deterministic evals as advisory checks and inspect the output.
2. Make stable command checks blocking once false failures are rare.
3. Publish `json` or `csv` output as CI artifacts for PR review.
4. Add LLM judge checks as non-blocking reports.
5. Promote judge checks to blocking only after the standards and scoring behavior are stable.

## Typical Workflow

For one ticket or work item:

```bash
svdo-meter eval run repo-alignment

svdo-meter run \
  --ticket ENG-142 \
  --label "Add password reset flow" \
  --harness codex \
  --workspace . \
  "Implement the password reset flow described in ENG-142"

svdo-meter eval run repo-alignment
svdo-meter report ENG-142
```

That gives you a before-and-after alignment signal, durable local telemetry for the agent session, and a report tied back to the ticket.
