use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const REPORT_FIXTURE: &str = include_str!("../../../tests/fixtures/report/single_work.jsonl");
const TELEMETRY_FIXTURE: &str = include_str!("../../../tests/fixtures/telemetry/valid.jsonl");
const COMPARISON_FIXTURE: &str =
    include_str!("../../../tests/fixtures/comparison/comparable_runs.jsonl");
const COMPARE_ENG_142_FIXTURE: &str = include_str!("../../../tests/fixtures/compare/eng_142.jsonl");
const COMPARE_AGGREGATE_FIXTURE: &str =
    include_str!("../../../tests/fixtures/compare/aggregate_recent.jsonl");
const TODO_CLI_EVAL: &str =
    include_str!("../../../examples/todo-cli/.svdo/evals/todo-cli-implementation.yaml");
const TODO_CLI_STANDARD: &str =
    include_str!("../../../examples/todo-cli/.svdo/standards/todo-cli-quality.md");
const TODO_CLI_TASK: &str = include_str!("../../../examples/todo-cli/TASK.md");
const TODO_CLI_STOPWORD_MATRIX: &str =
    include_str!("../../../examples/todo-cli/stopword-matrix.yaml");
const TODO_CLI_STOPWORD_RUNNER: &str =
    include_str!("../../../examples/todo-cli/run-stopword-matrix.sh");
const TODO_CLI_STOPWORD_REPORTER: &str =
    include_str!("../../../examples/todo-cli/report-stopword-study.sh");
const TODO_CLI_VARIANTS: &str = include_str!("../../../examples/todo-cli/prompts/variants.yaml");
const TODO_CLI_CONCISE_PROMPT: &str =
    include_str!("../../../examples/todo-cli/prompts/concise-baseline.md");
const TODO_CLI_TELEGRAPHIC_PROMPT: &str =
    include_str!("../../../examples/todo-cli/prompts/telegraphic-low-stopword.md");
const TODO_CLI_STOPWORD_HEAVY_PROMPT: &str =
    include_str!("../../../examples/todo-cli/prompts/stopword-heavy-guided.md");
const TODO_CLI_POLITE_PROMPT: &str =
    include_str!("../../../examples/todo-cli/prompts/polite-redundant-stopword.md");
const TODO_CLI_REFERENCE_IMPLEMENTATION: &str = r#"#!/usr/bin/env python3
import json
import sys
from pathlib import Path

STORE = Path(".todo.json")

def load():
    if not STORE.exists():
        return []
    return json.loads(STORE.read_text())

def save(items):
    STORE.write_text(json.dumps(items))

def fail(message):
    print(message, file=sys.stderr)
    sys.exit(1)

def find(items, raw_id):
    try:
        item_id = int(raw_id)
    except ValueError:
        fail("invalid id")
    for item in items:
        if item["id"] == item_id:
            return item
    fail("todo not found")

args = sys.argv[1:]
if not args:
    fail("missing command")

command = args[0]
items = load()

if command == "add":
    if len(args) != 2:
        fail("usage: add <text>")
    next_id = max([item["id"] for item in items], default=0) + 1
    item = {"id": next_id, "text": args[1], "completed": False}
    items.append(item)
    save(items)
    print(f"Added {next_id}: {args[1]}")
elif command == "list":
    if len(args) != 1:
        fail("usage: list")
    for item in items:
        mark = "x" if item["completed"] else " "
        print(f'{item["id"]}. [{mark}] {item["text"]}')
elif command == "complete":
    if len(args) != 2:
        fail("usage: complete <id>")
    item = find(items, args[1])
    item["completed"] = True
    save(items)
    print(f'Completed {item["id"]}: {item["text"]}')
elif command == "delete":
    if len(args) != 2:
        fail("usage: delete <id>")
    item = find(items, args[1])
    items = [candidate for candidate in items if candidate["id"] != item["id"]]
    save(items)
    print(f'Deleted {item["id"]}: {item["text"]}')
else:
    fail("unknown command")
"#;

#[test]
fn help_succeeds_for_documented_command_paths() {
    assert_success_contains(&["--help"], "svdo-meter");
    assert_success_contains(&["--help"], "compare");
    assert_success_contains(&["compare", "--help"], "svdo-meter compare ENG-142");
    assert_success_contains(&["--help"], "telemetry");
    assert_success_contains(&["run", "--help"], "svdo-meter run --ticket");
    assert_success_contains(&["eval", "--help"], "Run one eval");
    assert_success_contains(&["eval", "run", "--help"], "svdo-meter eval run");
    assert_success_contains(&["report", "--help"], "svdo-meter report --last 7d");
    assert_success_contains(&["telemetry", "--help"], "sessions");
    assert_success_contains(&["telemetry", "sessions", "--help"], "List discovered");
    assert_success_contains(&["telemetry", "runs", "--help"], "List telemetry runs");
    assert_success_contains(
        &["telemetry", "inspect", "--help"],
        "Run identifier or provider session identifier",
    );
    assert_success_contains(&["run", "--help"], "--codex-profile");
    assert_success_contains(&["run", "--help"], "--dangerous-bypass");
    assert_success_contains(&["run", "--help"], "[aliases: --id, --ticket-id]");
    assert_success_contains(
        &["run", "--help"],
        "svdo-meter run --ticket ENG-142 --harness codex --dangerous-bypass PROMPT",
    );
    assert_success_contains(&["run", "--help"], "codex, claude, opencode, gemini");
    assert_success_contains(
        &["run", "--help"],
        "svdo-meter run --ticket ENG-142 --harness opencode --model github-copilot/gpt-5 --opencode-agent build PROMPT",
    );
    assert_success_contains(
        &["eval", "run", "--help"],
        "codex, claude, opencode, gemini",
    );
}

