#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DEFAULT_SVDO_METER_BIN="svdo-meter"
if [ -x "$SCRIPT_DIR/../../target/debug/svdo-meter" ]; then
  DEFAULT_SVDO_METER_BIN="$SCRIPT_DIR/../../target/debug/svdo-meter"
fi
SVDO_METER_BIN="${SVDO_METER_BIN:-$DEFAULT_SVDO_METER_BIN}"
WORKSPACE="${SVDO_TYPESAFE_EXPERIMENT_WORKSPACE:-$SCRIPT_DIR}"
EVAL_ID="${SVDO_TYPESAFE_EXPERIMENT_EVAL:-calculator-quality}"
ITERATIONS="${SVDO_TYPESAFE_EXPERIMENT_ITERATIONS:-30}"
OUTPUT_ROOT="${SVDO_TYPESAFE_EXPERIMENT_OUTPUT_ROOT:-$SCRIPT_DIR/experiment-output}"
RUN_ID="${SVDO_TYPESAFE_EXPERIMENT_RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
OUTPUT_DIR="${SVDO_TYPESAFE_EXPERIMENT_OUTPUT_DIR:-$OUTPUT_ROOT/$RUN_ID}"
RAW_DIR="$OUTPUT_DIR/raw"
SUMMARY_JSON="$OUTPUT_DIR/summary.json"
SUMMARY_CSV="$OUTPUT_DIR/summary.csv"
SUMMARY_TXT="$OUTPUT_DIR/summary.txt"
RUN_LOG="$OUTPUT_DIR/run.log"

CODEX_MODEL="${SVDO_TYPESAFE_EXPERIMENT_CODEX_MODEL:-gpt-5.5}"
TYPESAFE_MODEL="${SVDO_TYPESAFE_EXPERIMENT_TYPESAFE_MODEL:-jev-latest}"
TYPESAFE_API_KEY_ENV="${SVDO_TYPESAFE_EXPERIMENT_TYPESAFE_API_KEY_ENV:-TYPESAFE_API_KEY}"
TYPESAFE_URL="${SVDO_TYPESAFE_EXPERIMENT_TYPESAFE_URL:-https://api.typesafe.ai/v1/systemone}"
DRY_RUN="${SVDO_TYPESAFE_EXPERIMENT_DRY_RUN:-0}"
FAIL_FAST="${SVDO_TYPESAFE_EXPERIMENT_FAIL_FAST:-1}"
SUMMARIZE_ONLY="${SVDO_TYPESAFE_EXPERIMENT_SUMMARIZE_ONLY:-0}"

status=0
EVAL_RUN_HELP=""
SUPPORTS_JUDGE_BACKEND=0
SUPPORTS_JUDGE_HARNESS=0

note() {
  printf '\n==> %s\n' "$*" >&2
}

validate_positive_integer() {
  case "$1" in
    ''|*[!0-9]*|0)
      printf '%s must be a positive integer, got: %s\n' "$2" "$1" >&2
      exit 2
      ;;
  esac
}

run_eval() {
  local backend="$1"
  local iteration="$2"
  local output_file="$RAW_DIR/$(printf '%03d' "$iteration")-$backend.json"
  shift 2

  printf '+'
  printf ' %q' "$@"
  printf ' > %q\n' "$output_file"

  if [ "$DRY_RUN" = "1" ]; then
    printf '# would write %s\n' "$output_file"
    return 0
  fi

  "$@" >"$output_file"
  local eval_status=$?
  if [ "$eval_status" -ne 0 ]; then
    printf '%s iteration %s exited with status %s\n' "$backend" "$iteration" "$eval_status" >&2
    print_eval_failure "$output_file" >&2
    status=1
    if [ "$FAIL_FAST" = "1" ]; then
      printf 'stopping after first failed eval; set SVDO_TYPESAFE_EXPERIMENT_FAIL_FAST=0 to continue.\n' >&2
      exit "$eval_status"
    fi
  fi
  return 0
}

