use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use meter_adapters::{CodexAdapter, JsonlEventStore};
use meter_core::{HarnessConfig, HarnessKind};
use meter_engine::RunEngine;
use meter_report::{
    ReportDiagnostic, ReportQuery, TelemetryInspection, TraceReducer, TraceReport, apply_jsonl_line,
};

pub fn engine(
    workspace: &Option<PathBuf>,
    harness: HarnessKind,
    config: &HarnessConfig,
) -> RunEngine {
    let base = workspace.as_deref().unwrap_or_else(|| Path::new("."));
    let store = Arc::new(JsonlEventStore::default_under(base));
    let engine = RunEngine::new(store);
    match (harness, config) {
        (HarnessKind::Codex, HarnessConfig::Codex(config)) => {
            engine.with_adapter(Arc::new(CodexAdapter::new(config.binary.clone())))
        }
        (HarnessKind::Claude | HarnessKind::Gemini, _) => engine,
        (HarnessKind::Codex, _) => engine,
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
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut paths = Vec::new();
    let legacy_path = path.with_extension("jsonl");
    if legacy_path.is_file() {
        paths.push(legacy_path);
    }

    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(paths),
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
    paths.extend(stream_paths);
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use meter_core::{ClaudeConfig, HarnessConfig, HarnessKind, RawEventRetention, TicketId};
    use meter_engine::{HarnessOptions, RunError, RunRequest};
    use meter_report::ReportQuery;

    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{engine, load_report, load_telemetry_inspection};

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
    async fn claude_wiring_does_not_register_codex_adapter() {
        let workspace = Some(unique_temp_path("workspace"));
        let engine = engine(
            &workspace,
            HarnessKind::Claude,
            &HarnessConfig::Claude(ClaudeConfig { model: None }),
        );

        let error = engine
            .run(RunRequest {
                ticket_id: TicketId::new("ENG-CLAUDE").unwrap_or_else(|err| panic!("{err}")),
                label: None,
                harness: HarnessKind::Claude,
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
            RunError::MissingAdapter(HarnessKind::Claude)
        ));
    }

    fn unique_temp_path(file_name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("{nanos}-{file_name}"))
    }
}
