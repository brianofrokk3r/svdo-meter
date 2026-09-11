use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const REPORT_FIXTURE: &str = include_str!("../../../tests/fixtures/report/single_work.jsonl");
const TELEMETRY_FIXTURE: &str = include_str!("../../../tests/fixtures/telemetry/valid.jsonl");

#[test]
fn help_succeeds_for_documented_command_paths() {
    assert_success_contains(&["--help"], "svdo-meter");
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
    assert_success_contains(
        &["run", "--help"],
        "svdo-meter run --ticket ENG-142 --harness codex --dangerous-bypass PROMPT",
    );
    assert_success_contains(&["run", "--help"], "codex, claude, gemini, litellm");
    assert_success_contains(
        &["eval", "run", "--help"],
        "svdo-meter eval run --harness litellm --model gpt-5",
    );
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
fn eval_litellm_judge_harness_scores_judge_checks_without_live_api() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-litellm-judge-integration");
    write_eval_definition(
        &workspace,
        "litellm-judge.yaml",
        r#"
id: litellm-judge
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
        "Prefer direct API-backed harnesses.",
    )?;
    let bin_dir = workspace.join("bin");
    let curl = write_executable(
        &bin_dir,
        "curl",
        r#"#!/bin/sh
capture="${LITELLM_FIXTURE_CAPTURE:?missing capture path}"
body_path=""
previous=""
for arg in "$@"; do
  if [ "$previous" = "--data-binary" ]; then
    body_path="${arg#@}"
  fi
  previous="$arg"
done
cat > "$capture.stdin"
cp "$body_path" "$capture.body"
grep 'Authorization: Bearer fixture-litellm-key' "$capture.stdin" >/dev/null || exit 12
grep '"model":"fixture-litellm-model"' "$capture.body" >/dev/null || exit 13
grep 'Return only one JSON object' "$capture.body" >/dev/null || exit 14
printf 'HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n'
printf '{"model":"fixture-litellm-model","choices":[{"message":{"content":"{\"score\":1.0,\"passed\":true,\"violations\":[]}"}}],"usage":{"prompt_tokens":13,"completion_tokens":8}}'
"#,
    )?;
    let capture = workspace.join("litellm-capture");

    let output = run_svdo_meter_with_path_and_env(
        &[
            "eval",
            "run",
            "litellm-judge",
            "--workspace",
            path_str(&workspace)?,
            "--harness",
            "litellm",
            "--model",
            "fixture-litellm-model",
            "--format",
            "json",
        ],
        curl.parent()
            .ok_or_else(|| std::io::Error::other("missing bin dir"))?,
        &[
            ("LITELLM_API_KEY", "fixture-litellm-key"),
            ("LITELLM_API_BASE", "https://fixture.litellm.local"),
            ("LITELLM_FIXTURE_CAPTURE", path_str(&capture)?),
        ],
    );

    assert!(output.status.success());
    assert_stdout_contains(&output, "\"outcome\": \"passed\"");
    assert_stdout_contains(&output, "\"model\": \"fixture-litellm-model\"");
    assert_stdout_contains(&output, "\"harness\": \"litellm\"");
    assert_stdout_contains(&output, "\"input\": 13");
    assert_stdout_contains(&output, "\"output\": 8");
    assert_output_not_contains(&output, "fixture-litellm-key");
    let headers = fs::read_to_string(workspace.join("litellm-capture.stdin"))?;
    let body = fs::read_to_string(workspace.join("litellm-capture.body"))?;
    assert!(headers.contains("Authorization: Bearer fixture-litellm-key"));
    assert!(body.contains("\"model\":\"fixture-litellm-model\""));
    assert!(body.contains("Return only one JSON object"));
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_litellm_without_api_key_reports_actionable_error() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-litellm-missing-key-integration");
    write_eval_definition(
        &workspace,
        "litellm-missing-key.yaml",
        r#"
id: litellm-missing-key
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
        "Prefer direct API-backed harnesses.",
    )?;
    let bin_dir = workspace.join("bin");
    let marker = workspace.join("curl-was-called");
    let curl = write_executable(
        &bin_dir,
        "curl",
        r#"#!/bin/sh
touch "${LITELLM_SHOULD_NOT_BE_CALLED:?missing marker path}"
exit 42
"#,
    )?;

    let output = run_svdo_meter_with_path_without_litellm_key(
        &[
            "eval",
            "run",
            "litellm-missing-key",
            "--workspace",
            path_str(&workspace)?,
            "--harness",
            "litellm",
            "--model",
            "fixture-litellm-model",
        ],
        curl.parent()
            .ok_or_else(|| std::io::Error::other("missing bin dir"))?,
        &[("LITELLM_SHOULD_NOT_BE_CALLED", path_str(&marker)?)],
    );

    assert!(!output.status.success());
    assert_output_contains(&output, "LITELLM_API_KEY");
    assert_output_contains(&output, "set LITELLM_API_KEY");
    assert!(!marker.exists());
    fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn eval_litellm_invalid_judge_response_fails_without_leaking_secret() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-eval-litellm-invalid-integration");
    write_eval_definition(
        &workspace,
        "litellm-invalid.yaml",
        r#"
id: litellm-invalid
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
    let bin_dir = workspace.join("bin");
    let curl = write_executable(
        &bin_dir,
        "curl",
        r#"#!/bin/sh
printf 'HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n'
printf '{"model":"fixture-litellm-model","choices":[{"message":{"content":"I checked this but did not return JSON."}}],"usage":{"prompt_tokens":3,"completion_tokens":2}}'
"#,
    )?;

    let output = run_svdo_meter_with_path_and_env(
        &[
            "eval",
            "run",
            "litellm-invalid",
            "--workspace",
            path_str(&workspace)?,
            "--harness",
            "litellm",
            "--model",
            "fixture-litellm-model",
            "--format",
            "json",
        ],
        curl.parent()
            .ok_or_else(|| std::io::Error::other("missing bin dir"))?,
        &[
            ("LITELLM_API_KEY", "fixture-litellm-key"),
            ("LITELLM_API_BASE", "https://fixture.litellm.local"),
        ],
    );

    assert!(!output.status.success());
    assert_stdout_contains(&output, "\"id\": \"litellm-invalid\"");
    assert_stdout_contains(&output, "\"id\": \"cli-design\"");
    assert_stdout_contains(&output, "\"outcome\": \"failed\"");
    assert_stdout_contains(
        &output,
        "invalid judge response for eval `litellm-invalid` check `cli-design`",
    );
    assert_stdout_contains(&output, "\"harness\": \"litellm\"");
    assert_stdout_contains(&output, "\"model\": \"fixture-litellm-model\"");
    assert_output_not_contains(&output, "fixture-litellm-key");
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
        "--workspace",
        path_str(&repo_root).expect("repo root path must be UTF-8"),
        "--format",
        "json",
    ]);

    assert!(output.status.success());
    assert_stdout_contains(&output, "\"id\": \"rust-cli-smoke\"");
    assert_stdout_contains(&output, "\"id\": \"fixture-integrity\"");
    assert_stdout_contains(&output, "\"harness\": \"judge-unavailable\"");
}

