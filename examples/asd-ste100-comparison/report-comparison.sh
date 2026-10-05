#!/usr/bin/env bash
set -euo pipefail

python3 - "$@" <<'PY'
import argparse
import json
import statistics
import sys
from pathlib import Path


TOKEN_FIELDS = (
    "input_tokens",
    "cached_input_tokens",
    "cache_write_tokens",
    "output_tokens",
    "reasoning_tokens",
)
EXECUTION_FIELDS = (
    "wall_time_ms",
    "commands_executed",
    "failed_commands",
    "tool_calls",
    "files_changed",
    "errors",
)


def parse_args():
    parser = argparse.ArgumentParser(
        description="Render the ASD-STE100-style prompt comparison report."
    )
    parser.add_argument("work", help="Work id, such as ASD-STE100-TODO")
    parser.add_argument(
        "--workspace",
        default=".",
        help="Workspace containing aggregate .svdo/meter and .svdo/evals artifacts",
    )
    parser.add_argument(
        "--baseline", default="standard", help="Variant used for reported differences"
    )
    return parser.parse_args()


def warn(message):
    print(f"warning: {message}", file=sys.stderr)


def variant_from_label(label):
    prefix, separator, suffix = label.rpartition("-")
    return prefix if separator and suffix.isdigit() else label


def load_json_lines(meter_dir):
    for path in sorted(meter_dir.glob("*.jsonl")):
        try:
            handle = path.open(encoding="utf-8")
        except OSError as error:
            warn(f"cannot read {path}: {error}")
            continue
        with handle:
            for line_number, line in enumerate(handle, start=1):
                if not line.strip():
                    continue
                try:
                    value = json.loads(line)
                except json.JSONDecodeError as error:
                    warn(f"skipped {path}:{line_number}: {error}")
                    continue
                if isinstance(value, dict):
                    yield value
                else:
                    warn(f"skipped non-object record at {path}:{line_number}")


def numeric(value):
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def terminal_runs(workspace, work):
    runs = {}
    prefix = f"{work}-"
    for record in load_json_lines(workspace / ".svdo" / "meter"):
        ticket = record.get("ticket_id") or ""
        if ticket != work and not ticket.startswith(prefix):
            continue
        event_type = record.get("event_type")
        if event_type not in {"run.completed", "run.failed"}:
            continue
        label = record.get("label") or "unlabeled"
        data = ((record.get("payload") or {}).get("data") or {})
        metrics = data.get("metrics") or {}
        usage = metrics.get("token_usage") or {}
        values = {field: numeric(metrics.get(field)) for field in EXECUTION_FIELDS}
        values.update({field: numeric(usage.get(field)) for field in TOKEN_FIELDS})
        present_tokens = [values[field] for field in TOKEN_FIELDS if values[field] is not None]
        values["observed_token_sum"] = sum(present_tokens) if present_tokens else None
        runs[record.get("run_id") or f"{label}-{len(runs)}"] = {
            "label": label,
            "variant": variant_from_label(label),
            "status": "completed" if event_type == "run.completed" else "failed",
            **values,
        }
    return list(runs.values())


def eval_metrics(workspace, label):
    path = workspace / ".svdo" / "evals" / f"{label}-eval.json"
    if not path.exists():
        return None
    try:
        report = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        warn(f"skipped {path}: {error}")
        return None

    results = report.get("results") or []
    scores = []
    judge_scores = []
    required_passed = 0
    required_total = 0
    violations = 0
    for result in results:
        score = numeric(result.get("overall_score"))
        if score is not None:
            scores.append(score)
        violations += len(result.get("violations") or [])
        for check in result.get("checks") or []:
            if check.get("required") is True:
                required_total += 1
                if check.get("outcome") == "passed" or check.get("passed") is True:
                    required_passed += 1
            violations += len(check.get("violations") or [])
            if (check.get("type") or check.get("check_type")) == "judge":
                judge_score = numeric(check.get("score"))
                if judge_score is not None:
                    judge_scores.append(judge_score)
    return {
        "score": statistics.fmean(scores) if scores else None,
        "judge_score": statistics.fmean(judge_scores) if judge_scores else None,
        "required_passed": required_passed,
        "required_total": required_total,
        "violations": violations,
    }


def mean(values):
    observed = [value for value in values if value is not None]
    return statistics.fmean(observed) if observed else None


def metric_cell(values, digits=1):
    observed = [value for value in values if value is not None]
    if not observed:
        return "-"
    return f"{statistics.fmean(observed):.{digits}f} (n={len(observed)})"


