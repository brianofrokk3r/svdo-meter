#!/usr/bin/env bash
set -euo pipefail

python3 - "$@" <<'PY'
import argparse
from collections import Counter
import json
import math
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
    check_findings = 0
    judge_findings = 0
    required_failures = []
    below_threshold = False
    for result in results:
        score = numeric(result.get("overall_score"))
        if score is not None:
            scores.append(score)
            threshold = numeric(result.get("threshold"))
            if threshold is not None and score < threshold:
                below_threshold = True
        for check in result.get("checks") or []:
            if check.get("required") is True:
                required_total += 1
                if check.get("outcome") == "passed" or check.get("passed") is True:
                    required_passed += 1
                else:
                    required_failures.append(check.get("id") or "(unnamed required check)")
            findings = len(check.get("violations") or [])
            check_findings += findings
            if (check.get("type") or check.get("check_type")) == "judge":
                judge_findings += findings
                judge_score = numeric(check.get("score"))
                if judge_score is not None:
                    judge_scores.append(judge_score)
    return {
        "score": statistics.fmean(scores) if scores else None,
        "judge_score": statistics.fmean(judge_scores) if judge_scores else None,
        "required_passed": required_passed,
        "required_total": required_total,
        "has_required_checks": required_total > 0,
        "all_required_passed": required_total > 0 and required_passed == required_total,
        "check_findings": check_findings,
        "judge_findings": judge_findings,
        "required_failures": required_failures,
        "below_threshold": below_threshold,
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


def proportion_cell(successes, total):
    if total == 0:
        return "-"
    proportion = successes / total
    z = 1.96
    denominator = 1 + z * z / total
    center = (proportion + z * z / (2 * total)) / denominator
    margin = (
        z
        * math.sqrt(
            proportion * (1 - proportion) / total + z * z / (4 * total * total)
        )
        / denominator
    )
    return (
        f"{successes}/{total} ({proportion:.1%}; "
        f"95% CI {(center - margin):.1%}-{(center + margin):.1%})"
    )


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
                "evaluated_runs": 0,
                "required_evaluated_runs": 0,
                "all_required_runs": 0,
                "check_findings": 0,
                "judge_findings": 0,
                "below_threshold_runs": 0,
                "required_failures": Counter(),
            },
        )
        group["runs"].append(run)
        evaluation = eval_metrics(workspace, run["label"])
        if evaluation:
            group["evaluated_runs"] += 1
            if evaluation["score"] is not None:
                group["eval_scores"].append(evaluation["score"])
            if evaluation["judge_score"] is not None:
                group["judge_scores"].append(evaluation["judge_score"])
            group["required_passed"] += evaluation["required_passed"]
            group["required_total"] += evaluation["required_total"]
            if evaluation["has_required_checks"]:
                group["required_evaluated_runs"] += 1
                group["all_required_runs"] += int(evaluation["all_required_passed"])
            group["check_findings"] += evaluation["check_findings"]
            group["judge_findings"] += evaluation["judge_findings"]
            group["below_threshold_runs"] += int(evaluation["below_threshold"])
            group["required_failures"].update(evaluation["required_failures"])
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
    print("Metric values are means; n is the number of runs that reported the metric.")
    print("Quality pass and finding values are counts across evaluated runs.")
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
                proportion_cell(
                    group["all_required_runs"], group["required_evaluated_runs"]
                ),
                f"{group['below_threshold_runs']}/{group['evaluated_runs']}",
                f"{group['check_findings']} ({group['judge_findings']} judge)",
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
        ["Variant", "Runs", "Completed", "Eval", "Judge", "Required", "All required runs", "Below threshold", "Findings"],
        quality_rows,
    ))
    print("Findings count check-level violation entries once; result-level copies and score-threshold messages are excluded.")
    print("All-required-run intervals are descriptive 95% Wilson intervals, not causal estimates.")

    required_failure_rows = []
    for variant in ordered_variants(groups):
        group = groups[variant]
        for check_id, count in group["required_failures"].most_common():
            required_failure_rows.append([variant, check_id, count, group["evaluated_runs"]])
    if required_failure_rows:
        print()
        print("Required check failures")
        print(table(["Variant", "Check", "Failed runs", "Evaluated runs"], required_failure_rows))
    print()
    print("Execution behavior")
    print(table(
        ["Variant", "Wall ms", "Commands", "Failed cmd", "Tool calls", "Files", "Errors"],
        execution_rows,
    ))
    activity_fields = ("commands_executed", "tool_calls", "files_changed")
    activity_values = [
        run[field]
        for group in groups.values()
        for run in group["runs"]
        for field in activity_fields
        if run[field] is not None
    ]
    if activity_values and all(value == 0 for value in activity_values):
        print("Warning: every observed command, tool-call, and file-change counter is zero; verify harness telemetry before interpreting execution behavior.")
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