#[test]
fn compare_command_accepts_work_and_repeated_filters() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-compare-integration");
    let telemetry_dir = workspace.join(".svdo").join("meter");
    fs::create_dir_all(&telemetry_dir)?;
    fs::write(telemetry_dir.join("runs.jsonl"), COMPARISON_FIXTURE)?;

    let output = run_svdo_meter(&[
        "compare",
        "ENG-142",
        "--workspace",
        path_str(&workspace)?,
        "--harness",
        "codex",
        "--harness",
        "opencode",
        "--model",
        "gpt-5.6",
        "--model",
        "github-copilot/gpt-5",
        "--model",
        "anthropic/claude-sonnet-5",
        "--since",
        "30d",
    ]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "SVDO Comparison — ENG-142");
    assert_stdout_contains(&output, "Codex");
    assert_stdout_contains(&output, "OpenCode");
    assert_stdout_contains(&output, "Runs");
    assert_stdout_contains(&output, "Skipped line 3");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn compare_command_renders_ticket_fixture_with_unavailable_and_zero_metrics() -> std::io::Result<()>
{
    let workspace = unique_temp_path("svdo-meter-compare-ticket-fixture");
    write_workspace_telemetry_streams(&workspace, COMPARE_ENG_142_FIXTURE)?;
    write_compare_artifact(
        &workspace,
        "evals/eng-142-results.json",
        r#"
{
  "results": [
    {
      "run_id": "018f6f1b-97f1-7c04-9a96-aaaaaaaaaaa1",
      "overall_score": 0.94,
      "checks": [
        {"id": "a", "required": true, "outcome": "passed"},
        {"id": "b", "required": true, "outcome": "passed"}
      ],
      "violations": ["minor"]
    },
    {
      "run_id": "018f6f1b-97f1-7c04-9a96-bbbbbbbbbbb2",
      "overall_score": 0.97,
      "required_checks": {"passed": 2, "total": 2},
      "violations": []
    },
    {
      "run_id": "018f6f1b-97f1-7c04-9a96-ddddddddddd4",
      "overall_score": 0.88,
      "required_checks_passed": 1,
      "required_checks_total": 2,
      "violations_count": 3
    }
  ]
}
"#,
    )?;
    write_compare_artifact(
        &workspace,
        "runs/eng-142-costs.json",
        r#"
{
  "results": [
    {"run_id": "018f6f1b-97f1-7c04-9a96-aaaaaaaaaaa1", "estimated_cost_usd": 1.42, "rework_count": 0},
    {"run_id": "018f6f1b-97f1-7c04-9a96-bbbbbbbbbbb2", "estimated_cost_usd": 1.19, "rework_count": 0},
    {"run_id": "018f6f1b-97f1-7c04-9a96-ddddddddddd4", "estimated_cost_usd": 1.67, "rework_count": 1}
  ]
}
"#,
    )?;

    let output = run_svdo_meter(&["compare", "ENG-142", "--workspace", path_str(&workspace)?]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "SVDO Comparison — ENG-142");
    assert_stdout_contains(&output, "Codex");
    assert_stdout_contains(&output, "Claude");
    assert_stdout_contains(&output, "OpenCode");
    assert_stdout_row_contains(&output, "Runs", &["1", "1", "1", "1"]);
    assert_stdout_row_contains(&output, "Commands", &["14", "0", "—"]);
    assert_stdout_row_contains(&output, "Tool calls", &["27", "31", "42"]);
    assert_stdout_row_contains(&output, "Input tokens", &["145k", "182k", "210k"]);
    assert_stdout_row_contains(&output, "Est. cost", &["$1.19", "$1.42", "$1.67"]);
    assert_stdout_row_contains(&output, "Eval score", &[".88", ".94", ".97"]);
    assert_stdout_row_contains(&output, "Required checks", &["1/2", "2/2"]);
    assert_stdout_row_contains(&output, "Violations", &["0", "1", "3"]);

    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn compare_command_renders_aggregate_fixture_with_repeated_harnesses_and_since()
-> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-compare-aggregate-fixture");
    write_workspace_telemetry_streams(&workspace, COMPARE_AGGREGATE_FIXTURE)?;

    let output = run_svdo_meter(&[
        "compare",
        "--workspace",
        path_str(&workspace)?,
        "--harness",
        "codex",
        "--harness",
        "claude",
        "--harness",
        "opencode",
        "--since",
        "30d",
    ]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "SVDO Comparison");
    assert_stdout_row_contains(&output, "Tasks", &["2", "2", "2"]);
    assert_stdout_row_contains(&output, "Pass rate", &["100%", "50%"]);
    assert_stdout_row_contains(&output, "Median tokens", &["153k", "193.5k", "209k"]);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("ENG-OLD"));

    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn compare_command_supports_repeated_model_filters_within_harness() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-compare-model-fixture");
    write_workspace_telemetry_streams(&workspace, COMPARE_ENG_142_FIXTURE)?;

    let output = run_svdo_meter(&[
        "compare",
        "ENG-142",
        "--workspace",
        path_str(&workspace)?,
        "--harness",
        "opencode",
        "--model",
        "openai/gpt-5.6",
        "--model",
        "anthropic/claude-sonnet-5",
    ]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "gpt-5.6");
    assert_stdout_contains(&output, "claude-sonnet-5");
    assert_stdout_row_contains(&output, "Model", &["claude-sonnet-5", "gpt-5.6"]);
    assert_stdout_row_contains(&output, "Runs", &["1", "1"]);
    assert!(
        !String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.contains("Commands"))
    );

    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn compare_command_supports_model_filter_across_repeated_harnesses() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-compare-model-harness-fixture");
    write_workspace_telemetry_streams(&workspace, COMPARE_ENG_142_FIXTURE)?;

    let output = run_svdo_meter(&[
        "compare",
        "ENG-142",
        "--workspace",
        path_str(&workspace)?,
        "--model",
        "gpt-5.6",
        "--harness",
        "codex",
        "--harness",
        "opencode",
    ]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "Codex / gpt-5.6");
    assert_stdout_contains(&output, "OpenCode / gpt-5.6");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Claude /"));
    assert_stdout_row_contains(&output, "Commands", &["0", "—"]);

    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn compare_command_enriches_aggregate_metrics_from_eval_and_run_artifacts() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-compare-artifact-enrichment");
    write_workspace_telemetry_streams(&workspace, COMPARE_AGGREGATE_FIXTURE)?;
    write_compare_artifact(
        &workspace,
        "evals/recent-results.json",
        r#"
{
  "results": [
    {"run_id": "018f6f1b-97f1-7c04-9a96-eeeeeeeeeee1", "overall_score": 0.91},
    {"run_id": "018f6f1b-97f1-7c04-9a96-eeeeeeeeeee2", "overall_score": 0.88},
    {"run_id": "018f6f1b-97f1-7c04-9a96-fffffffffff3", "overall_score": 0.94},
    {"run_id": "018f6f1b-97f1-7c04-9a96-fffffffffff4", "overall_score": 0.96},
    {"run_id": "018f6f1b-97f1-7c04-9a96-999999999995", "overall_score": 0.86},
    {"run_id": "018f6f1b-97f1-7c04-9a96-999999999996", "overall_score": 0.90}
  ]
}
"#,
    )?;
    write_compare_artifact(
        &workspace,
        "runs/rework.json",
        r#"
{
  "results": [
    {"run_id": "018f6f1b-97f1-7c04-9a96-eeeeeeeeeee1", "rework_count": 1},
    {"run_id": "018f6f1b-97f1-7c04-9a96-eeeeeeeeeee2", "rework_count": 2},
    {"run_id": "018f6f1b-97f1-7c04-9a96-fffffffffff3", "rework_count": 0},
    {"run_id": "018f6f1b-97f1-7c04-9a96-fffffffffff4", "rework_count": 0},
    {"run_id": "018f6f1b-97f1-7c04-9a96-999999999995", "rework_count": 1},
    {"run_id": "018f6f1b-97f1-7c04-9a96-999999999996", "rework_count": 3}
  ]
}
"#,
    )?;

    let output = run_svdo_meter(&[
        "compare",
        "--workspace",
        path_str(&workspace)?,
        "--harness",
        "codex",
        "--harness",
        "claude",
        "--harness",
        "opencode",
        "--since",
        "30d",
    ]);

    assert!(output.status.success());
    assert_stdout_row_contains(&output, "Median eval", &[".90", ".95", ".88"]);
    assert_stdout_row_contains(&output, "Median rework", &["1.5", "0", "2"]);

    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_run_specific_definition_renders_terminal_output() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-specific-integration");
    write_eval_definition(
        &workspace,
        "smoke.yaml",
        r#"
id: smoke
task: Check local fixture.
checks:
  - id: has-marker
    type: command
    command: test -f marker.txt
    required: true
    weight: 1.0
threshold: 1.0
"#,
    )?;
    fs::write(workspace.join("marker.txt"), "ok")?;

    let output = run_svdo_meter(&["eval", "run", "smoke", "--workspace", path_str(&workspace)?]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "SVDO Eval");
    assert_stdout_contains(&output, "smoke");
    assert_stdout_contains(&output, "PASS");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_run_all_definitions_renders_json_output() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-all-integration");
    write_eval_definition(
        &workspace,
        "one.yaml",
        r#"
id: one
task: First eval.
checks:
  - id: pass
    type: command
    command: "true"
    required: true
threshold: 1.0
"#,
    )?;
    write_eval_definition(
        &workspace,
        "two.yaml",
        r#"
id: two
task: Second eval.
checks:
  - id: pass
    type: command
    command: "true"
    required: true
threshold: 1.0
"#,
    )?;

    let output = run_svdo_meter(&[
        "eval",
        "run",
        "--workspace",
        path_str(&workspace)?,
        "--format",
        "json",
    ]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "\"passed\": true");
    assert_stdout_contains(&output, "\"id\": \"one\"");
    assert_stdout_contains(&output, "\"id\": \"two\"");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_required_failure_exits_nonzero_and_reports_reason() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-fail-integration");
    write_eval_definition(
        &workspace,
        "required-failure.yaml",
        r#"
id: required-failure
task: Demonstrate hard failure.
checks:
  - id: required
    type: command
    command: "printf required-failed >&2; exit 7"
    required: true
    weight: 0.1
  - id: optional
    type: command
    command: "true"
    required: false
    weight: 0.9
threshold: 0.5
"#,
    )?;

    let output = run_svdo_meter(&[
        "eval",
        "run",
        "required-failure.yaml",
        "--workspace",
        path_str(&workspace)?,
    ]);

    assert!(!output.status.success());
    assert_stdout_contains(&output, "FAIL");
    assert_stdout_contains(&output, "Failed checks: required");
    assert_stdout_contains(&output, "required-failed");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_csv_output_is_pipe_friendly() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-csv-integration");
    write_eval_definition(
        &workspace,
        "csv.yaml",
        r#"
id: csv
task: Render CSV.
checks:
  - id: pass
    type: command
    command: "true"
    required: true
threshold: 1.0
"#,
    )?;

    let output = run_svdo_meter(&[
        "eval",
        "run",
        "csv",
        "--workspace",
        path_str(&workspace)?,
        "--format",
        "csv",
    ]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "eval_id,eval_passed,overall_score");
    assert_stdout_contains(&output, "csv,true,1.0000");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_judge_command_scores_judge_checks() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-judge-integration");
    write_eval_definition(
        &workspace,
        "judge.yaml",
        r#"
id: judge
task: Review architecture.
checks:
  - id: architecture
    type: judge
    standard: architecture
    required: true
    weight: 1.0
threshold: 1.0
"#,
    )?;
    write_standard(
        &workspace,
        "architecture.md",
        "Prefer direct process execution.",
    )?;
    let judge = write_executable(
        &workspace,
        "judge.sh",
        r#"#!/bin/sh
test -f "$1" || exit 9
test "$SVDO_METER_JUDGE_REQUEST" = "$1" || exit 10
grep -q '"standard": "architecture"' "$1" || exit 11
printf '{"score": 1.0, "passed": true, "violations": [], "model": "fixture-model", "harness": "fixture-judge"}'
"#,
    )?;

    let output = run_svdo_meter(&[
        "eval",
        "run",
        "judge",
        "--workspace",
        path_str(&workspace)?,
        "--judge-command",
        path_str(&judge)?,
        "--format",
        "json",
    ]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "\"outcome\": \"passed\"");
    assert_stdout_contains(&output, "\"score\": 1.0");
    assert_stdout_contains(&output, "\"model\": \"fixture-model\"");
    assert_stdout_contains(&output, "\"harness\": \"fixture-judge\"");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_codex_judge_harness_scores_judge_checks() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-codex-judge-integration");
    write_eval_definition(
        &workspace,
        "codex-judge.yaml",
        r#"
id: codex-judge
task: Review architecture.
checks:
  - id: architecture
    type: judge
    standard: architecture
    required: true
    weight: 1.0
threshold: 1.0
"#,
    )?;
    write_standard(
        &workspace,
        "architecture.md",
        "Prefer direct process execution.",
    )?;
    let bin_dir = workspace.join("bin");
    let codex = write_executable(
        &bin_dir,
        "codex",
        r#"#!/bin/sh
test "$1" = "exec" || exit 9
test "$2" = "--json" || exit 10
case "$*" in
  *"--model gpt-5"*) ;;
  *) exit 11 ;;
