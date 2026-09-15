#!/bin/sh
if [ -z "${BASH_VERSION:-}" ]; then
  exec /usr/bin/env bash "$0" "$@"
fi
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"

WORK_ID="LABEL-TODO-DUEDATES"
WORKSPACE="examples/label-grouped-reporting"
LABELS=("plan" "implement")
SVDO_METER_BIN="${SVDO_METER_BIN:-svdo-meter}"
PRICING_FILE=""

if [[ $# -gt 0 && "$1" != "--workspace" && "$1" != "--label" && "$1" != "--pricing-file" && "$1" != "--help" && "$1" != "-h" ]]; then
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
    --pricing-file)
      PRICING_FILE="${2:?missing value for --pricing-file}"
      shift 2
      ;;
    --help|-h)
      cat <<'USAGE'
Usage: report-label-groups.sh [WORK_ID] [--workspace PATH] [--label LABEL]
                              [--pricing-file PATH]

Summarize svdo-meter report output for workflow phase labels. Defaults to
WORK_ID=LABEL-TODO-DUEDATES, workspace=examples/label-grouped-reporting, and
labels plan and implement.

When --pricing-file is provided, include estimated spend by label using
svdo-meter report's pricing support.
USAGE
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

resolve_path() {
  local path="$1"
  if [[ -e "$path" ]]; then
    printf '%s\n' "$path"
  elif [[ -e "$REPO_ROOT/$path" ]]; then
    printf '%s\n' "$REPO_ROOT/$path"
  else
    printf '%s\n' "$path"
  fi
}

WORKSPACE_DISPLAY="$WORKSPACE"
WORKSPACE="$(resolve_path "$WORKSPACE")"

if [[ -n "$PRICING_FILE" ]]; then
  PRICING_FILE_DISPLAY="$PRICING_FILE"
  PRICING_FILE="$(resolve_path "$PRICING_FILE")"
fi

if [[ ! -d "$WORKSPACE/.svdo/meter" ]]; then
  echo "no telemetry directory found at $WORKSPACE_DISPLAY/.svdo/meter" >&2
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
  local report_args=("$WORK_ID" --label "$label" --workspace "$WORKSPACE" --format csv)
  if [[ -n "$PRICING_FILE" ]]; then
    report_args+=(--pricing-file "$PRICING_FILE")
  fi
  csv="$("$SVDO_METER_BIN" report "${report_args[@]}")"
  awk -F, -v label="$label" -v include_cost="$PRICING_FILE" 'NR > 1 {
      runs += $4
      time += $5
      input += $6
      output += $7
      if (include_cost != "" && $13 != "") {
        cost += $13
        has_cost = 1
      }
    } END {
      if (include_cost != "" && has_cost) {
        printf "%s,%d,%d,%d,%d,%.10f\n", label, runs, time, input, output, cost
      } else if (include_cost != "") {
        printf "%s,%d,%d,%d,%d,\n", label, runs, time, input, output
      } else {
        printf "%s,%d,%d,%d,%d\n", label, runs, time, input, output
      }
    }' <<<"$csv"
}

format_cost() {
  local cost="${1:-}"
  if [[ -z "$cost" ]]; then
    printf 'unavailable'
  else
    awk -v cost="$cost" 'BEGIN { printf "$%.4f", cost }'
  fi
}

printf 'SVDO Label Group Report - %s\n' "$WORK_ID"
printf 'Workspace: %s\n\n' "$WORKSPACE_DISPLAY"
if [[ -n "$PRICING_FILE" ]]; then
  printf 'Pricing: %s\n\n' "$PRICING_FILE_DISPLAY"
  printf '%-10s  %4s  %-10s  %12s  %13s  %11s\n' "Label" "Runs" "Agent time" "Input tokens" "Output tokens" "Spend"
  printf '%-10s  %4s  %-10s  %12s  %13s  %11s\n' "----------" "----" "----------" "------------" "-------------" "-----------"
else
  printf '%-10s  %4s  %-10s  %12s  %13s\n' "Label" "Runs" "Agent time" "Input tokens" "Output tokens"
  printf '%-10s  %4s  %-10s  %12s  %13s\n' "----------" "----" "----------" "------------" "-------------"
fi

total_runs=0
total_time=0
total_input=0
total_output=0
implement_time=0
total_cost=""
implement_cost=""

for label in "${LABELS[@]}"; do
  IFS=, read -r name runs time input output cost < <(read_label_report "$label")
  total_runs=$((total_runs + runs))
  total_time=$((total_time + time))
  total_input=$((total_input + input))
  total_output=$((total_output + output))
  if [[ -n "${cost:-}" ]]; then
    total_cost="$(awk -v a="${total_cost:-0}" -v b="$cost" 'BEGIN { printf "%.10f", a + b }')"
  fi
  if [[ "$name" == "implement" ]]; then
    implement_time="$time"
    implement_cost="${cost:-}"
  fi
  if [[ -n "$PRICING_FILE" ]]; then
    printf '%-10s  %4d  %-10s  %12d  %13d  %11s\n' "$name" "$runs" "$(format_ms "$time")" "$input" "$output" "$(format_cost "${cost:-}")"
  else
    printf '%-10s  %4d  %-10s  %12d  %13d\n' "$name" "$runs" "$(format_ms "$time")" "$input" "$output"
  fi
done

if [[ -n "$PRICING_FILE" ]]; then
  printf '%-10s  %4s  %-10s  %12s  %13s  %11s\n' "----------" "----" "----------" "------------" "-------------" "-----------"
  printf '%-10s  %4d  %-10s  %12d  %13d  %11s\n\n' "total" "$total_runs" "$(format_ms "$total_time")" "$total_input" "$total_output" "$(format_cost "$total_cost")"
else
  printf '%-10s  %4s  %-10s  %12s  %13s\n' "----------" "----" "----------" "------------" "-------------"
  printf '%-10s  %4d  %-10s  %12d  %13d\n\n' "total" "$total_runs" "$(format_ms "$total_time")" "$total_input" "$total_output"
fi

if (( total_time > 0 && implement_time > 0 )); then
  awk -v part="$implement_time" -v total="$total_time" 'BEGIN {
    printf "Insight: implement used %.1f%% of measured agent time.\n", (part / total) * 100
  }'
fi

if [[ -n "$total_cost" && -n "$implement_cost" ]]; then
  awk -v part="$implement_cost" -v total="$total_cost" 'BEGIN {
    if (total > 0) {
      printf "Insight: implement used %.1f%% of estimated spend.\n", (part / total) * 100
    }
  }'
fi
