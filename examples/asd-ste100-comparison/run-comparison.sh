#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SVDO_METER_BIN="${SVDO_METER_BIN:-svdo-meter}"
WORK="${SVDO_ASD_WORK:-ASD-STE100-TODO-$(date +%Y%m%d-%H%M%S)}"
WORKSPACE="${SVDO_ASD_WORKSPACE:-$(mktemp -d -t svdo-asd-ste100-todo.XXXXXX)}"
RUN_WORKSPACES_DIR="$WORKSPACE/runs"
OUTPUT_ROOT="${SVDO_ASD_OUTPUT_ROOT:-$SCRIPT_DIR/study-output}"
OUTPUT_DIR="${SVDO_ASD_OUTPUT_DIR:-$OUTPUT_ROOT/$WORK}"
RUN_LOG="$OUTPUT_DIR/run.log"
REPORT_OUTPUT="$OUTPUT_DIR/svdo-report.txt"
COMPARE_OUTPUT="$OUTPUT_DIR/svdo-compare.txt"
SUMMARY_OUTPUT="$OUTPUT_DIR/comparison-summary.txt"
HARNESS="${SVDO_ASD_HARNESS:-codex}"
MODEL="${SVDO_ASD_MODEL:-gpt-5.5}"
REPETITIONS="${SVDO_ASD_REPETITIONS:-10}"
VARIANTS="${SVDO_ASD_VARIANTS:-standard asd-ste100}"
DRY_RUN="${SVDO_ASD_DRY_RUN:-0}"
RUN_EVALS="${SVDO_ASD_RUN_EVALS:-1}"
RUN_JUDGE="${SVDO_ASD_JUDGE:-1}"
DANGEROUS_BYPASS="${SVDO_ASD_DANGEROUS_BYPASS:-0}"
OPENCODE_AGENT="${SVDO_ASD_OPENCODE_AGENT:-build}"

status=0
codex_skip_git_repo_check=0

note() {
  printf '\n==> %s\n' "$*" >&2
}

prompt_for_variant() {
  case "$1" in
    standard) printf '%s/prompts/standard.md\n' "$SCRIPT_DIR" ;;
    asd-ste100) printf '%s/prompts/asd-ste100.md\n' "$SCRIPT_DIR" ;;
    *) return 1 ;;
  esac
}

run_or_print() {
  printf '+'
  printf ' %q' "$@"
  printf '\n'

  if [[ "$DRY_RUN" == "1" ]]; then
    return 0
  fi

  "$@"
  local step_status=$?
  if [[ "$step_status" -ne 0 ]]; then
    printf 'step exited with status %s: %s\n' "$step_status" "$*" >&2
    status=1
  fi
  return 0
}

run_and_save() {
  local output_file="$1"
  shift

  printf '+'
  printf ' %q' "$@"
  printf ' | tee %q\n' "$output_file"

  if [[ "$DRY_RUN" == "1" ]]; then
    printf '# writes %s\n' "$output_file"
    return 0
  fi

  "$@" 2>&1 | tee "$output_file"
  local step_status=${PIPESTATUS[0]}
  if [[ "$step_status" -ne 0 ]]; then
    printf 'step exited with status %s: %s\n' "$step_status" "$*" >&2
    status=1
  fi
  return 0
}

validate_positive_integer() {
  case "$1" in
    ''|*[!0-9]*|0)
      printf '%s must be a positive integer, got: %s\n' "$2" "$1" >&2
      exit 2
      ;;
  esac
}

if [[ "$DRY_RUN" != "1" ]] && ! command -v "$SVDO_METER_BIN" >/dev/null 2>&1; then
  printf 'svdo-meter was not found. Install it, add it to PATH, or set SVDO_METER_BIN.\n' >&2
  exit 127
fi

validate_positive_integer "$REPETITIONS" "SVDO_ASD_REPETITIONS"

if [[ "$DRY_RUN" != "1" && "$HARNESS" == "codex" ]]; then
  run_help="$("$SVDO_METER_BIN" run --help 2>/dev/null || true)"
  if [[ "$run_help" == *"--codex-skip-git-repo-check"* ]]; then
    codex_skip_git_repo_check=1
  fi
fi

mkdir -p "$OUTPUT_DIR" || exit 1
: >"$RUN_LOG" || exit 1
cat >"$OUTPUT_DIR/study-info.txt" <<EOF
SVDO Meter ASD-STE100-style prompt comparison

Work:        $WORK
Workspace:   $WORKSPACE
Output dir:  $OUTPUT_DIR
Run log:     $RUN_LOG
Report:      $REPORT_OUTPUT
Compare:     $COMPARE_OUTPUT
Summary:     $SUMMARY_OUTPUT
Harness:     $HARNESS
Model:       $MODEL
Repetitions: $REPETITIONS per variant
Variants:    $VARIANTS
Ticket IDs:  $WORK-<label>
EOF
exec > >(tee -a "$RUN_LOG") 2>&1

mkdir -p "$WORKSPACE/.svdo/evals" "$WORKSPACE/.svdo/meter" "$WORKSPACE/.svdo/standards" "$RUN_WORKSPACES_DIR"
cp "$SCRIPT_DIR/.svdo/evals/expanded-todo-cli.yaml" "$WORKSPACE/.svdo/evals/"
cp "$SCRIPT_DIR/.svdo/standards/expanded-todo-cli-quality.md" "$WORKSPACE/.svdo/standards/"

