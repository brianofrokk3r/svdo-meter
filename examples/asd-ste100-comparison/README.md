# ASD-STE100-Style Prompt Comparison

This example compares two ways to write the same coding-agent task:

- `standard`: ordinary technical prose.
- `asd-ste100`: controlled technical English inspired by ASD-STE100.

The hypothesis is deliberately nondirectional: language style can affect implementation correctness, repository alignment, context usage, and execution behavior. The experiment measures whether a difference appears in the selected environment. It does not assume that the controlled prompt will perform better.

## Expanded task

Both prompts ask an agent to create a standard-library Python todo CLI. The task retains the basic todo fixture's persistent `add`, `list`, `complete`, and `delete` behavior and adds enough interacting behavior to reveal planning and context differences:

- optional validated due dates;
- repeatable normalized labels;
- status, exact-date, due-on-or-before, and label filters;
- logical AND for combined filters;
- label-grouped counts for open and completed todos;
- deterministic ordering and display;
- compatibility with basic stored todo records;
- failure atomicity for invalid commands and invalid stored data;
- a standard-library automated test suite.

See [standard.md](prompts/standard.md) and [asd-ste100.md](prompts/asd-ste100.md). The manifest in [variants.yaml](prompts/variants.yaml) gives both variants the same R01–R15 requirement inventory. The runner also supplies the same fixture, harness, model, repetition count, permissions, evaluation, and quality standard to both variants. Prompt language style is the intended independent variable.

## Controlled-language conventions

The `asd-ste100` variant applies this reproducible subset of ASD-STE100-style practices:

1. Use short declarative sentences.
2. Put one instruction in a sentence where practical.
3. Use active voice and an explicit actor or command subject.
4. Use the same term for the same object or operation.
5. State conditions explicitly with words such as `if` and `when`.
6. Separate requirements instead of joining several obligations in one long sentence.
7. Avoid ambiguous pronouns, vague references, implied optionality, and decorative wording.
8. Preserve literal command names, formats, regular expressions, and expected output exactly.

The transformation changes sentence structure and vocabulary control only. It does not add examples, implementation advice, constraints, acceptance criteria, or deliverables. The standard variant can combine related facts in conventional prose; the controlled variant usually separates those facts.

This is an **ASD-STE100-style adaptation**, not a formal conformance claim or certification. The example does not reproduce or replace the ASD-STE100 specification.

## Prerequisites

- Bash and Python 3.
- `svdo-meter` installed or built from this repository.
- A supported agent CLI with any credentials required by that provider.
- A working provider session and writable configuration, cache, log, and state directories for the selected agent CLI.
- Sufficient provider time and budget for two or more agent attempts.

When using the repository binary, build it first and point the workflow to it:

```bash
cargo build --bin svdo-meter
export SVDO_METER_BIN="$PWD/target/debug/svdo-meter"
```

## Smoke test

First validate the matrix without invoking an agent:

```bash
SVDO_ASD_DRY_RUN=1 \
SVDO_ASD_REPETITIONS=1 \
./examples/asd-ste100-comparison/run-comparison.sh
```

The output must contain one `standard-001` run and one `asd-ste100-001` run. Dry-run mode creates the directory layout and prints the commands, but it does not fabricate telemetry or evaluation results.

Before the live run, start the selected agent CLI once in the same environment. Confirm that it can initialize its local state and reach the configured provider. A successful dry run validates command construction only; it does not validate provider access, generated implementations, telemetry completeness, or evaluation results.

Then run one live attempt per variant:

```bash
SVDO_ASD_REPETITIONS=1 \
./examples/asd-ste100-comparison/run-comparison.sh
```

The script continues through the matrix if an individual agent run or eval fails. It produces the available reports and returns a nonzero status at the end when any step failed.

### Successful smoke-test criteria

The live smoke test is successful only when all of these conditions are true:

- `run-comparison.sh` exits with status zero;
- terminal telemetry contains one `run.completed` event for `standard-001` and one for `asd-ste100-001`;
- both attempt workspaces contain generated `todo.py` and `test_todo.py` files;
- both aggregate eval JSON files show that every required check passed;
- `svdo-report.txt`, `svdo-compare.txt`, and `comparison-summary.txt` are nonempty and include both variants; and
- the summary exposes correctness, token or context, execution, and repository-alignment fields. A provider can mark an unsupported metric as unavailable, but the report must not replace an unavailable value with zero.

A durable `run.failed` event demonstrates failure recording, but it does not satisfy the successful-comparison criteria and must not be interpreted as evidence about either prompt style.

## Larger comparison

The default is 10 repetitions per variant (20 independent agent runs):

```bash
./examples/asd-ste100-comparison/run-comparison.sh
```

Each attempt gets a unique ticket and a fresh child workspace. This prevents a provider session, generated implementation, or local state from carrying from one attempt to another. Run a one-repetition smoke test before committing to the default because a full study can consume substantial time and provider budget.

Common overrides are:

```bash
SVDO_ASD_WORK=ASD-TODO-EXPERIMENT \
SVDO_ASD_WORKSPACE=/tmp/asd-todo-study \
SVDO_ASD_HARNESS=codex \
SVDO_ASD_MODEL=gpt-5.5 \
SVDO_ASD_REPETITIONS=5 \
./examples/asd-ste100-comparison/run-comparison.sh
```