esac
case "$*" in
  *"--skip-git-repo-check"*) ;;
  *) exit 12 ;;
esac
printf '{"type":"agent_message","message":"{\"score\":1.0,\"passed\":true,\"violations\":[],\"model\":\"gpt-5\",\"harness\":\"codex\"}"}\n'
"#,
    )?;

    let output = run_svdo_meter_with_path(
        &[
            "eval",
            "run",
            "codex-judge",
            "--workspace",
            path_str(&workspace)?,
            "--harness",
            "codex",
            "--model",
            "gpt-5",
            "--format",
            "json",
        ],
        codex
            .parent()
            .ok_or_else(|| std::io::Error::other("missing bin dir"))?,
    );

    assert!(output.status.success());
    assert_stdout_contains(&output, "\"outcome\": \"passed\"");
    assert_stdout_contains(&output, "\"model\": \"gpt-5\"");
    assert_stdout_contains(&output, "\"harness\": \"codex\"");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_claude_judge_harness_scores_judge_checks() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-claude-judge-integration");
    write_eval_definition(
        &workspace,
        "claude-judge.yaml",
        r#"
id: claude-judge
task: Review architecture.
checks:
  - id: architecture
    type: judge
    standard: architecture
    required: true
    weight: 1.0
threshold: 1.0
"#,
    )?;
    write_standard(
        &workspace,
        "architecture.md",
        "Prefer direct process execution.",
    )?;
    let bin_dir = workspace.join("bin");
    let claude = write_executable(
        &bin_dir,
        "claude",
        r#"#!/bin/sh
test "$1" = "-p" || exit 9
case "$*" in
  *"--output-format stream-json"*) ;;
  *) exit 10 ;;
