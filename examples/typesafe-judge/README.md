# TypeSafe Judge Calculator Example

This example shows TypeSafe scoring against a concrete generated artifact: a
small Python calculator CLI.

```text
calc.py
test_calc.py
.svdo/evals/calculator-quality.yaml
.svdo/standards/calculator-quality.md
.svdo/rubrics/calculator-implementation.yaml
```

Run the local tests:

```bash
python -m unittest test_calc.py
```

Run the eval from the repository root:

```bash
export TYPESAFE_API_KEY=...
cargo run -p svdo-meter --bin svdo-meter -- \
  eval run calculator-quality \
  --workspace examples/typesafe-judge
```

The eval uses a command check for calculator behavior and a TypeSafe-backed
judge check for implementation quality. The TypeSafe request includes the task,
standard, ordered Score criteria, git snapshot data when available, and a
bounded snapshot of source files such as `calc.py` and `test_calc.py`.

## Speed experiment

Run the paired Codex-vs-TypeSafe judge experiment:

```bash
export TYPESAFE_API_KEY=...
./examples/typesafe-judge/run-eval-speed-experiment.sh
```

The script defaults to 30 paired iterations of the same `calculator-quality`
eval. Each iteration runs the eval once with a traditional Codex judge and once
with the TypeSafe judge, preserving the raw eval JSON for every run under
`examples/typesafe-judge/experiment-output/<timestamp>/raw/`.
When `target/debug/svdo-meter` exists, the script uses that repo-local binary by
default so newly added eval flags are available. Set `SVDO_METER_BIN` to override
it.

The summary artifacts report pass rates, overall scores, judge scores,
`eval_duration_ms`, judge `check_duration_ms`, judge token usage when reported,
and Codex/TypeSafe speedups:

```text
examples/typesafe-judge/experiment-output/<timestamp>/summary.txt
examples/typesafe-judge/experiment-output/<timestamp>/summary.json
examples/typesafe-judge/experiment-output/<timestamp>/summary.csv
```

Useful knobs:

```bash
SVDO_TYPESAFE_EXPERIMENT_ITERATIONS=50
SVDO_TYPESAFE_EXPERIMENT_CODEX_MODEL=gpt-5
SVDO_TYPESAFE_EXPERIMENT_TYPESAFE_MODEL=jev-latest
SVDO_TYPESAFE_EXPERIMENT_OUTPUT_DIR=/tmp/typesafe-speed
```

Regenerate summaries from an existing run without rerunning evals:

```bash
SVDO_TYPESAFE_EXPERIMENT_SUMMARIZE_ONLY=1 \
SVDO_TYPESAFE_EXPERIMENT_OUTPUT_DIR=examples/typesafe-judge/experiment-output/<timestamp> \
  ./examples/typesafe-judge/run-eval-speed-experiment.sh
```

# Experiment results
