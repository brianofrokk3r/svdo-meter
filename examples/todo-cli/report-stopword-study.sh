#!/usr/bin/env bash
set -euo pipefail

python3 - "$@" <<'PY'
import argparse
import json
import math
from pathlib import Path


def parse_args():
    parser = argparse.ArgumentParser(
        description="Render the todo-cli stopword prompt impact study report."
    )
    parser.add_argument("work", help="Ticket/work id to report, such as STOPWORD-TODO")
    parser.add_argument(
        "--workspace",
        default=".",
        help="Workspace containing .svdo/meter and .svdo/evals artifacts",
    )
    parser.add_argument(
        "--baseline",
        default="concise-baseline",
        help="Variant id to use as the baseline comparison",
    )
    return parser.parse_args()


def strip_repetition_suffix(label):
    prefix, separator, suffix = label.rpartition("-")
    if separator and suffix.isdigit():
        return prefix
    return label


def telemetry_records(workspace):
    meter_dir = workspace / ".svdo" / "meter"
    for path in sorted(meter_dir.glob("*.jsonl")):
        with path.open(encoding="utf-8") as handle:
            for line_number, line in enumerate(handle, start=1):
                if not line.strip():
                    continue
                try:
                    yield json.loads(line)
                except json.JSONDecodeError as error:
                    print(f"warning: skipped {path}:{line_number}: {error}")


def token_usage(record):
    payload = record.get("payload") or {}
    data = payload.get("data") or {}
    metrics = data.get("metrics") or {}
    return metrics.get("token_usage") or data.get("token_usage") or {}


def terminal_runs(workspace, work):
    runs = {}
    for record in telemetry_records(workspace):
        if record.get("ticket_id") != work:
            continue
        event_type = record.get("event_type")
        if event_type not in {"run.completed", "run.failed"}:
            continue
        label = record.get("label") or "unlabeled"
        usage = token_usage(record)
        runs[record.get("run_id") or label] = {
            "label": label,
            "variant": strip_repetition_suffix(label),
            "status": "completed" if event_type == "run.completed" else "failed",
            "output_tokens": usage.get("output_tokens"),
        }
    return list(runs.values())


def eval_metrics(workspace, label):
    path = workspace / ".svdo" / "evals" / f"{label}-eval.json"
    if not path.exists():
        return None
    try:
        report = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        print(f"warning: skipped {path}: {error}")
        return None

    results = report.get("results") or []
    scores = []
    judge_scores = []
    required_passed = 0
    required_total = 0
    violations = 0

    for result in results:
        if isinstance(result.get("overall_score"), (int, float)):
            scores.append(float(result["overall_score"]))
        result_violations = result.get("violations") or []
        if isinstance(result_violations, list):
            violations += len(result_violations)

        required = result.get("required_checks") or {}
        if isinstance(required.get("passed"), int) and isinstance(required.get("total"), int):
            required_passed += required["passed"]
            required_total += required["total"]

        for check in result.get("checks") or []:
            if check.get("required") is True:
                required_total += 1
                if check.get("outcome") == "passed" or check.get("passed") is True:
                    required_passed += 1
            check_violations = check.get("violations") or []
            if isinstance(check_violations, list):
                violations += len(check_violations)
            check_type = check.get("type") or check.get("check_type")
            if check_type == "judge" and isinstance(check.get("score"), (int, float)):
                judge_scores.append(float(check["score"]))

    return {
        "score": mean(scores),
        "judge_score": mean(judge_scores),
        "required_passed": required_passed if required_total else None,
        "required_total": required_total if required_total else None,
        "violations": violations,
    }


def mean(values):
    return sum(values) / len(values) if values else None


def sample_variance(values):
    if len(values) < 2:
        return None
    center = mean(values)
    return sum((value - center) ** 2 for value in values) / (len(values) - 1)


def median(values):
    if not values:
        return None
    ordered = sorted(values)
    middle = len(ordered) // 2
    if len(ordered) % 2:
        return float(ordered[middle])
    return (ordered[middle - 1] + ordered[middle]) / 2


def summarize(values):
    variance = sample_variance(values)
    return {
        "n": len(values),
        "mean": mean(values),
        "variance": variance,
        "std_dev": math.sqrt(variance) if variance is not None else None,
        "min": min(values) if values else None,
        "p50": median(values),
        "max": max(values) if values else None,
    }


def confidence_heuristic(group, baseline):
    left = group["output"]
    right = baseline["output"]
    if left["n"] < 2 or right["n"] < 2:
        return None
    if left["variance"] is None or right["variance"] is None:
        return None
    standard_error = math.sqrt(
        left["variance"] / left["n"] + right["variance"] / right["n"]
    )
    difference = (left["mean"] or 0.0) - (right["mean"] or 0.0)
    if standard_error == 0:
        return abs(difference) > 0
    return abs(difference) > 1.96 * standard_error