esac
case "$*" in
  *"--model sonnet"*) ;;
  *) exit 11 ;;
esac
printf '{"type":"assistant","message":{"model":"sonnet","content":[{"type":"text","text":"{\"score\":1.0,\"passed\":true,\"violations\":[],\"model\":\"sonnet\",\"harness\":\"claude\"}"}]}}\n'
"#,
    )?;

    let output = run_svdo_meter_with_path(
        &[
            "eval",
            "run",
            "claude-judge",
            "--workspace",
            path_str(&workspace)?,
            "--harness",
            "claude",
            "--model",
            "sonnet",
            "--format",
            "json",
        ],
        claude
            .parent()
            .ok_or_else(|| std::io::Error::other("missing bin dir"))?,
    );

    assert!(output.status.success());
    assert_stdout_contains(&output, "\"outcome\": \"passed\"");
    assert_stdout_contains(&output, "\"model\": \"sonnet\"");
    assert_stdout_contains(&output, "\"harness\": \"claude\"");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_opencode_judge_harness_scores_judge_checks() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-opencode-judge-integration");
    write_eval_definition(
        &workspace,
        "opencode-judge.yaml",
        r#"
id: opencode-judge
task: Review architecture.
checks:
  - id: architecture
    type: judge
    standard: architecture
    required: true
    weight: 1.0
threshold: 1.0
"#,
    )?;
    write_standard(
        &workspace,
        "architecture.md",
        "Prefer direct process execution.",
    )?;
    let bin_dir = workspace.join("bin");
    let opencode = install_opencode_fixture(&bin_dir)?;

    let output = run_svdo_meter_with_path(
        &[
            "eval",
            "run",
            "opencode-judge",
            "--workspace",
            path_str(&workspace)?,
            "--harness",
            "opencode",
            "--model",
            "github-copilot/gpt-5",
            "--format",
            "json",
        ],
        opencode
            .parent()
            .ok_or_else(|| std::io::Error::other("missing bin dir"))?,
    );

    assert!(output.status.success());
    assert_stdout_contains(&output, "\"outcome\": \"passed\"");
    assert_stdout_contains(&output, "\"model\": \"github-copilot/gpt-5\"");
    assert_stdout_contains(&output, "\"harness\": \"opencode\"");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_fails_when_required_judge_rejects_work() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-judge-fail-integration");
    write_eval_definition(
        &workspace,
        "judge-fail.yaml",
        r#"
id: judge-fail
task: Review implementation.
checks:
  - id: architecture
    type: judge
    required: true
    weight: 1.0
threshold: 0.8
"#,
    )?;
    let judge = write_executable(
        &workspace,
        "judge.sh",
        r#"#!/bin/sh
printf '{"score": 0.25, "passed": false, "violations": ["missing architecture evidence"]}'
"#,
    )?;

    let output = run_svdo_meter(&[
        "eval",
        "run",
        "judge-fail",
        "--workspace",
        path_str(&workspace)?,
        "--judge-command",
        path_str(&judge)?,
    ]);

    assert!(!output.status.success());
    assert_stdout_contains(&output, "FAIL");
    assert_stdout_contains(&output, "Failed checks: architecture");
    assert_stdout_contains(&output, "missing architecture evidence");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_reports_invalid_judge_response_with_context() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-judge-invalid-integration");
    write_eval_definition(
        &workspace,
        "judge-invalid.yaml",
        r#"
id: judge-invalid
task: Review CLI design.
checks:
  - id: cli-design
    type: judge
    standard: cli-design
    required: true
    weight: 1.0
threshold: 1.0
"#,
    )?;
    write_standard(
        &workspace,
        "cli-design.md",
        "Keep command-line output precise and actionable.",
    )?;
    let judge = write_executable(
        &workspace,
        "judge.sh",
        r#"#!/bin/sh
printf 'I checked the CLI and it looks okay, but forgot the score field.'
"#,
    )?;

    let output = run_svdo_meter(&[
        "eval",
        "run",
        "judge-invalid",
        "--workspace",
        path_str(&workspace)?,
        "--judge-command",
        path_str(&judge)?,
        "--format",
        "json",
    ]);

    assert!(!output.status.success());
    assert_stdout_contains(&output, "\"id\": \"judge-invalid\"");
    assert_stdout_contains(&output, "\"id\": \"cli-design\"");
    assert_stdout_contains(&output, "\"outcome\": \"failed\"");
    assert_stdout_contains(
        &output,
        "invalid judge response for eval `judge-invalid` check `cli-design`",
    );
    assert_stdout_contains(&output, "forgot the score field");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn repo_sample_evals_are_runnable() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let output = run_svdo_meter(&[
        "eval",
        "run",
        "cli-entrypoint",
        "--workspace",
        path_str(&repo_root).expect("repo root path must be UTF-8"),
        "--format",
        "json",
    ]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "\"id\": \"cli-entrypoint\"");
    assert_stdout_contains(&output, "\"id\": \"main-entrypoint\"");
    assert_stdout_contains(&output, "\"harness\": \"judge-unavailable\"");
}

