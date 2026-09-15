# SVDO Meter Examples

This page keeps fuller runnable examples out of the README while preserving copy-pasteable workflows.

## Calculator Benchmark: Codex vs OpenCode

The calculator example exercises the full production flow with a small deterministic task:

```text
svdo-meter run -> svdo-meter eval run -> svdo-meter report -> svdo-meter compare
```

It compares Codex and OpenCode on the same prompt and eval:

- Codex model: `gpt-5.5`
- OpenCode model: `openai/gpt-5.5`
- Prompt: `examples/calculator/TASK.md`
- Eval: `examples/calculator/.svdo/evals/calc-implementation.yaml`

Install `svdo-meter`, make sure `svdo-meter`, `codex`, `opencode`, and `python3` are available on `PATH`, then run:

```bash
./examples/calculator/benchmark-gpt55-codex-vs-opencode.sh
```

The script creates an isolated temporary workspace, copies only the calculator eval definition into that workspace, runs each harness with `--prompt-file`, evaluates the generated `calc.py`, writes compare enrichment artifacts under `.svdo/evals/`, and prints both `svdo-meter report` and `svdo-meter compare` output. The temporary workspace is kept so you can inspect generated files and telemetry after the run:

```text
<temp-workspace>/
  calc.py
  .svdo/
    meter/
    evals/
```

You can override the work id or workspace with environment variables:

```bash
SVDO_BENCH_WORK=CALC-GPT55-MANUAL \
SVDO_BENCH_WORKSPACE=/tmp/svdo-calc-gpt55 \
./examples/calculator/benchmark-gpt55-codex-vs-opencode.sh
```

Additional knobs are available for model or agent experiments:

```bash
SVDO_BENCH_CODEX_MODEL=gpt-5.5 \
SVDO_BENCH_OPENCODE_MODEL=openai/gpt-5.5 \
SVDO_BENCH_OPENCODE_AGENT=build \
./examples/calculator/benchmark-gpt55-codex-vs-opencode.sh
```

The benchmark uses `--dangerous-bypass` because it runs in an isolated throwaway workspace. Use that posture only for workspaces where automatic edits and command execution are acceptable.

## Todo CLI Stopword Impact Study

The todo CLI stopword study is a reproducible workflow for measuring how prompt wording changes correlate with observed output token generation in your local environment. It keeps the coding task fixed and varies only prompt style, stopword density, and verbosity.

Study inputs and artifacts:

- Fixture and task: `examples/todo-cli/` and `examples/todo-cli/TASK.md`
- Prompt variants: `examples/todo-cli/prompts/` and `examples/todo-cli/prompts/variants.yaml`
- Run matrix: `examples/todo-cli/stopword-matrix.yaml`
- Runner: `examples/todo-cli/run-stopword-matrix.sh`
- Study summary script: `examples/todo-cli/report-stopword-study.sh`
- Eval definition: `examples/todo-cli/.svdo/evals/todo-cli-implementation.yaml`
- Judge standard: `examples/todo-cli/.svdo/standards/todo-cli-quality.md`

The task asks an agent to build `todo.py` with `add`, `list`, `complete`, and `delete` commands backed by `.todo.json`. The default study uses Codex with OpenAI GPT-5.5, four prompt variants, and 30 repetitions per variant for 120 total agent runs.

Install `svdo-meter`, make sure the selected harness is available on `PATH`, and configure the provider credentials required by that harness. From the repository root, preview the full matrix without running agents:

```bash
cd examples/todo-cli
SVDO_STOPWORD_DRY_RUN=1 ./run-stopword-matrix.sh
```

Run a low-cost smoke test before spending time and provider budget on the full matrix:

```bash
SVDO_STOPWORD_REPETITIONS=1 ./run-stopword-matrix.sh
```

Run the default OpenAI GPT-5.5 study:

```bash
./run-stopword-matrix.sh
```

The helper creates a disposable workspace, copies the fixture eval and judge standard into that workspace, runs each variant with `svdo-meter run --prompt-file`, captures telemetry under `.svdo/meter/`, writes per-run eval JSON under `.svdo/evals/`, then renders:

```bash
svdo-meter report "$SVDO_STOPWORD_WORK" --workspace "$SVDO_STOPWORD_WORKSPACE"
svdo-meter compare "$SVDO_STOPWORD_WORK" --workspace "$SVDO_STOPWORD_WORKSPACE"
./report-stopword-study.sh "$SVDO_STOPWORD_WORK" --workspace "$SVDO_STOPWORD_WORKSPACE" --baseline concise-baseline
```

Run one Codex attempt with OpenAI GPT-5.5 from a disposable copy of the fixture if you want to inspect a single generated `todo.py` before running a matrix:

```bash
svdo-meter run \
  --ticket STOPWORD-TODO \
  --label concise-baseline-001 \
  --harness codex \
  --model gpt-5.5 \
  --dangerous-bypass \
  --prompt-file TASK.md
```

Then evaluate and inspect telemetry from that workspace:

```bash
svdo-meter eval run todo-cli-implementation --workspace .
svdo-meter report STOPWORD-TODO --workspace .
svdo-meter compare STOPWORD-TODO --workspace .
./report-stopword-study.sh STOPWORD-TODO --workspace . --baseline concise-baseline
```

The fixture documents harness and model overrides in `examples/todo-cli/README.md`. Judge checks are included but remain advisory unless you run the eval with `--harness` and `--model` or provide `--judge-command`.

### Configuration

Use environment variables to adapt the matrix without editing the helper script:

- `SVDO_STOPWORD_HARNESS`: override the harness, such as `codex` or `opencode`.
- `SVDO_STOPWORD_MODEL`: override the harness-specific model id, such as `gpt-5.5` or `openai/gpt-5.5`.
- `SVDO_STOPWORD_REPETITIONS`: reduce or increase repetitions per variant.
- `SVDO_STOPWORD_VARIANTS`: run a space-separated subset, such as `concise-baseline stopword-heavy-guided`.
- `SVDO_STOPWORD_WORK`: set the exact ticket/work id used by report commands.
- `SVDO_STOPWORD_WORKSPACE`: reuse a specific disposable workspace.
- `SVDO_STOPWORD_RUN_EVALS=0`: skip per-run eval artifacts when you only want telemetry.
- `SVDO_STOPWORD_JUDGE=1`: pass the selected harness and model to LLM judge checks.
- `SVDO_METER_BIN`: point at a local binary, such as `../../target/debug/svdo-meter`.

For example:

```bash
SVDO_STOPWORD_HARNESS=opencode \
SVDO_STOPWORD_MODEL=openai/gpt-5.5 \
SVDO_STOPWORD_OPENCODE_AGENT=build \
SVDO_STOPWORD_REPETITIONS=1 \
./run-stopword-matrix.sh
```

### Cost And Time

The default matrix is meant for a statistically useful comparison, not a quick demo. With four variants and 30 repetitions per variant, it performs 120 agent runs plus eval work. Start with `SVDO_STOPWORD_DRY_RUN=1`, then `SVDO_STOPWORD_REPETITIONS=1`, and only run the full matrix after credentials, model access, and local command execution are working.

Use `--dangerous-bypass` only in a disposable workspace where automatic edits and command execution are acceptable. The helper enables that posture by default because it creates or reuses a dedicated study workspace.

### Interpreting Results

The example `report-stopword-study.sh` script groups runs by the prompt variant label prefix before the trailing repetition suffix. For example, `stopword-heavy-guided-001` contributes to the `stopword-heavy-guided` group. The primary measured token metric is telemetry `output_tokens`.

For each variant, the study summary script reads SVDO Meter telemetry and label-named eval artifacts, then summarizes sample size, mean, variance, standard deviation, min, p50, max, completion rate, eval score, judge score, required checks, and violations. It compares each variant with the `concise-baseline` group using a Welch-style 95% confidence heuristic. Read that significance line as a practical confidence signal for this specific harness, model, machine, prompt set, and time period.

The study is designed to make prompt wording effects measurable and reproducible in one environment. It is not universal proof that stopwords always increase or decrease output token generation.
