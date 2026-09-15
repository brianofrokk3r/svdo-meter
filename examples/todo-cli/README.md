# Todo CLI Fixture

This fixture gives svdo-meter studies a small, repeatable coding task for measuring how prompt wording affects agent runs.

The task asks an agent to implement `todo.py` with `add`, `list`, `complete`, and `delete` commands. The CLI stores state in `.todo.json` so eval checks can exercise behavior across separate process invocations.

## Run One Agent Attempt

From an isolated workspace that contains this fixture's files:

```bash
svdo-meter run \
  --ticket STOPWORD-TODO \
  --label concise-baseline-001 \
  --harness codex \
  --model gpt-5.5 \
  --dangerous-bypass \
  --prompt-file TASK.md
```

## Run A Prompt Variant

Prompt variants live in `prompts/` and keep the todo-cli task intent constant while changing stopword density, verbosity, or wording style. The variant index in `prompts/variants.yaml` includes ids, names, stopword profiles, and report descriptions.

Run a selected variant by changing `--prompt-file` and using the variant id in the run label:

```bash
svdo-meter run \
  --ticket STOPWORD-TODO \
  --label stopword-heavy-guided-001 \
  --harness codex \
  --model gpt-5.5 \
  --dangerous-bypass \
  --prompt-file prompts/stopword-heavy-guided.md
```

For a balanced study, run the same number of repetitions for each variant listed in `prompts/variants.yaml`, then compare token telemetry and eval results by label prefix.

## Run The Stopword Matrix

The reproducible matrix is defined in `stopword-matrix.yaml`. It defaults to:

- ticket/work id prefix: `STOPWORD-TODO`
- harness: `codex`
- model: `gpt-5.5`
- variants: all four prompt variants in `prompts/variants.yaml`
- repetitions: 30 per variant, 120 agent runs total
- labels: `<variant-id>-NNN`, such as `concise-baseline-001`
- per-run ticket ids: `<work-id>-<label>`, such as `STOPWORD-TODO-20260914-233007-concise-baseline-001`
- primary metric: observed `output_tokens` in svdo-meter telemetry

Preview the full command matrix without running agents:

```bash
SVDO_STOPWORD_DRY_RUN=1 ./run-stopword-matrix.sh
```

Run a cheap smoke test first:

```bash
SVDO_STOPWORD_REPETITIONS=1 ./run-stopword-matrix.sh
```

Run the full default matrix from this fixture directory:

```bash
./run-stopword-matrix.sh
```

The script creates a disposable aggregate workspace and a fresh child workspace for each agent attempt under `runs/<label>.*`. Each attempt also gets a distinct ticket id, `<work-id>-<label>`, so svdo-meter's session auto-discovery cannot resume a previous Codex session for the same work item. Each attempt runs with `svdo-meter run --prompt-file` in its own child workspace, then the script copies telemetry into the aggregate `.svdo/meter/` directory and per-run eval JSON into aggregate `.svdo/evals/` before rendering:

```bash
svdo-meter report --workspace "$SVDO_STOPWORD_WORKSPACE"
svdo-meter compare --workspace "$SVDO_STOPWORD_WORKSPACE"
./report-stopword-study.sh "$SVDO_STOPWORD_WORK" --workspace "$SVDO_STOPWORD_WORKSPACE" --baseline concise-baseline
```

It also saves a persistent transcript and report files under `study-output/<work-id>/` in this fixture directory, so clearing the terminal does not discard a long run's output:

```text
study-output/<work-id>/run.log
study-output/<work-id>/svdo-report.txt
study-output/<work-id>/svdo-compare.txt
study-output/<work-id>/study-summary.txt
study-output/<work-id>/study-info.txt
```

Eval artifacts are named with the run label, such as `.svdo/evals/concise-baseline-001-eval.json`, so the example study summary script can attach deterministic and judge quality results to the matching telemetry run.

The default 30 repetitions per variant is intended for a real comparison and can take substantial time and provider budget. Use `SVDO_STOPWORD_REPETITIONS=1` or `2` while checking local setup, credentials, and judge behavior.

The parent stopword-impact study uses OpenAI GPT-5.5 as the documented default. You can override the harness and model for local setups:

```bash
SVDO_STOPWORD_HARNESS=opencode \
SVDO_STOPWORD_MODEL=openai/gpt-5.5 \
SVDO_STOPWORD_OPENCODE_AGENT=build \
SVDO_STOPWORD_REPETITIONS=1 \
./run-stopword-matrix.sh
```

Other useful overrides:

- `SVDO_STOPWORD_WORK`: set the exact ticket/work id.
- `SVDO_STOPWORD_WORKSPACE`: reuse a specific disposable aggregate workspace.
- `SVDO_STOPWORD_VARIANTS`: run a subset, for example `concise-baseline stopword-heavy-guided`.
- `SVDO_STOPWORD_OUTPUT_ROOT`: override the default persistent output root, `study-output`.
- `SVDO_STOPWORD_OUTPUT_DIR`: override the exact directory for the run log and saved reports.
- `SVDO_STOPWORD_RUN_EVALS=0`: skip per-run eval artifacts.
- `SVDO_STOPWORD_JUDGE=0`: skip passing the selected harness and model to judge checks.
- `SVDO_METER_BIN`: point at a local binary, such as `../../target/debug/svdo-meter`.

Use `--dangerous-bypass` only in a disposable workspace where automatic edits and command execution are acceptable.

## Evaluate The Result

Run deterministic checks:

```bash
svdo-meter eval run todo-cli-implementation --workspace .
```

Run the same eval with an optional judge check when a judge harness is available:

```bash
svdo-meter eval run todo-cli-implementation --workspace . --harness codex --model gpt-5.5
```

Render telemetry and compare repeated runs:

```bash
svdo-meter report STOPWORD-TODO --workspace .
svdo-meter compare STOPWORD-TODO --workspace .
./report-stopword-study.sh STOPWORD-TODO --workspace . --baseline concise-baseline
```

`report-stopword-study.sh` groups runs by the label prefix before the trailing numeric repetition suffix. For example, `stopword-heavy-guided-001` contributes to the `stopword-heavy-guided` variant. The script analyzes observed telemetry `output_tokens`, then prints sample size, mean, variance, standard deviation, min/p50/max, completion rate, eval score, judge score, required checks, violations, and pairwise differences from the baseline. Its significance line uses a Welch-style 95% confidence heuristic, so interpret it as a reproducible local measurement for this environment rather than universal proof about stopwords.

The stopword-impact parent study can copy this fixture into temporary workspaces, swap in prompt variants, and label runs by variant and repetition while preserving the same target behavior.