#[test]
fn todo_cli_fixture_eval_passes_against_reference_implementation() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-todo-cli-fixture");
    fs::create_dir_all(&workspace)?;
    fs::write(workspace.join("TASK.md"), TODO_CLI_TASK)?;
    fs::write(workspace.join("todo.py"), TODO_CLI_REFERENCE_IMPLEMENTATION)?;
    write_eval_definition(&workspace, "todo-cli-implementation.yaml", TODO_CLI_EVAL)?;
    write_standard(&workspace, "todo-cli-quality.md", TODO_CLI_STANDARD)?;

    let output = run_svdo_meter(&[
        "eval",
        "run",
        "todo-cli-implementation",
        "--workspace",
        path_str(&workspace)?,
        "--format",
        "json",
    ]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "\"id\": \"todo-cli-implementation\"");
    assert_stdout_contains(&output, "\"id\": \"delete-removes-item-with-stable-ids\"");
    assert_stdout_contains(&output, "\"harness\": \"judge-unavailable\"");
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn todo_cli_stopword_matrix_documents_runnable_variants() {
    assert!(TODO_CLI_STOPWORD_MATRIX.contains("default_harness: codex"));
    assert!(TODO_CLI_STOPWORD_MATRIX.contains("default_model: gpt-5.5"));
    assert!(TODO_CLI_STOPWORD_MATRIX.contains("default_repetitions_per_variant: 30"));
    assert!(TODO_CLI_STOPWORD_MATRIX.contains("primary: output_tokens"));
    assert!(TODO_CLI_STOPWORD_MATRIX.contains("svdo-meter report --workspace <workspace>"));
    assert!(TODO_CLI_STOPWORD_MATRIX.contains("svdo-meter compare --workspace <workspace>"));
    assert!(TODO_CLI_STOPWORD_MATRIX.contains("./report-stopword-study.sh STOPWORD-TODO"));
    assert!(TODO_CLI_STOPWORD_MATRIX.contains("Welch-style 95%"));

    for variant in [
        (
            "concise-baseline",
            "prompts/concise-baseline.md",
            TODO_CLI_CONCISE_PROMPT,
        ),
        (
            "telegraphic-low-stopword",
            "prompts/telegraphic-low-stopword.md",
            TODO_CLI_TELEGRAPHIC_PROMPT,
        ),
        (
            "stopword-heavy-guided",
            "prompts/stopword-heavy-guided.md",
            TODO_CLI_STOPWORD_HEAVY_PROMPT,
        ),
        (
            "polite-redundant-stopword",
            "prompts/polite-redundant-stopword.md",
            TODO_CLI_POLITE_PROMPT,
        ),
    ] {
        let (id, prompt_file, prompt_body) = variant;
        assert!(
            TODO_CLI_VARIANTS.contains(id),
            "missing variant metadata for {id}"
        );
        assert!(
            TODO_CLI_STOPWORD_MATRIX.contains(id),
            "missing matrix entry for {id}"
        );
        assert!(
            TODO_CLI_STOPWORD_MATRIX.contains(prompt_file),
            "missing matrix prompt file for {id}"
        );
        assert!(prompt_body.contains("add"));
        assert!(prompt_body.contains("list"));
        assert!(prompt_body.contains("complete"));
        assert!(prompt_body.contains("delete"));
    }
}

