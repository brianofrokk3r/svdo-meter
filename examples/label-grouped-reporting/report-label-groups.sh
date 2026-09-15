#!/usr/bin/env bash
set -euo pipefail

WORK_ID="LABEL-TODO-DUEDATES"
WORKSPACE="examples/label-grouped-reporting"
LABELS=("plan" "implement")
SVDO_METER_BIN="${SVDO_METER_BIN:-svdo-meter}"

if [[ $# -gt 0 && "$1" != "--workspace" ]]; then
  WORK_ID="$1"
  shift
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --workspace)
      WORKSPACE="${2:?missing value for --workspace}"
      shift 2
      ;;
    --label)
      LABELS+=("${2:?missing value for --label}")
      shift 2
      ;;
    --help|-h)
      cat <<'USAGE'
Usage: report-label-groups.sh [WORK_ID] [--workspace PATH] [--label LABEL]

Summarize svdo-meter report output for workflow phase labels. Defaults to
WORK_ID=LABEL-TODO-DUEDATES, workspace=examples/label-grouped-reporting, and
labels plan and implement.
USAGE
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ ! -d "$WORKSPACE/.svdo/meter" ]]; then
  echo "no telemetry directory found at $WORKSPACE/.svdo/meter" >&2
  exit 1
fi

format_ms() {
  local ms="${1:-0}"
  local seconds=$((ms / 1000))
  local minutes=$((seconds / 60))
  seconds=$((seconds % 60))
  if (( minutes > 0 )); then
    printf '%dm %02ds' "$minutes" "$seconds"
  else
    printf '%ds' "$seconds"
  fi
}

read_label_report() {
  local label="$1"
  local csv
  csv="$("$SVDO_METER_BIN" report "$WORK_ID" --label "$label" --workspace "$WORKSPACE" --format csv)"
  awk -F, -v label="$label" 'NR > 1 {
      runs += $4
      time += $5
      input += $6
      output += $7
    } END {
      printf "%s,%d,%d,%d,%d\n", label, runs, time, input, output
    }' <<<"$csv"
}

printf 'SVDO Label Group Report - %s\n' "$WORK_ID"
printf 'Workspace: %s\n\n' "$WORKSPACE"
printf '%-10s  %4s  %-10s  %12s  %13s\n' "Label" "Runs" "Agent time" "Input tokens" "Output tokens"
printf '%-10s  %4s  %-10s  %12s  %13s\n' "----------" "----" "----------" "------------" "-------------"

total_runs=0
total_time=0
total_input=0
total_output=0
implement_time=0

for label in "${LABELS[@]}"; do
  IFS=, read -r name runs time input output < <(read_label_report "$label")
  total_runs=$((total_runs + runs))
  total_time=$((total_time + time))
  total_input=$((total_input + input))
  total_output=$((total_output + output))
  if [[ "$name" == "implement" ]]; then
    implement_time="$time"
  fi
  printf '%-10s  %4d  %-10s  %12d  %13d\n' "$name" "$runs" "$(format_ms "$time")" "$input" "$output"
done

printf '%-10s  %4s  %-10s  %12s  %13s\n' "----------" "----" "----------" "------------" "-------------"
printf '%-10s  %4d  %-10s  %12d  %13d\n\n' "total" "$total_runs" "$(format_ms "$total_time")" "$total_input" "$total_output"

if (( total_time > 0 && implement_time > 0 )); then
  awk -v part="$implement_time" -v total="$total_time" 'BEGIN {
    printf "Insight: implement used %.1f%% of measured agent time.\n", (part / total) * 100
  }'
fi
