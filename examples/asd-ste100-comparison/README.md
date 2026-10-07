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
- compatibility with the explicitly supplied legacy root-array storage fixture;
- failure atomicity for invalid commands and invalid stored data;
- a standard-library automated test suite.

See [standard.md](prompts/standard.md) and [asd-ste100.md](prompts/asd-ste100.md). The manifest in [variants.yaml](prompts/variants.yaml) gives both variants the same R01–R15 requirement inventory. The runner also supplies the same fixture, harness, model, repetition count, permissions, evaluation, and quality standard to both variants. Prompt language style is the intended independent variable. Both prompts state the exact legacy root-array representation, and each attempt receives the same reference file at `.svdo/fixtures/basic-todo.json`.

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

For Codex runs, the workflow explicitly selects the `workspace-write` sandbox
because each attempt must create `todo.py` and `test_todo.py` in its isolated
workspace. Approval and sandbox bypass remain disabled unless
`SVDO_ASD_DANGEROUS_BYPASS=1` is set.

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

Attempts use a balanced interleaved rotation. With the default two variants,
the first repetition runs `standard` then `asd-ste100`, the second runs
`asd-ste100` then `standard`, and subsequent repetitions continue that pattern.
This keeps variants close in time and prevents one variant from always running
first. The deterministic schedule is recorded in `study-info.txt`.

The declared primary outcome is the proportion of attempts that pass every
required deterministic check. The report presents its count, rate, and a
descriptive 95% Wilson interval for each variant. Eval and judge scores,
check-level findings, execution time, and provider token counters are secondary
outcomes.

Common overrides are:

```bash
SVDO_ASD_WORK=ASD-TODO-EXPERIMENT \
SVDO_ASD_WORKSPACE=/tmp/asd-todo-study \
SVDO_ASD_HARNESS=codex \
SVDO_ASD_MODEL=gpt-5.5 \
SVDO_ASD_JUDGE_MODEL=gpt-5.6-sol \
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
| `SVDO_ASD_JUDGE_MODEL` | Select the judge model independently. It defaults to `SVDO_ASD_MODEL`; use a distinct model for a stronger acceptance study when available. |
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
│   ├── fixtures/basic-todo.json
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

For new studies, the summary counts only check-level findings, reports the
number of attempts that passed every required check, supplies a descriptive
95% Wilson interval for that per-run pass rate, and lists failed required checks
by ID. It excludes duplicated result-level copies and score-threshold messages
from the finding count. If all observed command, tool-call, and file-change
counters are zero, the summary prints a telemetry warning.

## Recorded experimental output: 2026-10-06

This retained output predates the design corrections described above. It used
grouped rather than interleaved execution, did not provide or explicitly define
the legacy root-array fixture, used the implementation model as its judge, and
reported duplicated violation entries. Keep it as historical evidence and do
not combine it directly with results from the corrected protocol.

The retained study
[`ASD-STE100-TODO-20261006-094849`](study-output/ASD-STE100-TODO-20261006-094849/)
used the Codex harness with `gpt-5.6-sol` and ran 30 attempts per variant. All
60 agent runs emitted `run.completed`. The aggregate summary reported these
results:

| Metric | `standard` | `asd-ste100` | ASD-STE100-style difference |
| --- | ---: | ---: | ---: |
| Mean eval score | 0.96 | 0.90 | -0.06 |
| Mean judge score | 0.98 | 0.96 | -0.02 |
| Required checks passed | 198/210 (94.3%) | 181/210 (86.2%) | -17 checks (-8.1 percentage points) |
| Reported violation entries | 49 | 121 | +72 (2.47 times the standard count) |
| Mean wall time | 172,688.0 ms | 155,957.9 ms | -16,730.1 ms (-9.7%) |
| Mean input tokens | 217,682.9 | 223,622.5 | +5,939.6 (+2.7%) |
| Mean output tokens | 8,603.0 | 8,386.6 | -216.4 (-2.5%) |
| Mean observed token-counter sum | 424,413.3 | 436,565.4 | +12,152.1 (+2.9%) |

### What the violations imply

**Primary finding: the ASD-STE100-style implementations failed the required
legacy-storage check in 29/30 attempts, compared with 12/30 standard
attempts.** This is a backward-compatibility failure, not merely a larger
collection of minor style findings. All missed required checks came from the
composite `malformed-storage-and-legacy-records` check:

| Required check result | `standard` | `asd-ste100` |
| --- | ---: | ---: |
| Passed | 18/30 (60.0%) | 1/30 (3.3%) |
| Failed at the legacy-state step | 12/30 (40.0%) | 29/30 (96.7%) |

The failing implementations required `.todo.json` to contain a new root object
with `next_id` and `todos`. They rejected the basic fixture's existing root JSON
array. Consequently, an existing user could upgrade to one of these
implementations and find that the CLI no longer opens their stored todos. The
failure output shows that the implementations rejected the legacy schema; it
does not indicate that they overwrote the old file.

The repository-alignment judge found related and secondary problems. Counts in
this table are the number of attempts in which the judge mentioned the category;
one attempt can appear in more than one row.

| Judge finding | `standard` | `asd-ste100` | Practical implication |
| --- | ---: | ---: | --- |
| Legacy root-array schema rejected | 1 | 7 | Confirms the backward-compatibility defect found more consistently by the command check. |
| Tests used the new wrapper instead of the real legacy representation | 1 | 7 | The generated tests could pass while failing to protect the required upgrade path. |
| Unsupported help, abbreviated, or repeated options accepted | 9 | 3 | Misspelled or undefined CLI input can be silently interpreted as a valid command. |
| Invalid stored due dates or labels mishandled | 0 | 4 | Invalid state can be accepted, or can produce a traceback instead of a clear schema error. |
| State-file symbolic link not guarded | 0 | 1 | The CLI can read state outside the current working directory through a symbolic link. |
| Generated `__pycache__` artifact retained | 1 | 2 | The output contains an unrelated file; this is a scope and repository-hygiene issue rather than a behavior failure. |

The raw totals of 49 and 121 are **reported violation entries**, not counts of
independent defects or failed runs. The summary adds check-level violations,
result-level copies of some of those violations, and an additional entry when
an overall score is below its threshold. The totals therefore contain
duplication. The 12/30 versus 29/30 required-check failure rate is the clearer
and more consequential comparison.

### Attribution caveat

This result does not, by itself, identify a defect in ASD-STE100 or in the Codex
CLI. Both prompt variants say that legacy todo objects can omit `due` and
`labels`, but neither prompt states that the basic fixture stores those objects
in a root JSON array. The evaluator requires that exact root-array format. The
study therefore includes an unstated acceptance constraint, and the high
failure rate partly exposes a prompt-to-evaluator alignment problem.

Both variants used the same Codex CLI, model, sandbox, and evaluator, so the
data does not point to a variant-specific CLI failure. The language rewrite may
have changed what schema the model inferred, but that is only a hypothesis. In
addition, the runner completed all `standard` attempts before starting the
`asd-ste100` attempts instead of randomizing or interleaving them. Provider or
time-dependent changes are therefore confounded with prompt style.

The current protocol corrects these issues by stating and supplying the legacy
fixture, using balanced interleaving, supporting an independently selected
judge model, and removing duplicated result-level violations from new
summaries. If the difference persists in a new run, it would support a narrower
claim about this ASD-STE100-style rewrite on this task and model, not about the
ASD-STE100 specification in general.

Overall, the standard prompt had higher observed eval and judge scores and was
much more likely to preserve the required legacy-storage compatibility. The
ASD-STE100-style prompt completed about 9.7% faster on average, but its observed
token-counter sum was about 2.9% higher.

These are descriptive results for this task, repository revision, harness,
model, and provider environment. The study summary does not include uncertainty
estimates or a statistical-significance test, so it does not establish that
either writing style is generally superior or that prompt style caused the
differences. `run.completed` records harness completion, not passage of every
required check. The observed token-counter sum adds every counter present in a
terminal event; it is not a billing total, and counters can overlap.

The terminal events recorded zero commands, tool calls, changed files, and
errors for both variants. The retained `run.log` nevertheless contains tool
activity and rejected commands, so those zero-valued execution counters should
be treated as a telemetry limitation rather than evidence that no tools ran.
See the retained
[`comparison-summary.txt`](study-output/ASD-STE100-TODO-20261006-094849/comparison-summary.txt),
[`study-info.txt`](study-output/ASD-STE100-TODO-20261006-094849/study-info.txt),
and [`run.log`](study-output/ASD-STE100-TODO-20261006-094849/run.log) for the
source output and resolved configuration.

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

- **Correctness and completion:** terminal completion, eval score, required checks, attempts passing every required check with a descriptive 95% Wilson interval, judge score, and nonduplicated check-level findings.
- **Token and context use:** input, cached input, cache-write, output, and reasoning tokens, plus an observed sum of the counters present in each terminal event.
- **Execution behavior:** wall time, executed commands, failed commands, tool calls, changed files, and errors.
- **Repository alignment:** the shared judge score and judge findings.

Provider harnesses do not always expose every metric. A dash means that no matching run reported the value; zero remains a measured zero. The `n` beside a mean is the number of runs that reported that metric. The observed token sum does not infer missing counters.

## Interpretation guidance

Compare quality before token totals. A shorter run that omits requirements is not an efficiency win. Check required-test results and judge feedback, then compare context and execution behavior among implementations of similar correctness.

Use multiple repetitions because agent output varies. Keep harness, model, permissions, repository revision, eval, and repetition count fixed. Treat the printed differences as observations from this repository revision and environment. Do not attribute causality to ASD-STE100-style language without adequate samples and controls, and do not generalize one provider's result to other tasks or models.

Report improvements only when retained telemetry and evaluated output support them. A neutral result, a regression, or mixed tradeoffs are valid outcomes.