def score_cell(value):
    return "-" if value is None else f"{value:.2f}"


def table(headers, rows):
    widths = [
        max(len(str(row[index])) for row in [headers, *rows])
        for index in range(len(headers))
    ]
    lines = ["  ".join(str(cell).ljust(widths[index]) for index, cell in enumerate(headers))]
    lines.append("  ".join("-" * width for width in widths))
    lines.extend(
        "  ".join(str(cell).ljust(widths[index]) for index, cell in enumerate(row))
        for row in rows
    )
    return "\n".join(lines)


def group_runs(workspace, runs):
    groups = {}
    for run in runs:
        group = groups.setdefault(
            run["variant"],
            {
                "runs": [],
                "eval_scores": [],
                "judge_scores": [],
                "required_passed": 0,
                "required_total": 0,
                "violations": 0,
            },
        )
        group["runs"].append(run)
        evaluation = eval_metrics(workspace, run["label"])
        if evaluation:
            if evaluation["score"] is not None:
                group["eval_scores"].append(evaluation["score"])
            if evaluation["judge_score"] is not None:
                group["judge_scores"].append(evaluation["judge_score"])
            group["required_passed"] += evaluation["required_passed"]
            group["required_total"] += evaluation["required_total"]
            group["violations"] += evaluation["violations"]
    return groups


def ordered_variants(groups):
    preferred = [variant for variant in ("standard", "asd-ste100") if variant in groups]
    return preferred + sorted(set(groups) - set(preferred))


def main():
    args = parse_args()
    workspace = Path(args.workspace)
    runs = terminal_runs(workspace, args.work)
    groups = group_runs(workspace, runs)

    print(f"SVDO ASD-STE100-Style Prompt Comparison - {args.work}")
    print("=" * 58)
    print(f"Workspace: {workspace}")
    print("Grouping: label prefix before the trailing numeric repetition")
    print("Values are means; n is the number of runs that reported the metric.")
    print("Observed token sum adds only token counters present in each terminal event.")
    print()

    if not groups:
        print("No matching terminal telemetry found.")
        return

    quality_rows = []
    execution_rows = []
    token_rows = []
    for variant in ordered_variants(groups):
        group = groups[variant]
        variant_runs = group["runs"]
        completed = sum(run["status"] == "completed" for run in variant_runs)
        quality_rows.append(
            [
                variant,
                len(variant_runs),
                f"{completed}/{len(variant_runs)}",
                score_cell(mean(group["eval_scores"])),
                score_cell(mean(group["judge_scores"])),
                (
                    f"{group['required_passed']}/{group['required_total']}"
                    if group["required_total"]
                    else "-"
                ),
                group["violations"],
            ]
        )
        execution_rows.append(
            [variant]
            + [metric_cell([run[field] for run in variant_runs]) for field in EXECUTION_FIELDS]
        )
        token_rows.append(
            [variant]
            + [
                metric_cell([run[field] for run in variant_runs])
                for field in (*TOKEN_FIELDS, "observed_token_sum")
            ]
        )

    print("Quality and repository alignment")
    print(table(
        ["Variant", "Runs", "Completed", "Eval", "Judge", "Required", "Violations"],
        quality_rows,
    ))
    print()
    print("Execution behavior")
    print(table(
        ["Variant", "Wall ms", "Commands", "Failed cmd", "Tool calls", "Files", "Errors"],
        execution_rows,
    ))
    print()
    print("Token and context usage")
    print(table(
        ["Variant", "Input", "Cached", "Cache write", "Output", "Reasoning", "Observed sum"],
        token_rows,
    ))

    baseline = groups.get(args.baseline)
    if baseline:
        print()
        print(f"Observed mean differences vs {args.baseline} (variant minus baseline)")
        baseline_runs = baseline["runs"]
        for variant in ordered_variants(groups):
            if variant == args.baseline:
                continue
            variant_runs = groups[variant]["runs"]
            differences = []
            for field in ("wall_time_ms", "input_tokens", "output_tokens", "tool_calls"):
                left = mean([run[field] for run in variant_runs])
                right = mean([run[field] for run in baseline_runs])
                value = "-" if left is None or right is None else f"{left - right:+.1f}"
                differences.append(f"{field}={value}")
            print(f"{variant}: " + ", ".join(differences))
        print("These are local observations, not evidence that one writing style is generally superior.")
    else:
        print()
        print(f"Baseline '{args.baseline}' was not found; no differences were calculated.")


if __name__ == "__main__":
    main()
PY