print_eval_failure() {
  local output_file="$1"
  if [ ! -s "$output_file" ]; then
    printf 'no JSON output was captured in %s\n' "$output_file"
    return 0
  fi

  python3 - "$output_file" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
try:
    report = json.loads(path.read_text(encoding="utf-8"))
except json.JSONDecodeError as error:
    print(f"captured output is not valid JSON: {path}: {error}")
    return_code = 0
else:
    for result in report.get("results") or []:
        print(
            f"eval {result.get('id', '<unknown>')} score "
            f"{result.get('overall_score', '-')} passed={result.get('passed', '-')}"
        )
        for check in result.get("checks") or []:
            if check.get("outcome") != "passed":
                print(
                    f"  check {check.get('id', '<unknown>')} "
                    f"[{check.get('type', '-')}] outcome={check.get('outcome', '-')} "
                    f"score={check.get('score', '-')}"
                )
                for violation in check.get("violations") or []:
                    print(f"    - {violation}")
        for violation in result.get("violations") or []:
            print(f"  overall: {violation}")
PY
}

preflight_svdo_meter() {
  if [ "$DRY_RUN" != "1" ] && ! command -v "$SVDO_METER_BIN" >/dev/null 2>&1; then
    printf 'svdo-meter was not found. Build this repo, install it globally, add it to PATH, or set SVDO_METER_BIN.\n' >&2
    exit 127
  fi

  EVAL_RUN_HELP="$("$SVDO_METER_BIN" eval run --help 2>&1)"
  case "$EVAL_RUN_HELP" in
    *"--harness"*) SUPPORTS_JUDGE_HARNESS=1 ;;
  esac
  case "$EVAL_RUN_HELP" in
    *"--judge-backend"*) SUPPORTS_JUDGE_BACKEND=1 ;;
  esac

  if [ "$SUPPORTS_JUDGE_HARNESS" != "1" ]; then
    printf '%s does not support `svdo-meter eval run --harness`, which is required for the Codex judge comparison.\n' "$SVDO_METER_BIN" >&2
    printf 'Build the current repo and rerun with `SVDO_METER_BIN=target/debug/svdo-meter`.\n' >&2
    exit 2
  fi

  if [ "$SUPPORTS_JUDGE_BACKEND" != "1" ]; then
    printf '%s does not support `--judge-backend typesafe`; TypeSafe runs will use the eval file judge config instead.\n' "$SVDO_METER_BIN" >&2
    printf 'Set `SVDO_METER_BIN=target/debug/svdo-meter` to use the explicit TypeSafe CLI flags.\n' >&2
  fi
}

