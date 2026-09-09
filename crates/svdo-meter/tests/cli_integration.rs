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

fn run_svdo_meter(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_svdo-meter"))
        .args(args)
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
