use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use meter_adapters::{ClaudeAdapter, CodexAdapter, JsonlEventStore};
use meter_core::{HarnessConfig, HarnessKind};
use meter_engine::{NdjsonWriteSink, RunEngine};
use meter_report::{
    ReportDiagnostic, ReportQuery, TelemetryInspection, TraceReducer, TraceReport, apply_jsonl_line,
};

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
        (HarnessKind::Gemini, _) => engine,
        (HarnessKind::Codex, _) => engine,
        (HarnessKind::Claude, _) => engine,
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

#[cfg(test)]
mod tests {
    use crate::cli::{EmitFormat, RunArgs, RunSink};

    use meter_core::{GeminiConfig, HarnessConfig, HarnessKind, RawEventRetention, TicketId};
    use meter_engine::{HarnessOptions, RunError, RunRequest};
    use meter_report::ReportQuery;

    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{RunSinkSelection, engine, load_report, load_telemetry_inspection};

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
            codex_config: Vec::new(),
        }
    }
}