| Variable | Purpose |
| --- | --- |
| `SVDO_METER_BIN` | Select the `svdo-meter` executable. |
| `SVDO_ASD_WORK` | Set the work-id prefix. |
| `SVDO_ASD_WORKSPACE` | Set the aggregate workspace. |
| `SVDO_ASD_HARNESS` | Select the harness. |
| `SVDO_ASD_MODEL` | Select the harness-specific model. |
| `SVDO_ASD_REPETITIONS` | Set attempts per variant. |
| `SVDO_ASD_VARIANTS` | Select a space-separated subset for diagnostics. Use both variants for a comparison. |
| `SVDO_ASD_OUTPUT_ROOT` | Set the persistent result root. |
| `SVDO_ASD_OUTPUT_DIR` | Set the exact persistent result directory. |
| `SVDO_ASD_DRY_RUN=1` | Print commands without agent execution. |
| `SVDO_ASD_RUN_EVALS=0` | Skip evals. Do not use this for a quality comparison. |
| `SVDO_ASD_JUDGE=0` | Skip the repository-alignment judge. Use this only when judge credentials are unavailable. |
| `SVDO_ASD_DANGEROUS_BYPASS=1` | Pass the harness bypass option. This is off by default. |
| `SVDO_ASD_OPENCODE_AGENT` | Select the OpenCode agent when that harness is used. |

The full declarative configuration is in [comparison-matrix.yaml](comparison-matrix.yaml).

## Generated artifacts

The workflow prints its temporary or configured aggregate workspace. It contains:

```text
<workspace>/
├── .svdo/
│   ├── meter/*.jsonl
│   ├── evals/<variant>-<repetition>-eval.json
│   └── standards/expanded-todo-cli-quality.md
└── runs/
    └── <variant>-<repetition>.<suffix>/
```

Canonical JSONL is the durable source for the comparison. Per-run workspaces retain the generated implementation and their own telemetry. The aggregate eval directory retains the result of applying the same checks to every implementation.

Viewable output is saved separately under `examples/asd-ste100-comparison/study-output/<work-id>/`:

- `run.log`: complete workflow output;
- `study-info.txt`: resolved study configuration and paths;
- `svdo-report.txt`: native aggregate SVDO report;
- `svdo-compare.txt`: native SVDO comparison;
- `comparison-summary.txt`: prompt-variant summary.

To regenerate the study-specific report from retained artifacts:

```bash
./examples/asd-ste100-comparison/report-comparison.sh \
  ASD-TODO-EXPERIMENT \
  --workspace /tmp/asd-todo-study \
  --baseline standard
```

Inspect the canonical runs and native report directly when validating a study:

```bash
svdo-meter telemetry runs --workspace /tmp/asd-todo-study
svdo-meter report --workspace /tmp/asd-todo-study
svdo-meter compare --workspace /tmp/asd-todo-study
```

## Common validation

Every generated implementation is evaluated with [.svdo/evals/expanded-todo-cli.yaml](.svdo/evals/expanded-todo-cli.yaml). Its required command checks cover:

- persistent core commands and stable IDs;
- due-date and label normalization and display;
- exact-date, inclusive-date, status, and label filters with AND semantics;
- label-group counts, multi-label counting, and unlabeled items;
- invalid inputs that preserve existing state;
- malformed storage and compatibility with basic records;
- the generated standard-library tests.

The repository-alignment judge uses the same [quality standard](.svdo/standards/expanded-todo-cli-quality.md) for both variants. It assesses task completeness, deterministic behavior, state safety, test quality, dependency scope, and unrelated changes. It explicitly ignores prompt style.

Do not set `SVDO_ASD_RUN_EVALS=0` or `SVDO_ASD_JUDGE=0` for an acceptance run. Those switches are diagnostic fallbacks. A comparison without the shared command checks or repository-alignment judge is incomplete.

## Repository validation

From the repository root, validate the example and the surrounding Rust workspace:

```bash
bash -n examples/asd-ste100-comparison/run-comparison.sh
bash -n examples/asd-ste100-comparison/report-comparison.sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

The last command requires `cargo-deny` to be installed. An unavailable validation tool is not a passing gate; record it as an environment limitation and run the gate in CI or another prepared environment.

## Troubleshooting incomplete runs

- If the selected agent cannot create a configuration, cache, log, or state file, fix that CLI's directory permissions or use a correctly configured harness before rerunning the study.
- If the runner exits nonzero, inspect `run.log` and the terminal `run.completed` or `run.failed` events before reading the summary.
- If both variants fail before model execution, do not compare their duration, token, or eval rows as prompt-style results.
- If a token field is `-`, the provider did not report it. Preserve the missing value and compare only metrics supported by both variants.
- If an eval or judge was skipped, the run can help diagnose the workflow but is not a complete implementation-quality comparison.

## Comparison metrics

The summary groups labels such as `standard-001` by the prefix before the numeric repetition. It reports:

- **Correctness and completion:** terminal completion, eval score, required checks, judge score, and violations.
- **Token and context use:** input, cached input, cache-write, output, and reasoning tokens, plus an observed sum of the counters present in each terminal event.
- **Execution behavior:** wall time, executed commands, failed commands, tool calls, changed files, and errors.
- **Repository alignment:** the shared judge score and violations.

Provider harnesses do not always expose every metric. A dash means that no matching run reported the value; zero remains a measured zero. The `n` beside a mean is the number of runs that reported that metric. The observed token sum does not infer missing counters.

## Interpretation guidance

Compare quality before token totals. A shorter run that omits requirements is not an efficiency win. Check required-test results and judge feedback, then compare context and execution behavior among implementations of similar correctness.

Use multiple repetitions because agent output varies. Keep harness, model, permissions, repository revision, eval, and repetition count fixed. Treat the printed differences as observations from this repository revision and environment. Do not attribute causality to ASD-STE100-style language without adequate samples and controls, and do not generalize one provider's result to other tasks or models.

Report improvements only when retained telemetry and evaluated output support them. A neutral result, a regression, or mixed tradeoffs are valid outcomes.
