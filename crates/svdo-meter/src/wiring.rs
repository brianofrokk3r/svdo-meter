use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use meter_adapters::{ClaudeAdapter, CodexAdapter, JsonlEventStore, OpenCodeAdapter};
use meter_core::{HarnessConfig, HarnessKind};
use meter_engine::{NdjsonWriteSink, RunEngine};
use meter_report::{
    ComparableEvalMetrics, ComparableRunSummary, ComparisonDiscoveryQuery,
    ComparisonDiscoveryReport, ComparisonMetric, ReportDiagnostic, ReportQuery,
    TelemetryInspection, TraceReducer, TraceReport, apply_jsonl_line,
    discover_comparable_runs_from_jsonl_lines,
};
use serde_json::Value;

use crate::cli::{EmitFormat, RunArgs, RunSink};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunSinkSelection {
    pub jsonl: bool,
    pub stdout_ndjson: bool,
}

impl RunSinkSelection {
    pub fn from_args(args: &RunArgs) -> Self {
        Self {
            jsonl: true,
            stdout_ndjson: args.sinks.contains(&RunSink::Stdout)
                || args.emit == Some(EmitFormat::Ndjson),
        }
    }
}

pub fn engine(
    workspace: &Option<PathBuf>,
    harness: HarnessKind,
    config: &HarnessConfig,
    sinks: RunSinkSelection,
) -> RunEngine {
    let base = workspace.as_deref().unwrap_or_else(|| Path::new("."));
    let store = Arc::new(JsonlEventStore::default_under(base));
    let mut engine = RunEngine::new(store);
    if sinks.stdout_ndjson {
        engine = engine.with_event_sink(Arc::new(NdjsonWriteSink::new(tokio::io::stdout())));
    }
    match (harness, config) {
        (HarnessKind::Codex, HarnessConfig::Codex(config)) => {
            engine.with_adapter(Arc::new(CodexAdapter::new(config.clone())))
        }
        (HarnessKind::Claude, HarnessConfig::Claude(config)) => {
            engine.with_adapter(Arc::new(ClaudeAdapter::new(config.binary.clone())))
        }
        (HarnessKind::OpenCode, HarnessConfig::OpenCode(config)) => {
            engine.with_adapter(Arc::new(OpenCodeAdapter::new(config.clone())))
        }
        (HarnessKind::Gemini, _) => engine,
        (HarnessKind::Codex, _) => engine,
        (HarnessKind::Claude, _) => engine,
        (HarnessKind::OpenCode, _) => engine,
    }
}

pub fn default_telemetry_path(workspace: &Option<PathBuf>) -> PathBuf {
    let base = workspace.as_deref().unwrap_or_else(|| Path::new("."));
    JsonlEventStore::default_under(base).path().to_path_buf()
}