def format_number(value, digits=1):
    return "-" if value is None else f"{value:.{digits}f}"


def format_score(value):
    if value is None:
        return "-"
    formatted = f"{value:.2f}"
    return formatted[1:] if formatted.startswith("0") else formatted


def format_int(value):
    return "-" if value is None else str(value)


def table(headers, rows):
    widths = [
        max(len(str(row[index])) for row in [headers] + rows)
        for index in range(len(headers))
    ]
    output = ["  ".join(str(cell).ljust(widths[index]) for index, cell in enumerate(headers))]
    output.append("  ".join("-" * width for width in widths))
    output.extend(
        "  ".join(str(cell).ljust(widths[index]) for index, cell in enumerate(row))
        for row in rows
    )
    return "\n".join(output)


def main():
    args = parse_args()
    workspace = Path(args.workspace)
    runs = terminal_runs(workspace, args.work)
    groups = {}
    for run in runs:
        groups.setdefault(
            run["variant"],
            {
                "runs": 0,
                "completed": 0,
                "failed": 0,
                "tokens": [],
                "eval_scores": [],
                "judge_scores": [],
                "required_passed": 0,
                "required_total": 0,
                "violations": 0,
            },
        )
        group = groups[run["variant"]]
        group["runs"] += 1
        group[run["status"]] += 1
        if isinstance(run["output_tokens"], int):
            group["tokens"].append(run["output_tokens"])

        metrics = eval_metrics(workspace, run["label"])
        if metrics:
            if metrics["score"] is not None:
                group["eval_scores"].append(metrics["score"])
            if metrics["judge_score"] is not None:
                group["judge_scores"].append(metrics["judge_score"])
            if metrics["required_total"] is not None:
                group["required_passed"] += metrics["required_passed"]
                group["required_total"] += metrics["required_total"]
            group["violations"] += metrics["violations"]

    summaries = {}
    for variant, group in sorted(groups.items()):
        total_terminal = group["completed"] + group["failed"]
        summaries[variant] = {
            "runs": group["runs"],
            "completed": group["completed"],
            "failed": group["failed"],
            "completion_rate": (
                group["completed"] / total_terminal if total_terminal else None
            ),
            "output": summarize(group["tokens"]),
            "eval_score": mean(group["eval_scores"]),
            "judge_score": mean(group["judge_scores"]),
            "required_passed": group["required_passed"],
            "required_total": group["required_total"],
            "violations": group["violations"],
        }

    print(f"SVDO Stopword Study Report - {args.work}")
    print("=" * 48)
    print("Metric: output_tokens")
    print("Grouping: run label prefix before trailing numeric repetition")
    print(
        "Significance: Welch-style 95% confidence heuristic; local environment only."
    )
    print()

    if not summaries:
        print("No matching terminal telemetry found.")
        return

    rows = []
    for variant, summary in summaries.items():
        output = summary["output"]
        rows.append(
            [
                variant,
                summary["runs"],
                output["n"],
                format_number(output["mean"]),
                format_number(output["variance"]),
                format_number(output["std_dev"]),
                f"{format_int(output['min'])} / {format_number(output['p50'])} / {format_int(output['max'])}",
                f"{summary['completed']}/{summary['completed'] + summary['failed']} ({format_number((summary['completion_rate'] or 0) * 100, 0)}%)",
                format_score(summary["eval_score"]),
                format_score(summary["judge_score"]),
                (
                    f"{summary['required_passed']}/{summary['required_total']}"
                    if summary["required_total"]
                    else "-"
                ),
                summary["violations"],
            ]
        )

    print(
        table(
            [
                "Variant",
                "Runs",
                "Token n",
                "Mean output",
                "Variance",
                "Std dev",
                "Min / p50 / max",
                "Completion",
                "Eval",
                "Judge",
                "Required",
                "Violations",
            ],
            rows,
        )
    )

    baseline = summaries.get(args.baseline)
    if not baseline:
        print()
        print(f"Baseline '{args.baseline}' was not found; skipping comparisons.")
        return

    print()
    print(f"Comparisons vs baseline: {args.baseline}")
    for variant, summary in summaries.items():
        if variant == args.baseline:
            continue
        difference = None
        if summary["output"]["mean"] is not None and baseline["output"]["mean"] is not None:
            difference = summary["output"]["mean"] - baseline["output"]["mean"]
        significant = confidence_heuristic(summary, baseline)
        if significant is None:
            verdict = "insufficient token samples"
        elif significant:
            verdict = "significant by heuristic"
        else:
            verdict = "not significant by heuristic"
        print(f"{variant}: mean diff {format_number(difference)}, {verdict}")


if __name__ == "__main__":
    main()
PY
