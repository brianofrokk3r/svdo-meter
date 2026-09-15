#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"

WORK_ID="${SVDO_LABEL_WORK_ID:-LABEL-TODO-DUEDATES}"
WORKSPACE="${SVDO_LABEL_WORKSPACE:-}"
HARNESS="${SVDO_LABEL_HARNESS:-codex}"
MODEL="${SVDO_LABEL_MODEL:-gpt-5.5}"
SVDO_METER_BIN="${SVDO_METER_BIN:-svdo-meter}"
DANGEROUS_BYPASS="${SVDO_LABEL_DANGEROUS_BYPASS:-0}"

usage() {
  cat <<'USAGE'
Usage: run-plan-implement-report.sh [--work-id ID] [--workspace PATH]
                                    [--harness HARNESS] [--model MODEL]
                                    [--dangerous-bypass]

Runs the label-grouped reporting workflow end to end:

  1. copy examples/todo-cli into a disposable workspace
  2. run svdo-meter with --label plan
  3. run svdo-meter with --label implement
  4. print overall, per-label, and grouped report output

Environment overrides:
  SVDO_LABEL_WORK_ID
  SVDO_LABEL_WORKSPACE
  SVDO_LABEL_HARNESS
  SVDO_LABEL_MODEL
  SVDO_LABEL_DANGEROUS_BYPASS=1
  SVDO_METER_BIN
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-id|--ticket)
      WORK_ID="${2:?missing value for $1}"
      shift 2
      ;;
    --workspace)
      WORKSPACE="${2:?missing value for --workspace}"
      shift 2
      ;;
    --harness)
      HARNESS="${2:?missing value for --harness}"
      shift 2
      ;;
    --model)
      MODEL="${2:?missing value for --model}"
      shift 2
      ;;
    --dangerous-bypass)
      DANGEROUS_BYPASS=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$WORKSPACE" ]]; then
  WORKSPACE="$(mktemp -d "${TMPDIR:-/tmp}/svdo-label-grouped-reporting.XXXXXX")"
else
  if [[ -e "$WORKSPACE" && -n "$(find "$WORKSPACE" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]]; then
    echo "workspace already exists and is not empty: $WORKSPACE" >&2
    echo "choose an empty --workspace or omit it to use a fresh temporary directory" >&2
    exit 1
  fi
  mkdir -p "$WORKSPACE"
fi

PLAN_PROMPT="$SCRIPT_DIR/prompts/plan-due-dates.md"
IMPLEMENT_PROMPT="$SCRIPT_DIR/prompts/implement-due-dates.md"
REPORTER="$SCRIPT_DIR/report-label-groups.sh"

cp -R "$REPO_ROOT/examples/todo-cli/." "$WORKSPACE/"

run_args=(
  --ticket "$WORK_ID"
  --harness "$HARNESS"
  --workspace "$WORKSPACE"
)

if [[ -n "$MODEL" ]]; then
  run_args+=(--model "$MODEL")
fi

if [[ "$DANGEROUS_BYPASS" == "1" || "$DANGEROUS_BYPASS" == "true" ]]; then
  run_args+=(--dangerous-bypass)
fi

printf 'SVDO Label Grouped Workflow\n'
printf 'Work: %s\n' "$WORK_ID"
printf 'Workspace: %s\n' "$WORKSPACE"
printf 'Harness: %s\n' "$HARNESS"
if [[ -n "$MODEL" ]]; then
  printf 'Model: %s\n' "$MODEL"
fi
printf '\n'

printf '== Plan phase ==\n'
"$SVDO_METER_BIN" run "${run_args[@]}" --label plan --prompt-file "$PLAN_PROMPT"
printf '\n'

printf '== Implement phase ==\n'
"$SVDO_METER_BIN" run "${run_args[@]}" --label implement --prompt-file "$IMPLEMENT_PROMPT"
printf '\n'

printf '== Overall report ==\n'
"$SVDO_METER_BIN" report "$WORK_ID" --workspace "$WORKSPACE"
printf '\n'

printf '== Plan report ==\n'
"$SVDO_METER_BIN" report "$WORK_ID" --label plan --workspace "$WORKSPACE"
printf '\n'

printf '== Implement report ==\n'
"$SVDO_METER_BIN" report "$WORK_ID" --label implement --workspace "$WORKSPACE"
printf '\n'

printf '== Label group report ==\n'
SVDO_METER_BIN="$SVDO_METER_BIN" "$REPORTER" "$WORK_ID" --workspace "$WORKSPACE"