write_summary() {
  python3 - "$RAW_DIR" "$SUMMARY_JSON" "$SUMMARY_CSV" "$SUMMARY_TXT" <<'PY'
import csv
import json
import statistics
import sys
from pathlib import Path


raw_dir = Path(sys.argv[1])
summary_json = Path(sys.argv[2])
summary_csv = Path(sys.argv[3])
summary_txt = Path(sys.argv[4])


def mean(values):
    return statistics.fmean(values) if values else None


def median(values):
    return statistics.median(values) if values else None


def rounded(value, digits=4):
    return None if value is None else round(value, digits)


def fmt(value, digits=4):
    if value is None:
        return "-"
    return f"{value:.{digits}f}"


def fmt_ratio(value, digits=2):
    if value is None:
        return "-"
    return f"{value:.{digits}f}x"


def extract_record(path):
    contents = path.read_text(encoding="utf-8")
    if not contents.strip():
        return None, f"skipped empty artifact: {path}"
    try:
        report = json.loads(contents)
    except json.JSONDecodeError as error:
        return None, f"skipped invalid JSON artifact: {path}: {error}"
    stem = path.stem
    iteration, backend = stem.split("-", 1)
    result = (report.get("results") or [{}])[0]
    checks = result.get("checks") or []
    judge_checks = [check for check in checks if check.get("type") == "judge"]
    judge = judge_checks[0] if judge_checks else {}
    usage = judge.get("token_usage") or {}
    return {
        "iteration": int(iteration),
        "backend": backend,
        "passed": bool(report.get("passed")),
        "eval_duration_ms": number(report.get("duration_ms")),
        "result_duration_ms": number(result.get("duration_ms")),
        "overall_score": number(result.get("overall_score")),
        "judge_score": number(judge.get("score")),
        "judge_duration_ms": number(judge.get("duration_ms")),
        "judge_outcome": judge.get("outcome"),
        "judge_harness": judge.get("harness"),
        "judge_model": judge.get("model"),
        "judge_input_tokens": number(usage.get("input")),
        "judge_output_tokens": number(usage.get("output")),
        "judge_cache_read_tokens": number(usage.get("cache_read")),
        "judge_total_tokens": number(usage.get("total")),
        "violations": len(result.get("violations") or []),
        "path": str(path),
    }, None


def number(value):
    if isinstance(value, (int, float)):
        return float(value)
    return None


def summarize(records):
    passed = [record for record in records if record["passed"]]
    eval_durations = compact(record["eval_duration_ms"] for record in records)
    judge_durations = compact(record["judge_duration_ms"] for record in records)
    scores = compact(record["overall_score"] for record in records)
    judge_scores = compact(record["judge_score"] for record in records)
    input_tokens = compact(record["judge_input_tokens"] for record in records)
    output_tokens = compact(record["judge_output_tokens"] for record in records)
    cache_read_tokens = compact(record["judge_cache_read_tokens"] for record in records)
    total_tokens = compact(record["judge_total_tokens"] for record in records)
    return {
        "iterations": len(records),
        "passed": len(passed),
        "pass_rate": rounded(len(passed) / len(records), 4) if records else None,
        "overall_score_mean": rounded(mean(scores)),
        "judge_score_mean": rounded(mean(judge_scores)),
        "eval_duration_ms_mean": rounded(mean(eval_durations), 1),
        "eval_duration_ms_p50": rounded(median(eval_durations), 1),
        "eval_duration_ms_min": rounded(min(eval_durations), 1) if eval_durations else None,
        "eval_duration_ms_max": rounded(max(eval_durations), 1) if eval_durations else None,
        "judge_duration_ms_mean": rounded(mean(judge_durations), 1),
        "judge_duration_ms_p50": rounded(median(judge_durations), 1),
        "judge_duration_ms_min": rounded(min(judge_durations), 1) if judge_durations else None,
        "judge_duration_ms_max": rounded(max(judge_durations), 1) if judge_durations else None,
        "total_eval_duration_ms": rounded(sum(eval_durations), 1),
        "total_judge_duration_ms": rounded(sum(judge_durations), 1),
        "judge_input_tokens_mean": rounded(mean(input_tokens), 1),
        "judge_input_tokens_sum": rounded(sum(input_tokens), 1) if input_tokens else None,
        "judge_output_tokens_mean": rounded(mean(output_tokens), 1),
        "judge_output_tokens_sum": rounded(sum(output_tokens), 1) if output_tokens else None,
        "judge_cache_read_tokens_mean": rounded(mean(cache_read_tokens), 1),
        "judge_cache_read_tokens_sum": rounded(sum(cache_read_tokens), 1) if cache_read_tokens else None,
        "judge_total_tokens_mean": rounded(mean(total_tokens), 1),
        "judge_total_tokens_sum": rounded(sum(total_tokens), 1) if total_tokens else None,
        "violations": sum(record["violations"] for record in records),
    }


def compact(values):
    return [value for value in values if isinstance(value, (int, float))]


records = []
warnings = []
for path in sorted(raw_dir.glob("*.json")):
    record, warning = extract_record(path)
    if warning:
        warnings.append(warning)
    if record:
        records.append(record)
by_backend = {}
for record in records:
    by_backend.setdefault(record["backend"], []).append(record)

summaries = {
    backend: summarize(backend_records)
    for backend, backend_records in sorted(by_backend.items())
}

codex = summaries.get("codex", {})
typesafe = summaries.get("typesafe", {})
comparison = {}
for metric in ("judge_duration_ms_mean", "eval_duration_ms_mean", "total_judge_duration_ms", "total_eval_duration_ms"):
    left = codex.get(metric)
    right = typesafe.get(metric)
    comparison[f"{metric}_speedup"] = rounded(left / right, 2) if left and right else None

for metric in (
    "judge_input_tokens_mean",
    "judge_output_tokens_mean",
    "judge_cache_read_tokens_mean",
    "judge_total_tokens_mean",
    "judge_input_tokens_sum",
    "judge_output_tokens_sum",
    "judge_cache_read_tokens_sum",
    "judge_total_tokens_sum",
):
    left = codex.get(metric)
    right = typesafe.get(metric)
    comparison[f"{metric}_ratio"] = rounded(left / right, 2) if left and right else None

comparison["overall_score_delta"] = rounded(
    abs((codex.get("overall_score_mean") or 0.0) - (typesafe.get("overall_score_mean") or 0.0))
    if codex.get("overall_score_mean") is not None and typesafe.get("overall_score_mean") is not None
    else None
)
comparison["judge_score_delta"] = rounded(
    abs((codex.get("judge_score_mean") or 0.0) - (typesafe.get("judge_score_mean") or 0.0))
    if codex.get("judge_score_mean") is not None and typesafe.get("judge_score_mean") is not None
    else None
)

artifact = {
    "records": records,
    "summaries": summaries,
    "comparison": comparison,
    "warnings": warnings,
}
summary_json.write_text(json.dumps(artifact, indent=2) + "\n", encoding="utf-8")

with summary_csv.open("w", encoding="utf-8", newline="") as handle:
    writer = csv.DictWriter(handle, fieldnames=list(records[0].keys()) if records else [])
    if records:
        writer.writeheader()
        writer.writerows(records)

lines = []
lines.append("TypeSafe judge speed experiment")
lines.append("=" * 32)
lines.append("")
lines.append(
    "Backend   n   pass   score   judge score   mean judge ms   p50 judge ms   mean eval ms"
)
lines.append(
    "-------  --  -----  ------  -----------   -------------   ------------   ------------"
)
for backend in ("codex", "typesafe"):
    summary = summaries.get(backend, {})
    lines.append(
        f"{backend:<8} "
        f"{summary.get('iterations', 0):>2}  "
        f"{summary.get('passed', 0):>2}/{summary.get('iterations', 0):<2}  "
        f"{fmt(summary.get('overall_score_mean')):>6}  "
        f"{fmt(summary.get('judge_score_mean')):>11}   "
        f"{fmt(summary.get('judge_duration_ms_mean'), 1):>13}   "
        f"{fmt(summary.get('judge_duration_ms_p50'), 1):>12}   "
        f"{fmt(summary.get('eval_duration_ms_mean'), 1):>12}"
    )
lines.append("")
lines.append("Judge Token Usage")
lines.append("-----------------")
lines.append(
    "Backend   mean input   mean output   mean cache   mean total   total input   total output   total tokens"
)
lines.append(
    "-------  -----------  ------------  -----------  -----------  ------------  -------------  ------------"
)
for backend in ("codex", "typesafe"):
    summary = summaries.get(backend, {})
    lines.append(
        f"{backend:<8} "
        f"{fmt(summary.get('judge_input_tokens_mean'), 1):>11}  "
        f"{fmt(summary.get('judge_output_tokens_mean'), 1):>12}  "
        f"{fmt(summary.get('judge_cache_read_tokens_mean'), 1):>11}  "
        f"{fmt(summary.get('judge_total_tokens_mean'), 1):>11}  "
        f"{fmt(summary.get('judge_input_tokens_sum'), 1):>12}  "
        f"{fmt(summary.get('judge_output_tokens_sum'), 1):>13}  "
        f"{fmt(summary.get('judge_total_tokens_sum'), 1):>12}"
    )
lines.append("")
lines.append(f"Mean judge speedup: {fmt_ratio(comparison.get('judge_duration_ms_mean_speedup'))}")
lines.append(f"Mean eval speedup:  {fmt_ratio(comparison.get('eval_duration_ms_mean_speedup'))}")
lines.append(f"Mean total token ratio: {fmt_ratio(comparison.get('judge_total_tokens_mean_ratio'))}")
lines.append(f"Total token ratio:      {fmt_ratio(comparison.get('judge_total_tokens_sum_ratio'))}")
lines.append(f"Score delta:        {fmt(comparison.get('overall_score_delta'), 4)}")
lines.append(f"Judge score delta:  {fmt(comparison.get('judge_score_delta'), 4)}")
if warnings:
    lines.append("")
    lines.append("Warnings")
    lines.extend(f"- {warning}" for warning in warnings)
summary_txt.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(summary_txt.read_text(encoding="utf-8"))
PY
  local summary_status=$?
  if [ "$summary_status" -ne 0 ]; then
    printf 'failed to summarize raw eval artifacts in %s\n' "$RAW_DIR" >&2
    status=1
  fi
}