#[test]
fn todo_cli_stopword_runner_uses_svdo_meter_workflow_and_overrides() {
    assert!(TODO_CLI_STOPWORD_RUNNER.starts_with("#!/usr/bin/env bash"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("SVDO_STOPWORD_HARNESS:-codex"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("SVDO_STOPWORD_MODEL:-gpt-5.5"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("SVDO_STOPWORD_REPETITIONS:-30"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("SVDO_STOPWORD_JUDGE:-1"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("SVDO_STOPWORD_VARIANTS"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("SVDO_STOPWORD_DRY_RUN"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("cp \"$SCRIPT_DIR/TASK.md\""));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("svdo-meter"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("run"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("--prompt-file"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("eval run todo-cli-implementation"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("report"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("compare"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains("report-stopword-study.sh"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains(".svdo/meter/"));
    assert!(TODO_CLI_STOPWORD_RUNNER.contains(".svdo/evals/"));
    assert!(TODO_CLI_STOPWORD_REPORTER.contains("SVDO Stopword Study Report"));
    assert!(TODO_CLI_STOPWORD_REPORTER.contains("Welch-style 95%"));
    assert!(TODO_CLI_STOPWORD_REPORTER.contains("-eval.json"));
}

#[test]
fn invalid_run_arguments_fail_before_harness_execution() {
    let output = run_svdo_meter(&["run", "--ticket", "ENG-142", "--harness", "codex"]);

    assert!(!output.status.success());
    assert_output_contains(&output, "required");
    assert_output_contains(&output, "PROMPT");
}

#[test]
fn run_id_alias_is_accepted_before_harness_execution() {
    let output = run_svdo_meter(&["run", "--id", "ENG-142", "--harness", "codex"]);

    assert!(!output.status.success());
    assert_output_contains(&output, "required");
    assert_output_contains(&output, "PROMPT");
}

#[test]
fn codex_flags_fail_with_non_codex_harness_before_harness_execution() {
    let output = run_svdo_meter(&[
        "run",
        "--ticket",
        "ENG-142",
        "--harness",
        "claude",
        "--codex-skip-git-repo-check",
        "Implement ENG-142",
    ]);

    assert!(!output.status.success());
    assert_output_contains(
        &output,
        "Codex-specific --codex-* options require --harness codex",
    );
}

#[test]
fn opencode_run_invokes_non_interactive_cli_with_expected_arguments() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-opencode-run-integration");
    let bin_dir = workspace.join("bin");
    let capture = workspace.join("opencode-argv.txt");
    fs::create_dir_all(&workspace)?;
    install_opencode_fixture(&bin_dir)?;

    let output = run_svdo_meter_with_path_and_env(
        &[
            "run",
            "--ticket",
            "ENG-OPENCODE",
            "--harness",
            "opencode",
            "--workspace",
            path_str(&workspace)?,
            "--model",
            "github-copilot/gpt-5",
            "--opencode-agent",
            "build",
            "--dangerous-bypass",
            "Implement ENG-OPENCODE",
        ],
        &bin_dir,
        &[("OPENCODE_CAPTURE", path_str(&capture)?)],
    );

    assert!(output.status.success());
    let args = fs::read_to_string(&capture)?;
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        vec![
            "run",
            "--format",
            "json",
            "--dir",
            path_str(&workspace)?,
            "--model",
            "github-copilot/gpt-5",
            "--auto",
            "--agent",
            "build",
            "Implement ENG-OPENCODE",
        ]
    );
    let mut telemetry = String::new();
    for entry in fs::read_dir(workspace.join(".svdo").join("meter"))? {
        telemetry.push_str(&fs::read_to_string(entry?.path())?);
    }
    assert!(telemetry.contains("\"harness\":\"opencode\""));
    assert!(telemetry.contains("\"session_id\":\"ses_opencode_discovered\""));
    assert!(telemetry.contains("\"source\":\"opencode\""));
    assert!(telemetry.contains("\"provider_event_count\":1"));
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn opencode_run_resumes_explicit_session_and_discovers_json_session() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-opencode-session-integration");
    let bin_dir = workspace.join("bin");
    let capture = workspace.join("opencode-argv.txt");
    fs::create_dir_all(&workspace)?;
    install_opencode_fixture(&bin_dir)?;

    let output = run_svdo_meter_with_path_and_env(
        &[
            "run",
            "--ticket",
            "ENG-OPENCODE-SESSION",
            "--harness",
            "opencode",
            "--workspace",
            path_str(&workspace)?,
            "--session",
            "ses_fixture",
            "--model",
            "github-copilot/gpt-5",
            "Continue ENG-OPENCODE-SESSION",
        ],
        &bin_dir,
        &[("OPENCODE_CAPTURE", path_str(&capture)?)],
    );

    assert!(output.status.success());
    let args = fs::read_to_string(&capture)?;
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        vec![
            "run",
            "--format",
            "json",
            "--dir",
            path_str(&workspace)?,
            "--model",
            "github-copilot/gpt-5",
            "--session",
            "ses_fixture",
            "Continue ENG-OPENCODE-SESSION",
        ]
    );
    let mut telemetry = String::new();
    for entry in fs::read_dir(workspace.join(".svdo").join("meter"))? {
        telemetry.push_str(&fs::read_to_string(entry?.path())?);
    }
    assert!(telemetry.contains("\"harness\":\"opencode\""));
    assert!(telemetry.contains("\"session_id\":\"ses_fixture\""));
    assert!(telemetry.contains("\"source\":\"user_override\""));
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn opencode_auto_resume_retries_fresh_when_stored_session_is_missing() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-opencode-stale-session-integration");
    let bin_dir = workspace.join("bin");
    let capture = workspace.join("opencode-argv.txt");
    fs::create_dir_all(&workspace)?;
    write_workspace_telemetry_streams(
        &workspace,
        &format!(
            r#"{{"schema_version":1,"event_id":"018f6f1b-97f1-7c04-9a96-eeeeeeeeeeee","event_type":"session.discovered","occurred_at":"2026-08-21T12:00:02Z","observed_at":"2026-08-21T12:00:02Z","run_id":"018f6f1b-97f1-7c04-9a96-333333333333","ticket_id":"ENG-OPENCODE-STALE","harness":"opencode","session_id":"ses_stale","workspace":{},"payload":{{"type":"session_discovered","data":{{"source":"opencode"}}}}}}"#,
            serde_json::to_string(path_str(&workspace)?)
                .unwrap_or_else(|error| panic!("failed to serialize workspace path: {error}"))
        ),
    )?;
    install_opencode_fixture(&bin_dir)?;

    let output = run_svdo_meter_with_path_and_env(
        &[
            "run",
            "--ticket",
            "ENG-OPENCODE-STALE",
            "--harness",
            "opencode",
            "--workspace",
            path_str(&workspace)?,
            "Continue ENG-OPENCODE-STALE",
        ],
        &bin_dir,
        &[
            ("OPENCODE_CAPTURE", path_str(&capture)?),
            ("OPENCODE_CAPTURE_MODE", "attempts"),
        ],
    );

    assert!(output.status.success());
    assert_output_contains(
        &output,
        "OpenCode session not found; starting a fresh session.",
    );
    let args = fs::read_to_string(&capture)?;
    let attempts = args
        .split("---\n")
        .filter(|chunk| !chunk.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(attempts.len(), 2);
    assert!(attempts[0].contains("--session\nses_stale\n"));
    assert!(!attempts[1].contains("--session\n"));
    let mut telemetry = String::new();
    for entry in fs::read_dir(workspace.join(".svdo").join("meter"))? {
        telemetry.push_str(&fs::read_to_string(entry?.path())?);
    }
    assert!(telemetry.contains("\"session_id\":\"ses_fresh\""));
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn report_command_renders_fixture_backed_outputs() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-report-integration");
    write_workspace_telemetry_streams(&workspace, REPORT_FIXTURE)?;

    let terminal = run_svdo_meter(&["report", "ENG-142", "--workspace", path_str(&workspace)?]);
    assert!(terminal.status.success());
    assert_stdout_contains(&terminal, "SVDO Trace");
    assert_stdout_contains(&terminal, "ENG-142");

    let json = run_svdo_meter(&[
        "report",
        "ENG-142",
        "--workspace",
        path_str(&workspace)?,
        "--format",
        "json",
    ]);
    assert!(json.status.success());
    assert_stdout_contains(&json, "\"work\": \"ENG-142\"");

    let csv = run_svdo_meter(&[
        "report",
        "ENG-142",
        "--workspace",
        path_str(&workspace)?,
        "--format",
        "csv",
    ]);
    assert!(csv.status.success());
    assert_stdout_contains(&csv, "work,harnesses,sessions,runs");

    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn report_command_reads_per_run_stream_directory() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-report-streams-integration");
    write_workspace_telemetry_streams(&workspace, REPORT_FIXTURE)?;

    let terminal = run_svdo_meter(&["report", "ENG-142", "--workspace", path_str(&workspace)?]);
    assert!(terminal.status.success());
    assert_stdout_contains(&terminal, "SVDO Trace");
    assert_stdout_contains(&terminal, "Runs\n  2");

    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn todo_cli_stopword_reporter_groups_variants_and_eval_artifacts() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-stopword-reporter-integration");
    write_workspace_telemetry_streams(
        &workspace,
        r#"
{"schema_version":1,"event_id":"018f6f1b-97f1-7c04-9a96-400000000001","event_type":"run.completed","occurred_at":"2026-09-01T12:00:00Z","observed_at":"2026-09-01T12:00:00Z","run_id":"018f6f1b-97f1-7c04-9a96-400000000101","ticket_id":"STOPWORD-TODO","label":"concise-baseline-001","harness":"codex","requested_model":"gpt-5.5","resolved_model":"openai/gpt-5.5","session_id":"sess-study-1","payload":{"type":"run_completed","data":{"metrics":{"wall_time_ms":1000,"active_time_ms":1000,"command_time_ms":0,"tool_time_ms":0,"turn_count":1,"provider_event_count":1,"commands_executed":0,"failed_commands":0,"files_changed":1,"tool_calls":1,"errors":0,"token_usage":{"input_tokens":100,"cached_input_tokens":0,"cache_write_tokens":0,"output_tokens":100,"reasoning_tokens":0}},"exit_code":0}}}
{"schema_version":1,"event_id":"018f6f1b-97f1-7c04-9a96-400000000002","event_type":"run.completed","occurred_at":"2026-09-01T12:01:00Z","observed_at":"2026-09-01T12:01:00Z","run_id":"018f6f1b-97f1-7c04-9a96-400000000102","ticket_id":"STOPWORD-TODO","label":"concise-baseline-002","harness":"codex","requested_model":"gpt-5.5","resolved_model":"openai/gpt-5.5","session_id":"sess-study-2","payload":{"type":"run_completed","data":{"metrics":{"wall_time_ms":1000,"active_time_ms":1000,"command_time_ms":0,"tool_time_ms":0,"turn_count":1,"provider_event_count":1,"commands_executed":0,"failed_commands":0,"files_changed":1,"tool_calls":1,"errors":0,"token_usage":{"input_tokens":100,"cached_input_tokens":0,"cache_write_tokens":0,"output_tokens":102,"reasoning_tokens":0}},"exit_code":0}}}
{"schema_version":1,"event_id":"018f6f1b-97f1-7c04-9a96-400000000003","event_type":"run.completed","occurred_at":"2026-09-01T12:02:00Z","observed_at":"2026-09-01T12:02:00Z","run_id":"018f6f1b-97f1-7c04-9a96-400000000103","ticket_id":"STOPWORD-TODO","label":"stopword-heavy-guided-001","harness":"codex","requested_model":"gpt-5.5","resolved_model":"openai/gpt-5.5","session_id":"sess-study-3","payload":{"type":"run_completed","data":{"metrics":{"wall_time_ms":1000,"active_time_ms":1000,"command_time_ms":0,"tool_time_ms":0,"turn_count":1,"provider_event_count":1,"commands_executed":0,"failed_commands":0,"files_changed":1,"tool_calls":1,"errors":0,"token_usage":{"input_tokens":100,"cached_input_tokens":0,"cache_write_tokens":0,"output_tokens":150,"reasoning_tokens":0}},"exit_code":0}}}
{"schema_version":1,"event_id":"018f6f1b-97f1-7c04-9a96-400000000004","event_type":"run.completed","occurred_at":"2026-09-01T12:03:00Z","observed_at":"2026-09-01T12:03:00Z","run_id":"018f6f1b-97f1-7c04-9a96-400000000104","ticket_id":"STOPWORD-TODO","label":"stopword-heavy-guided-002","harness":"codex","requested_model":"gpt-5.5","resolved_model":"openai/gpt-5.5","session_id":"sess-study-4","payload":{"type":"run_completed","data":{"metrics":{"wall_time_ms":1000,"active_time_ms":1000,"command_time_ms":0,"tool_time_ms":0,"turn_count":1,"provider_event_count":1,"commands_executed":0,"failed_commands":0,"files_changed":1,"tool_calls":1,"errors":0,"token_usage":{"input_tokens":100,"cached_input_tokens":0,"cache_write_tokens":0,"output_tokens":152,"reasoning_tokens":0}},"exit_code":0}}}
"#,
    )?;
    write_compare_artifact(
        &workspace,
        "evals/concise-baseline-001-eval.json",
        r#"
{
  "results": [
    {"overall_score": 1.0, "checks": [{"type": "command", "outcome": "passed", "required": true, "violations": []}]}
  ]
}
"#,
    )?;
    write_compare_artifact(
        &workspace,
        "evals/concise-baseline-002-eval.json",
        r#"
{
  "results": [
    {"overall_score": 1.0, "checks": [{"type": "command", "outcome": "passed", "required": true, "violations": []}]}
  ]
}
"#,
    )?;
    write_compare_artifact(
        &workspace,
        "evals/stopword-heavy-guided-001-eval.json",
        r#"
{
  "results": [
    {"overall_score": 0.9, "checks": [{"type": "command", "outcome": "passed", "required": true, "violations": []}, {"type": "judge", "outcome": "passed", "required": false, "score": 0.8, "violations": ["style"]}]}
  ]
}
"#,
    )?;
    write_compare_artifact(
        &workspace,
        "evals/stopword-heavy-guided-002-eval.json",
        r#"
{
  "results": [
    {"overall_score": 0.8, "checks": [{"type": "command", "outcome": "failed", "required": true, "violations": []}, {"type": "judge", "outcome": "passed", "required": false, "score": 0.9, "violations": []}]}
  ]
}
"#,
    )?;

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let reporter = repo_root
        .join("examples")
        .join("todo-cli")
        .join("report-stopword-study.sh");
    let output = Command::new(reporter)
        .args([
            "STOPWORD-TODO",
            "--workspace",
            path_str(&workspace)?,
            "--baseline",
            "concise-baseline",
        ])
        .output()?;

    assert!(output.status.success());
    assert_stdout_contains(&output, "SVDO Stopword Study Report - STOPWORD-TODO");
    assert_stdout_contains(&output, "Metric: output_tokens");
    assert_stdout_contains(&output, "concise-baseline");
    assert_stdout_contains(&output, "stopword-heavy-guided");
    assert_stdout_contains(&output, "Mean output");
    assert_stdout_contains(&output, "Variance");
    assert_stdout_contains(&output, "Required");
    assert_stdout_contains(&output, "Judge");
    assert_stdout_contains(&output, "Comparisons vs baseline");
    assert_stdout_contains(&output, "significant by heuristic");

    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn telemetry_commands_render_fixture_backed_outputs() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-telemetry-integration");
    write_workspace_telemetry_streams(&workspace, TELEMETRY_FIXTURE)?;

    let sessions = run_svdo_meter(&[
        "telemetry",
        "sessions",
        "--workspace",
        path_str(&workspace)?,
    ]);
    assert!(sessions.status.success());
    assert_stdout_contains(&sessions, "sess-telemetry-1");

    let runs = run_svdo_meter(&["telemetry", "runs", "--workspace", path_str(&workspace)?]);
    assert!(runs.status.success());
    assert_stdout_contains(&runs, "ENG-142");

    let inspection = run_svdo_meter(&[
        "telemetry",
        "inspect",
        "sess-telemetry-1",
        "--workspace",
        path_str(&workspace)?,
    ]);
    assert!(inspection.status.success());
    assert_stdout_contains(&inspection, "usage.reported");

    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn telemetry_commands_read_per_run_stream_directory() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-telemetry-streams-integration");
    write_workspace_telemetry_streams(&workspace, TELEMETRY_FIXTURE)?;

    let sessions = run_svdo_meter(&[
        "telemetry",
        "sessions",
        "--workspace",
        path_str(&workspace)?,
    ]);
    assert!(sessions.status.success());
    assert_stdout_contains(&sessions, "sess-telemetry-1");

    let inspection = run_svdo_meter(&[
        "telemetry",
        "inspect",
        "018f6f1b-97f1-7c04-9a96-111111111111",
        "--workspace",
        path_str(&workspace)?,
    ]);
    assert!(inspection.status.success());
    assert_stdout_contains(&inspection, "usage.reported");

    fs::remove_dir_all(workspace)?;
    Ok(())
}

fn assert_success_contains(args: &[&str], expected: &str) {
    let output = run_svdo_meter(args);

    assert!(output.status.success());
    assert_stdout_contains(&output, expected);
}

fn assert_stdout_contains(output: &Output, expected: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains(expected),
        "stdout did not contain `{expected}`:\nstatus: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
}

fn assert_stdout_row_contains(output: &Output, label: &str, expected_cells: &[&str]) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let row = stdout
        .lines()
        .find(|line| line.contains(label))
        .unwrap_or_else(|| {
            panic!(
                "stdout did not contain row `{label}`:\nstatus: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                output.status
            )
        });
    for expected in expected_cells {
        assert!(
            row.split_whitespace().any(|cell| cell == *expected),
            "row `{row}` did not contain cell `{expected}`:\nstatus: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status
        );
    }
}

fn assert_output_contains(output: &Output, expected: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains(expected) || stderr.contains(expected),
        "output did not contain `{expected}`:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn run_svdo_meter(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_svdo-meter"))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run svdo-meter: {error}"))
}

fn run_svdo_meter_with_path(args: &[&str], path_prefix: &Path) -> Output {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(path_prefix.to_path_buf()).chain(std::env::split_paths(&path)),
    )
    .unwrap_or_else(|error| panic!("failed to build PATH: {error}"));
    Command::new(env!("CARGO_BIN_EXE_svdo-meter"))
        .args(args)
        .env("PATH", path)
        .output()
        .unwrap_or_else(|error| panic!("failed to run svdo-meter: {error}"))
}

fn run_svdo_meter_with_path_and_env(
    args: &[&str],
    path_prefix: &Path,
    envs: &[(&str, &str)],
) -> Output {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(path_prefix.to_path_buf()).chain(std::env::split_paths(&path)),
    )
    .unwrap_or_else(|error| panic!("failed to build PATH: {error}"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_svdo-meter"));
    command.args(args);
    command.env("PATH", path);
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .output()
        .unwrap_or_else(|error| panic!("failed to run svdo-meter: {error}"))
}

fn write_workspace_telemetry_streams(workspace: &Path, contents: &str) -> std::io::Result<()> {
    let meter_dir = workspace.join(".svdo").join("meter");
    fs::create_dir_all(&meter_dir)?;
    for (index, line) in contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        fs::write(
            meter_dir.join(format!("fixture-run-{index}.jsonl")),
            format!("{line}\n"),
        )?;
    }
    Ok(())
}

fn write_eval_definition(workspace: &Path, file_name: &str, contents: &str) -> std::io::Result<()> {
    let eval_dir = workspace.join(".svdo").join("evals");
    fs::create_dir_all(&eval_dir)?;
    fs::write(eval_dir.join(file_name), contents)
}

fn write_compare_artifact(
    workspace: &Path,
    relative_path: &str,
    contents: &str,
) -> std::io::Result<()> {
    let path = workspace.join(".svdo").join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

fn write_standard(workspace: &Path, file_name: &str, contents: &str) -> std::io::Result<()> {
    let standard_dir = workspace.join(".svdo").join("standards");
    fs::create_dir_all(&standard_dir)?;
    fs::write(standard_dir.join(file_name), contents)
}

fn write_executable(workspace: &Path, file_name: &str, contents: &str) -> std::io::Result<PathBuf> {
    fs::create_dir_all(workspace)?;
    let path = workspace.join(file_name);
    fs::write(&path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)?;
    }
    Ok(path)
}

fn install_opencode_fixture(bin_dir: &Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(bin_dir)?;
    let path = bin_dir.join("opencode");
    fs::copy(env!("CARGO_BIN_EXE_svdo-meter-opencode-fixture"), &path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)?;
    }
    Ok(path)
}

fn path_str(path: &Path) -> std::io::Result<&str> {
    path.to_str()
        .ok_or_else(|| std::io::Error::other("temporary path must be UTF-8"))
}

fn unique_temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!("{}-{nanos}-{name}", std::process::id()))
}
