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
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TraceReducer::new(query.clone()).finish(Vec::new()));
        }
        Err(error) => return Err(error),
    };
    let reader = std::io::BufReader::new(file);
    let mut reducer = TraceReducer::new(query.clone());
    let mut diagnostics: Vec<ReportDiagnostic> = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        apply_jsonl_line(
            &mut reducer,
            &mut diagnostics,
            index.saturating_add(1),
            &line?,
        );
    }
    Ok(reducer.finish(diagnostics))
}

pub fn load_telemetry_inspection(path: &Path) -> Result<TelemetryInspection, std::io::Error> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TelemetryInspection::from_jsonl_lines(std::iter::empty::<
                &str,
            >()));
        }
        Err(error) => return Err(error),
    };
    let reader = std::io::BufReader::new(file);
    let lines = reader.lines().collect::<Result<Vec<_>, _>>()?;
    Ok(TelemetryInspection::from_jsonl_lines(lines))
}

#[cfg(test)]
mod tests {
    use meter_core::{ClaudeConfig, HarnessConfig, HarnessKind, RawEventRetention, TicketId};
    use meter_engine::{HarnessOptions, RunError, RunRequest};

    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{engine, load_telemetry_inspection};

    #[test]
    fn missing_telemetry_file_loads_empty_inspection() -> std::io::Result<()> {
        let path = unique_temp_path("missing-meter.jsonl");

        let inspection = load_telemetry_inspection(&path)?;

        assert!(inspection.records.is_empty());
        assert!(inspection.diagnostics.is_empty());
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