validate_positive_integer "$ITERATIONS" "SVDO_TYPESAFE_EXPERIMENT_ITERATIONS"

if [ "$SUMMARIZE_ONLY" = "1" ] && [ -z "${SVDO_TYPESAFE_EXPERIMENT_OUTPUT_DIR:-}" ]; then
  latest_output_dir="$(find "$OUTPUT_ROOT" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort | tail -1)"
  if [ -z "$latest_output_dir" ]; then
    printf 'no experiment output directories found under %s\n' "$OUTPUT_ROOT" >&2
    exit 2
  fi
  OUTPUT_DIR="$latest_output_dir"
  RAW_DIR="$OUTPUT_DIR/raw"
  SUMMARY_JSON="$OUTPUT_DIR/summary.json"
  SUMMARY_CSV="$OUTPUT_DIR/summary.csv"
  SUMMARY_TXT="$OUTPUT_DIR/summary.txt"
  RUN_LOG="$OUTPUT_DIR/run.log"
fi

if [ "$DRY_RUN" != "1" ] && ! command -v python3 >/dev/null 2>&1; then
  printf 'python3 is required to summarize eval JSON artifacts.\n' >&2
  exit 127
fi

if [ "$SUMMARIZE_ONLY" = "1" ]; then
  if [ ! -d "$RAW_DIR" ]; then
    printf 'raw artifact directory not found: %s\n' "$RAW_DIR" >&2
    exit 2
  fi
  write_summary
  printf '\nRegenerated summaries from existing raw artifacts:\n  %s\n  %s\n  %s\n' "$SUMMARY_TXT" "$SUMMARY_JSON" "$SUMMARY_CSV"
  exit "$status"
