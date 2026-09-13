#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SVDO_METER_BIN="${SVDO_METER_BIN:-svdo-meter}"
PROMPT_FILE="${SVDO_BENCH_PROMPT:-$SCRIPT_DIR/TASK.md}"
EVAL_SOURCE="${SVDO_BENCH_EVAL:-$SCRIPT_DIR/.svdo/evals/calc-implementation.yaml}"
WORK="${SVDO_BENCH_WORK:-CALC-GPT55-$(date +%Y%m%d-%H%M%S)}"
WORKSPACE="${SVDO_BENCH_WORKSPACE:-$(mktemp -d -t svdo-calc-gpt55.XXXXXX)}"

CODEX_MODEL="${SVDO_BENCH_CODEX_MODEL:-gpt-5.5}"
OPENCODE_MODEL="${SVDO_BENCH_OPENCODE_MODEL:-codex/gpt-5.5}"
OPENCODE_AGENT="${SVDO_BENCH_OPENCODE_AGENT:-build}"

status=0

note() {
  printf '\n==> %s\n' "$*" >&2
}

run_step() {
  note "$*"
  "$@"
  local step_status=$?
  if [ "$step_status" -ne 0 ]; then
    printf 'step exited with status %s: %s\n' "$step_status" "$*" >&2
    status=1
  fi
  return 0
}

write_compare_artifact() {
  local harness="$1"
  local model="$2"
  local raw_eval="$3"
  local compare_artifact="$4"

  python3 - "$WORK" "$harness" "$model" "$raw_eval" "$compare_artifact" <<'PY'
import json
import sys
from pathlib import Path

work, harness, model, raw_eval, compare_artifact = sys.argv[1:]
with open(raw_eval, encoding="utf-8") as handle:
    report = json.load(handle)

results = report.get("results") or [{}]
result = results[0]
artifact = {
    "work": work,
    "harness": harness,
    "requested_model": model,
    "overall_score": result.get("overall_score"),
    "checks": result.get("checks", []),
    "violations": result.get("violations", []),
}

Path(compare_artifact).parent.mkdir(parents=True, exist_ok=True)
with open(compare_artifact, "w", encoding="utf-8") as handle:
    json.dump(artifact, handle, indent=2)
    handle.write("\n")
PY
  local artifact_status=$?
  if [ "$artifact_status" -ne 0 ]; then
    printf 'failed to write compare artifact for %s from %s\n' "$harness" "$raw_eval" >&2
    status=1
  fi
}

reset_task_output() {
  rm -f "$WORKSPACE/calc.py"
}

if ! command -v "$SVDO_METER_BIN" >/dev/null 2>&1; then
  printf 'svdo-meter was not found. Install it globally, add it to PATH, or set SVDO_METER_BIN.\n' >&2
  exit 127
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf 'python3 is required to shape eval JSON for compare artifacts.\n' >&2
  exit 127
fi

if [ ! -f "$PROMPT_FILE" ]; then
  printf 'prompt file not found: %s\n' "$PROMPT_FILE" >&2
  exit 1
fi

if [ ! -f "$EVAL_SOURCE" ]; then
  printf 'eval definition not found: %s\n' "$EVAL_SOURCE" >&2
  exit 1
fi

mkdir -p "$WORKSPACE/.svdo/evals"
cp "$EVAL_SOURCE" "$WORKSPACE/.svdo/evals/"

cat <<EOF
SVDO Meter GPT-5.5 calculator benchmark

Work:      $WORK
Workspace: $WORKSPACE
Prompt:    $PROMPT_FILE
Eval:      $WORKSPACE/.svdo/evals/$(basename "$EVAL_SOURCE")
Binary:    $SVDO_METER_BIN

This script follows the README production flow:
  svdo-meter run -> svdo-meter eval run -> svdo-meter report -> svdo-meter compare
EOF

reset_task_output
run_step "$SVDO_METER_BIN" run \
  --ticket "$WORK" \
  --label "calculator codex gpt-5.5" \
  --harness codex \
  --workspace "$WORKSPACE" \
  --model "$CODEX_MODEL" \
  --dangerous-bypass \
  --prompt-file "$PROMPT_FILE"

CODEX_RAW_EVAL="$WORKSPACE/.svdo/evals/${WORK}-codex-raw.json"
run_step "$SVDO_METER_BIN" eval run implementation \
  --workspace "$WORKSPACE" \
  --format json >"$CODEX_RAW_EVAL"
write_compare_artifact \
  "codex" \
  "$CODEX_MODEL" \
  "$CODEX_RAW_EVAL" \
  "$WORKSPACE/.svdo/evals/${WORK}-codex-compare.json"

reset_task_output
run_step "$SVDO_METER_BIN" run \
  --ticket "$WORK" \
  --label "calculator opencode codex/gpt-5.5" \
  --harness opencode \
  --workspace "$WORKSPACE" \
  --model "$OPENCODE_MODEL" \
  --opencode-agent "$OPENCODE_AGENT" \
  --dangerous-bypass \
  --prompt-file "$PROMPT_FILE"

OPENCODE_RAW_EVAL="$WORKSPACE/.svdo/evals/${WORK}-opencode-raw.json"
run_step "$SVDO_METER_BIN" eval run implementation \
  --workspace "$WORKSPACE" \
  --format json >"$OPENCODE_RAW_EVAL"
write_compare_artifact \
  "opencode" \
  "$OPENCODE_MODEL" \
  "$OPENCODE_RAW_EVAL" \
  "$WORKSPACE/.svdo/evals/${WORK}-opencode-compare.json"

run_step "$SVDO_METER_BIN" report "$WORK" --workspace "$WORKSPACE"
run_step "$SVDO_METER_BIN" compare "$WORK" \
  --workspace "$WORKSPACE" \
  --harness codex \
  --harness opencode \
  --model gpt-5.5

cat <<EOF

Benchmark artifacts are in:
  $WORKSPACE

Telemetry:
  $WORKSPACE/.svdo/meter/

Eval artifacts:
  $WORKSPACE/.svdo/evals/
EOF

exit "$status"