pub fn load_report(path: &Path, query: &ReportQuery) -> Result<TraceReport, std::io::Error> {
    let mut reducer = TraceReducer::new(query.clone());
    let mut diagnostics: Vec<ReportDiagnostic> = Vec::new();
    let mut line_number = 0usize;
    for telemetry_path in telemetry_paths(path)? {
        let file = match std::fs::File::open(&telemetry_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let reader = std::io::BufReader::new(file);
        for line in reader.lines() {
            line_number = line_number.saturating_add(1);
            apply_jsonl_line(&mut reducer, &mut diagnostics, line_number, &line?);
        }
    }
    Ok(reducer.finish(diagnostics))
}

pub fn load_comparison_discovery(
    path: &Path,
    query: &ComparisonDiscoveryQuery,
) -> Result<ComparisonDiscoveryReport, std::io::Error> {
    let mut lines = Vec::new();
    for telemetry_path in telemetry_paths(path)? {
        let file = match std::fs::File::open(&telemetry_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let reader = std::io::BufReader::new(file);
        lines.extend(reader.lines().collect::<Result<Vec<_>, _>>()?);
    }
    let mut report = discover_comparable_runs_from_jsonl_lines(lines, query);
    enrich_comparison_from_artifacts(path, &mut report.runs)?;
    Ok(report)
}

pub fn load_telemetry_inspection(path: &Path) -> Result<TelemetryInspection, std::io::Error> {
    let mut lines = Vec::new();
    for telemetry_path in telemetry_paths(path)? {
        let file = match std::fs::File::open(&telemetry_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let reader = std::io::BufReader::new(file);
        lines.extend(reader.lines().collect::<Result<Vec<_>, _>>()?);
    }
    Ok(TelemetryInspection::from_jsonl_lines(lines))
}

fn telemetry_paths(path: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut stream_paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
            && entry.file_type()?.is_file()
        {
            stream_paths.push(path);
        }
    }
    stream_paths.sort();
    Ok(stream_paths)
}

fn enrich_comparison_from_artifacts(
    telemetry_path: &Path,
    runs: &mut [ComparableRunSummary],
) -> Result<(), std::io::Error> {
    let Some(svdo_dir) = telemetry_path.parent() else {
        return Ok(());
    };
    for artifact_dir in [svdo_dir.join("runs"), svdo_dir.join("evals")] {
        for path in json_artifact_paths(&artifact_dir)? {
            let contents = std::fs::read_to_string(path)?;
            let Ok(value) = serde_json::from_str::<Value>(&contents) else {
                continue;
            };
            apply_comparison_artifact(runs, &value);
        }
    }
    Ok(())
}

fn json_artifact_paths(path: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            paths.extend(json_artifact_paths(&path)?);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn apply_comparison_artifact(runs: &mut [ComparableRunSummary], value: &Value) {
    if let Some(results) = value.get("results").and_then(Value::as_array) {
        for result in results {
            apply_comparison_artifact(runs, result);
        }
    }

    let run_id = string_field_any(value, &["run_id", "runId", "run"]);
    let work = string_field_any(value, &["work", "ticket_id", "ticketId", "ticket"]);
    let harness = string_field_any(value, &["harness"]);
    let model = string_field_any(value, &["model", "requested_model", "resolved_model"]);

    for run in runs.iter_mut().filter(|run| {
        artifact_matches_run(
            run,
            run_id.as_deref(),
            work.as_deref(),
            harness.as_deref(),
            model.as_deref(),
        )
    }) {
        if let Some(cost) = cost_value(value) {
            run.estimated_cost_usd = ComparisonMetric::Observed(cost);
        }
        if let Some(rework) = u64_field_any(value, &["rework_count", "rework"]) {
            run.rework_count = ComparisonMetric::Observed(rework);
        } else if let Some(rework) = value
            .pointer("/metrics/rework_count")
            .and_then(Value::as_u64)
        {
            run.rework_count = ComparisonMetric::Observed(rework);
        }
        if let Some(eval) = eval_metrics(value) {
            run.eval = eval;
        }
    }
}

fn artifact_matches_run(
    run: &ComparableRunSummary,
    run_id: Option<&str>,
    work: Option<&str>,
    harness: Option<&str>,
    model: Option<&str>,
) -> bool {
    if let Some(run_id) = run_id {
        return run.run_id == run_id;
    }
    if let Some(work) = work
        && !run.works.iter().any(|candidate| candidate == work)
    {
        return false;
    }
    if let Some(harness) = harness
        && !run.harnesses.iter().any(|candidate| candidate == harness)
    {
        return false;
    }
    if let Some(model) = model
        && !run
            .requested_models
            .iter()
            .chain(&run.resolved_models)
            .any(|candidate| {
                candidate == model
                    || candidate.rsplit('/').next() == Some(model)
                    || Some(candidate.as_str()) == model.rsplit('/').next()
            })
    {
        return false;
    }
    work.is_some() || harness.is_some() || model.is_some()
}

fn cost_value(value: &Value) -> Option<f64> {
    f64_field_any(value, &["estimated_cost_usd", "cost_usd", "total_cost_usd"])
        .or_else(|| value.pointer("/cost/total").and_then(Value::as_f64))
        .or_else(|| value.pointer("/cost/estimated_usd").and_then(Value::as_f64))
}

fn eval_metrics(value: &Value) -> Option<ComparableEvalMetrics> {
    let score = f64_field_any(value, &["overall_score", "eval_score", "score"]);
    let checks = value.get("checks").and_then(Value::as_array);
    let required_passed = u64_field_any(value, &["required_checks_passed"])
        .or_else(|| {
            value
                .pointer("/required_checks/passed")
                .and_then(Value::as_u64)
        })
        .or_else(|| checks.map(|checks| required_checks_passed(checks)));
    let required_total = u64_field_any(value, &["required_checks_total"])
        .or_else(|| {
            value
                .pointer("/required_checks/total")
                .and_then(Value::as_u64)
        })
        .or_else(|| checks.map(|checks| required_checks_total(checks)));
    let violations = u64_field_any(value, &["violations_count", "violation_count"])
        .or_else(|| {
            value
                .get("violations")
                .and_then(Value::as_array)
                .map(|violations| violations.len() as u64)
        })
        .or_else(|| checks.map(|checks| check_violations(checks)));

    if score.is_none()
        && required_passed.is_none()
        && required_total.is_none()
        && violations.is_none()
    {
        return None;
    }

    Some(ComparableEvalMetrics {
        score: ComparisonMetric::from_option(score),
        required_checks_passed: ComparisonMetric::from_option(required_passed),
        required_checks_total: ComparisonMetric::from_option(required_total),
        violations: ComparisonMetric::from_option(violations),
    })
}

fn required_checks_total(checks: &[Value]) -> u64 {
    checks
        .iter()
        .filter(|check| {
            check
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count() as u64
}

fn required_checks_passed(checks: &[Value]) -> u64 {
    checks
        .iter()
        .filter(|check| {
            check
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter(|check| {
            check.get("outcome").and_then(Value::as_str) == Some("passed")
                || check.get("passed").and_then(Value::as_bool) == Some(true)
        })
        .count() as u64
}

fn check_violations(checks: &[Value]) -> u64 {
    checks
        .iter()
        .filter_map(|check| check.get("violations").and_then(Value::as_array))
        .map(|violations| violations.len() as u64)
        .sum()
}

fn string_field_any(value: &Value, fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

fn u64_field_any(value: &Value, fields: &[&str]) -> Option<u64> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(Value::as_u64))
}

fn f64_field_any(value: &Value, fields: &[&str]) -> Option<f64> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(Value::as_f64))
}

#[cfg(test)]
mod tests {
    use crate::cli::{EmitFormat, RunArgs, RunSink};

    use meter_core::{GeminiConfig, HarnessConfig, HarnessKind, RawEventRetention, TicketId};
    use meter_engine::{HarnessOptions, RunError, RunRequest};
    use meter_report::{ComparisonDiscoveryQuery, ReportQuery};

    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        RunSinkSelection, engine, load_comparison_discovery, load_report, load_telemetry_inspection,
    };

    #[test]
    fn run_sink_selection_preserves_jsonl_by_default() {
        let args = run_args(Vec::new(), None);

        let selection = RunSinkSelection::from_args(&args);

        assert_eq!(
            selection,
            RunSinkSelection {
                jsonl: true,
                stdout_ndjson: false
            }
        );
    }

    #[test]
    fn run_sink_selection_deduplicates_stdout_ndjson() {
        let args = run_args(
            vec![RunSink::Jsonl, RunSink::Stdout],
            Some(EmitFormat::Ndjson),
        );

        let selection = RunSinkSelection::from_args(&args);

        assert_eq!(
            selection,
            RunSinkSelection {
                jsonl: true,
                stdout_ndjson: true
            }
        );
    }

    #[test]
    fn missing_telemetry_file_loads_empty_inspection() -> std::io::Result<()> {
        let path = unique_temp_path("missing-meter");

        let inspection = load_telemetry_inspection(&path)?;

        assert!(inspection.records.is_empty());
        assert!(inspection.diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn report_loads_multiple_stream_files() -> std::io::Result<()> {
        let workspace = unique_temp_path("workspace-streams");
        let telemetry_dir = workspace.join(".svdo").join("meter");
        std::fs::create_dir_all(&telemetry_dir)?;
        std::fs::write(
            telemetry_dir.join("run-a.jsonl"),
            include_str!("../../../tests/fixtures/report/single_work.jsonl"),
        )?;
        std::fs::write(
            telemetry_dir.join("run-b.jsonl"),
            include_str!("../../../tests/fixtures/telemetry/valid.jsonl"),
        )?;

        let report = load_report(&telemetry_dir, &ReportQuery::default())?;

        assert!(report.groups.iter().any(|group| group.work == "ENG-142"));
        std::fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn comparison_discovery_loads_multiple_stream_files() -> std::io::Result<()> {
        let workspace = unique_temp_path("workspace-comparison-streams");
        let telemetry_dir = workspace.join(".svdo").join("meter");
        std::fs::create_dir_all(&telemetry_dir)?;
        std::fs::write(
            telemetry_dir.join("comparison-a.jsonl"),
            include_str!("../../../tests/fixtures/comparison/comparable_runs.jsonl"),
        )?;
        std::fs::write(
            telemetry_dir.join("comparison-b.jsonl"),
            include_str!("../../../tests/fixtures/telemetry/valid.jsonl"),
        )?;

        let report = load_comparison_discovery(
            &telemetry_dir,
            &ComparisonDiscoveryQuery {
                work: Some("ENG-142".to_owned()),
                harnesses: vec!["codex".to_owned()],
                ..ComparisonDiscoveryQuery::default()
            },
        )?;

        assert!(report.runs.iter().any(|run| {
            run.run_id == "018f6f1b-97f1-7c04-9a96-111111111111" && run.harnesses == vec!["codex"]
        }));
        assert_eq!(report.diagnostics.len(), 1);
        std::fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[tokio::test]
    async fn gemini_wiring_has_no_adapter_until_supported() {
        let workspace = Some(unique_temp_path("workspace"));
        let engine = engine(
            &workspace,
            HarnessKind::Gemini,
            &HarnessConfig::Gemini(GeminiConfig { model: None }),
            RunSinkSelection {
                jsonl: true,
                stdout_ndjson: false,
            },
        );

        let error = engine
            .run(RunRequest {
                ticket_id: TicketId::new("ENG-GEMINI").unwrap_or_else(|err| panic!("{err}")),
                label: None,
                harness: HarnessKind::Gemini,
                workspace,
                session_override: None,
                model: None,
                raw_event_retention: RawEventRetention::Disabled,
                execution_permission: meter_core::ExecutionPermissionMode::Standard,
                options: HarnessOptions::empty(),
                prompt: "Do work".to_owned(),
            })
            .await
            .err()
            .unwrap_or_else(|| panic!("expected missing adapter error"));

        assert!(matches!(
            error,
            RunError::MissingAdapter(HarnessKind::Gemini)
        ));
    }

    fn unique_temp_path(file_name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("{nanos}-{file_name}"))
    }

    fn run_args(sinks: Vec<RunSink>, emit: Option<EmitFormat>) -> RunArgs {
        RunArgs {
            ticket: "ENG-142".to_owned(),
            label: None,
            harness: HarnessKind::Codex,
            workspace: None,
            session: None,
            model: None,
            dangerous_bypass: false,
            claude_continue: false,
            claude_resume: None,
            claude_session_id: None,
            claude_fork_session: false,
            claude_permission_mode: None,
            claude_allowed_tools: Vec::new(),
            claude_disallowed_tools: Vec::new(),
            claude_add_dirs: Vec::new(),
            claude_mcp_configs: Vec::new(),
            claude_strict_mcp_config: false,
            claude_settings: None,
            claude_setting_sources: None,
            claude_system_prompt: None,
            claude_system_prompt_file: None,
            claude_append_system_prompts: Vec::new(),
            claude_append_system_prompt_files: Vec::new(),
            claude_max_turns: None,
            claude_max_budget_usd: None,
            prompt_file: None,
            prompt: Some("Do work".to_owned()),
            sinks,
            emit,
            codex_profile: None,
            codex_sandbox: None,
            codex_approve_for_me: false,
            codex_yolo: false,
            codex_skip_git_repo_check: false,
            codex_config: Vec::new(),
            opencode_agent: None,
        }
    }
}