cat <<EOF
SVDO Meter ASD-STE100-style prompt comparison

Work:        $WORK
Workspace:   $WORKSPACE
Harness:     $HARNESS
Model:       $MODEL
Repetitions: $REPETITIONS per variant
Variants:    $VARIANTS
Dry run:     $DRY_RUN
Ticket IDs:  $WORK-<label>

Attempt workspaces: $RUN_WORKSPACES_DIR/
Aggregate telemetry: $WORKSPACE/.svdo/meter/
Aggregate evals:     $WORKSPACE/.svdo/evals/
Persistent output:   $OUTPUT_DIR/
EOF

for variant in $VARIANTS; do
  prompt_file="$(prompt_for_variant "$variant")" || {
    printf 'unknown variant id: %s\n' "$variant" >&2
    exit 2
  }

  if [[ ! -f "$prompt_file" ]]; then
    printf 'prompt file not found for %s: %s\n' "$variant" "$prompt_file" >&2
    exit 1
  fi

  rep=1
  while [[ "$rep" -le "$REPETITIONS" ]]; do
    label="$(printf '%s-%03d' "$variant" "$rep")"
    run_ticket="$WORK-$label"
    run_workspace="$(mktemp -d "$RUN_WORKSPACES_DIR/$label.XXXXXX")" || exit 1

    mkdir -p "$run_workspace/.svdo/evals" "$run_workspace/.svdo/standards" || exit 1
    cp "$prompt_file" "$run_workspace/TASK.md"
    cp "$SCRIPT_DIR/.svdo/evals/expanded-todo-cli.yaml" "$run_workspace/.svdo/evals/"
    cp "$SCRIPT_DIR/.svdo/standards/expanded-todo-cli-quality.md" "$run_workspace/.svdo/standards/"

    note "run $label"

    run_args=(
      "$SVDO_METER_BIN" run
      --ticket "$run_ticket"
      --label "$label"
      --harness "$HARNESS"
      --workspace "$run_workspace"
      --model "$MODEL"
      --prompt-file "$run_workspace/TASK.md"
    )

    if [[ "$DANGEROUS_BYPASS" == "1" ]]; then
      run_args+=(--dangerous-bypass)
    fi

    if [[ "$HARNESS" == "codex" ]]; then
      if [[ "$codex_skip_git_repo_check" == "1" ]]; then
        run_args+=(--codex-skip-git-repo-check)
      elif [[ "$DRY_RUN" != "1" && ! -d "$run_workspace/.git" ]] && command -v git >/dev/null 2>&1; then
        git -C "$run_workspace" init --quiet
      fi
    elif [[ "$HARNESS" == "opencode" ]]; then
      run_args+=(--opencode-agent "$OPENCODE_AGENT")
    fi

    run_or_print "${run_args[@]}"

    if [[ "$DRY_RUN" != "1" ]]; then
      for stream in "$run_workspace"/.svdo/meter/*.jsonl; do
        [[ -e "$stream" ]] || continue
        cp "$stream" "$WORKSPACE/.svdo/meter/${label}-$(basename "$stream")"
      done
    fi

    if [[ "$RUN_EVALS" == "1" ]]; then
      eval_artifact="$run_workspace/.svdo/evals/${label}-eval.json"
      aggregate_eval_artifact="$WORKSPACE/.svdo/evals/${label}-eval.json"
      eval_args=(
        "$SVDO_METER_BIN" eval run expanded-todo-cli
        --workspace "$run_workspace"
        --format json
      )

      if [[ "$RUN_JUDGE" == "1" ]]; then
        eval_args+=(--harness "$HARNESS" --model "$MODEL")
      fi

      if [[ "$DRY_RUN" == "1" ]]; then
        run_or_print "${eval_args[@]}"
        printf '# writes %s\n' "$eval_artifact"
        printf '# copies %s to %s\n' "$eval_artifact" "$aggregate_eval_artifact"
      else
        note "eval $label"
        printf '+'
        printf ' %q' "${eval_args[@]}"
        printf ' > %q\n' "$eval_artifact"
        "${eval_args[@]}" >"$eval_artifact"
        eval_status=$?
        if [[ "$eval_status" -ne 0 ]]; then
          printf 'eval exited with status %s for %s\n' "$eval_status" "$label" >&2
          status=1
        fi
        cp "$eval_artifact" "$aggregate_eval_artifact"
      fi
    fi

    rep=$((rep + 1))
  done
done

note "native report"
run_and_save "$REPORT_OUTPUT" "$SVDO_METER_BIN" report --workspace "$WORKSPACE"

note "native compare"
run_and_save "$COMPARE_OUTPUT" "$SVDO_METER_BIN" compare --workspace "$WORKSPACE"

note "comparison summary"
run_and_save "$SUMMARY_OUTPUT" "$SCRIPT_DIR/report-comparison.sh" "$WORK" --workspace "$WORKSPACE" --baseline standard

cat <<EOF

Comparison workspace:
  $WORKSPACE

Persistent results:
  $OUTPUT_DIR

Saved reports:
  $REPORT_OUTPUT
  $COMPARE_OUTPUT
  $SUMMARY_OUTPUT
EOF

exit "$status"
