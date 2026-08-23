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
    assert_success_contains(&["report", "--help"], "svdo-meter report --last 7d");
    assert_success_contains(&["telemetry", "--help"], "sessions");
    assert_success_contains(&["telemetry", "sessions", "--help"], "List discovered");
    assert_success_contains(&["telemetry", "runs", "--help"], "List telemetry runs");
    assert_success_contains(
        &["telemetry", "inspect", "--help"],
        "Run identifier or provider session identifier",
    );
}

#[test]
fn invalid_run_arguments_fail_before_harness_execution() {
    let output = run_svdo_meter(&["run", "--ticket", "ENG-142", "--harness", "codex"]);

    assert!(!output.status.success());
    assert_output_contains(&output, "required");
    assert_output_contains(&output, "PROMPT");
}

#[test]
fn report_command_renders_fixture_backed_outputs() -> std::io::Result<()> {
    let workspace = unique_temp_path("svdo-meter-report-integration");
    write_workspace_telemetry(&workspace, REPORT_FIXTURE)?;

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
    write_workspace_telemetry(&workspace, TELEMETRY_FIXTURE)?;

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

fn write_workspace_telemetry(workspace: &Path, contents: &str) -> std::io::Result<()> {
    let svdo_dir = workspace.join(".svdo");
    fs::create_dir_all(&svdo_dir)?;
    fs::write(svdo_dir.join("meter.jsonl"), contents)
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