#[test]
fn invalid_run_arguments_fail_before_harness_execution() {
    let output = run_svdo_meter(&["run", "--ticket", "ENG-142", "--harness", "codex"]);

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
        "--codex-yolo",
        "Implement ENG-142",
    ]);

    assert!(!output.status.success());
    assert_output_contains(
        &output,
        "Codex-specific --codex-* options require --harness codex",
    );
}

#[test]
fn litellm_run_without_api_key_reports_actionable_error() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-litellm-missing-key-integration");
    fs::create_dir_all(&workspace)?;

    let output = run_svdo_meter_without_litellm_key(&[
        "run",
        "--ticket",
        "ENG-LITELLM",
        "--harness",
        "litellm",
        "--workspace",
        path_str(&workspace)?,
        "Implement ENG-LITELLM",
    ]);

    assert!(!output.status.success());
    assert_output_contains(&output, "LITELLM_API_KEY");
    assert_output_contains(&output, "set LITELLM_API_KEY");
    let mut telemetry = String::new();
    for entry in fs::read_dir(workspace.join(".svdo").join("meter"))? {
        telemetry.push_str(&fs::read_to_string(entry?.path())?);
    }
    assert!(telemetry.contains("\"harness\":\"litellm\""));
    assert!(!telemetry.contains("Authorization"));
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
    assert!(
        stdout.contains(expected),
        "stdout did not contain `{expected}`:\n{stdout}"
    );
}

fn assert_output_contains(output: &Output, expected: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains(expected) || stderr.contains(expected),
        "output did not contain `{expected}`:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn assert_output_not_contains(output: &Output, unexpected: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains(unexpected) && !stderr.contains(unexpected),
        "output contained `{unexpected}`:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn run_svdo_meter(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_svdo-meter"))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run svdo-meter: {error}"))
}

fn run_svdo_meter_without_litellm_key(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_svdo-meter"))
        .args(args)
        .env_remove("LITELLM_API_KEY")
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

fn run_svdo_meter_with_path_without_litellm_key(
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
    command.env_remove("LITELLM_API_KEY");
    for (key, value) in envs {
        command.env(key, value);
    }
    command
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
