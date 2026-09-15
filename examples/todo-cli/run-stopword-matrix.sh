#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SVDO_METER_BIN="${SVDO_METER_BIN:-svdo-meter}"
WORK="${SVDO_STOPWORD_WORK:-STOPWORD-TODO-$(date +%Y%m%d-%H%M%S)}"
WORKSPACE="${SVDO_STOPWORD_WORKSPACE:-$(mktemp -d -t svdo-stopword-todo.XXXXXX)}"
HARNESS="${SVDO_STOPWORD_HARNESS:-codex}"
MODEL="${SVDO_STOPWORD_MODEL:-gpt-5.5}"
REPETITIONS="${SVDO_STOPWORD_REPETITIONS:-30}"
VARIANTS="${SVDO_STOPWORD_VARIANTS:-concise-baseline telegraphic-low-stopword stopword-heavy-guided polite-redundant-stopword}"
DRY_RUN="${SVDO_STOPWORD_DRY_RUN:-0}"
RUN_EVALS="${SVDO_STOPWORD_RUN_EVALS:-1}"
RUN_JUDGE="${SVDO_STOPWORD_JUDGE:-0}"
DANGEROUS_BYPASS="${SVDO_STOPWORD_DANGEROUS_BYPASS:-1}"
OPENCODE_AGENT="${SVDO_STOPWORD_OPENCODE_AGENT:-build}"

status=0

note() {
  printf '\n==> %s\n' "$*" >&2
}

prompt_for_variant() {
  case "$1" in
    concise-baseline) printf '%s/prompts/concise-baseline.md\n' "$SCRIPT_DIR" ;;
    telegraphic-low-stopword) printf '%s/prompts/telegraphic-low-stopword.md\n' "$SCRIPT_DIR" ;;
    stopword-heavy-guided) printf '%s/prompts/stopword-heavy-guided.md\n' "$SCRIPT_DIR" ;;
    polite-redundant-stopword) printf '%s/prompts/polite-redundant-stopword.md\n' "$SCRIPT_DIR" ;;
    *) return 1 ;;
  esac
}

run_or_print() {
  printf '+'
  printf ' %q' "$@"
  printf '\n'

  if [ "$DRY_RUN" = "1" ]; then
    return 0
  fi

  "$@"
  local step_status=$?
  if [ "$step_status" -ne 0 ]; then
    printf 'step exited with status %s: %s\n' "$step_status" "$*" >&2
    status=1
  fi
  return 0
}

reset_task_output() {
  rm -f \
    "$WORKSPACE/todo.py" \
    "$WORKSPACE/.todo.json" \
    "$WORKSPACE/add.out" \
    "$WORKSPACE/list.out" \
    "$WORKSPACE/complete.out" \
    "$WORKSPACE/delete.out" \
    "$WORKSPACE/err.out"
}

validate_positive_integer() {
  case "$1" in
    ''|*[!0-9]*|0)
      printf '%s must be a positive integer, got: %s\n' "$2" "$1" >&2
      exit 2
      ;;
  esac
}

if [ "$DRY_RUN" != "1" ] && ! command -v "$SVDO_METER_BIN" >/dev/null 2>&1; then
  printf 'svdo-meter was not found. Install it globally, add it to PATH, or set SVDO_METER_BIN.\n' >&2
  exit 127
fi

validate_positive_integer "$REPETITIONS" "SVDO_STOPWORD_REPETITIONS"

mkdir -p "$WORKSPACE/.svdo/evals" "$WORKSPACE/.svdo/standards"
cp "$SCRIPT_DIR/TASK.md" "$WORKSPACE/TASK.md"
cp "$SCRIPT_DIR/.svdo/evals/todo-cli-implementation.yaml" "$WORKSPACE/.svdo/evals/"
cp "$SCRIPT_DIR/.svdo/standards/todo-cli-quality.md" "$WORKSPACE/.svdo/standards/"

cat <<EOF
SVDO Meter stopword prompt impact matrix

Work:        $WORK
Workspace:   $WORKSPACE
Harness:     $HARNESS
Model:       $MODEL
Repetitions: $REPETITIONS per variant
Variants:    $VARIANTS
Dry run:     $DRY_RUN

Telemetry will be written under:
  $WORKSPACE/.svdo/meter/

Eval artifacts will be written under:
  $WORKSPACE/.svdo/evals/
EOF

for variant in $VARIANTS; do
  prompt_file="$(prompt_for_variant "$variant")" || {
    printf 'unknown variant id: %s\n' "$variant" >&2
    exit 2
  }

  if [ ! -f "$prompt_file" ]; then
    printf 'prompt file not found for %s: %s\n' "$variant" "$prompt_file" >&2
    exit 1
  fi

  rep=1
  while [ "$rep" -le "$REPETITIONS" ]; do
    label="$(printf '%s-%03d' "$variant" "$rep")"
    note "run $label"
    reset_task_output

    run_args=(
      "$SVDO_METER_BIN" run
      --ticket "$WORK"
      --label "$label"
      --harness "$HARNESS"
      --workspace "$WORKSPACE"
      --model "$MODEL"
      --prompt-file "$prompt_file"
    )

    if [ "$DANGEROUS_BYPASS" = "1" ]; then
      run_args+=(--dangerous-bypass)
    fi

    if [ "$HARNESS" = "opencode" ]; then
      run_args+=(--opencode-agent "$OPENCODE_AGENT")
    fi

    run_or_print "${run_args[@]}"

    if [ "$RUN_EVALS" = "1" ]; then
      eval_artifact="$WORKSPACE/.svdo/evals/${label}-eval.json"
      eval_args=(
        "$SVDO_METER_BIN" eval run todo-cli-implementation
        --workspace "$WORKSPACE"
        --format json
      )

      if [ "$RUN_JUDGE" = "1" ]; then
        eval_args+=(--harness "$HARNESS" --model "$MODEL")
      fi

      if [ "$DRY_RUN" = "1" ]; then
        run_or_print "${eval_args[@]}"
        printf '# writes %s\n' "$eval_artifact"
      else
        note "eval $label"
        printf '+'
        printf ' %q' "${eval_args[@]}"
        printf ' > %q\n' "$eval_artifact"
        "${eval_args[@]}" >"$eval_artifact"
        eval_status=$?
        if [ "$eval_status" -ne 0 ]; then
          printf 'eval exited with status %s for %s\n' "$eval_status" "$label" >&2
          status=1
        fi
      fi
    fi

    rep=$((rep + 1))
  done
done

note "report"
run_or_print "$SVDO_METER_BIN" report "$WORK" --workspace "$WORKSPACE"

note "compare"
run_or_print "$SVDO_METER_BIN" compare "$WORK" --workspace "$WORKSPACE"

note "study summary"
run_or_print "$SCRIPT_DIR/report-stopword-study.sh" "$WORK" --workspace "$WORKSPACE" --baseline concise-baseline

cat <<EOF

Stopword matrix artifacts are in:
  $WORKSPACE

Telemetry:
  $WORKSPACE/.svdo/meter/

Eval artifacts:
  $WORKSPACE/.svdo/evals/
EOF

exit "$status"
