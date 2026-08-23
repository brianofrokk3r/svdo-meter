use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use meter_adapters::{CodexAdapter, JsonlEventStore};
use meter_core::HarnessConfig;
use meter_engine::RunEngine;
use meter_report::{
    ReportDiagnostic, ReportQuery, TelemetryInspection, TraceReducer, TraceReport, apply_jsonl_line,
};

pub fn engine(workspace: &Option<PathBuf>, config: &HarnessConfig) -> RunEngine {
    let base = workspace.as_deref().unwrap_or_else(|| Path::new("."));
    let store = Arc::new(JsonlEventStore::default_under(base));
    let codex_binary = match config {
        HarnessConfig::Codex(config) => config.binary.clone(),
        HarnessConfig::Claude(_) | HarnessConfig::Gemini(_) => PathBuf::from("codex"),
    };
    RunEngine::new(store).with_adapter(Arc::new(CodexAdapter::new(codex_binary)))
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
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::load_telemetry_inspection;

    #[test]
    fn missing_telemetry_file_loads_empty_inspection() -> std::io::Result<()> {
        let path = unique_temp_path("missing-meter.jsonl");

        let inspection = load_telemetry_inspection(&path)?;

        assert!(inspection.records.is_empty());
        assert!(inspection.diagnostics.is_empty());
        Ok(())
    }

    fn unique_temp_path(file_name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("{nanos}-{file_name}"))
    }
}