fi

preflight_svdo_meter

if [ "$DRY_RUN" != "1" ] && [ -z "${!TYPESAFE_API_KEY_ENV:-}" ]; then
  printf '%s is required for TypeSafe judge runs.\n' "$TYPESAFE_API_KEY_ENV" >&2
  exit 2
fi

mkdir -p "$RAW_DIR" || exit 1
: >"$RUN_LOG" || exit 1

cat >"$OUTPUT_DIR/experiment-info.txt" <<EOF
TypeSafe judge speed experiment

Workspace:        $WORKSPACE
Eval:             $EVAL_ID
Iterations:       $ITERATIONS paired runs
Codex model:      $CODEX_MODEL
TypeSafe model:   $TYPESAFE_MODEL
TypeSafe URL:     $TYPESAFE_URL
Raw artifacts:    $RAW_DIR
Summary JSON:     $SUMMARY_JSON
Summary CSV:      $SUMMARY_CSV
Summary text:     $SUMMARY_TXT
Run log:          $RUN_LOG
EOF

exec > >(tee -a "$RUN_LOG") 2>&1

cat "$OUTPUT_DIR/experiment-info.txt"

iteration=1
while [ "$iteration" -le "$ITERATIONS" ]; do
  note "iteration $iteration of $ITERATIONS"

  typesafe_args=(
    "$SVDO_METER_BIN" eval run "$EVAL_ID"
    --workspace "$WORKSPACE"
    --format json
  )
  if [ "$SUPPORTS_JUDGE_BACKEND" = "1" ]; then
    typesafe_args+=(
      --judge-backend typesafe
      --typesafe-model "$TYPESAFE_MODEL"
      --typesafe-api-key-env "$TYPESAFE_API_KEY_ENV"
      --typesafe-url "$TYPESAFE_URL"
    )
  fi
  codex_args=(
    "$SVDO_METER_BIN" eval run "$EVAL_ID"
    --workspace "$WORKSPACE"
    --harness codex
    --model "$CODEX_MODEL"
    --format json
  )

  if [ $((iteration % 2)) -eq 1 ]; then
    run_eval typesafe "$iteration" "${typesafe_args[@]}"
    run_eval codex "$iteration" "${codex_args[@]}"
  else
    run_eval codex "$iteration" "${codex_args[@]}"
    run_eval typesafe "$iteration" "${typesafe_args[@]}"
  fi

  iteration=$((iteration + 1))
done

if [ "$DRY_RUN" != "1" ]; then
  note "summary"
  write_summary
fi

cat <<EOF

Experiment artifacts are in:
  $OUTPUT_DIR

Raw eval JSON:
  $RAW_DIR

Summaries:
  $SUMMARY_TXT
  $SUMMARY_JSON
  $SUMMARY_CSV
EOF

exit "$status"
