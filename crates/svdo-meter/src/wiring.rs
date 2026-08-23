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
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use meter_report::{ReportQuery, render_inspection, render_json, render_runs, render_sessions};

    use super::{default_telemetry_path, load_report, load_telemetry_inspection};

    const REPORT_FIXTURE: &str = include_str!("../../../tests/fixtures/report/single_work.jsonl");
    const TELEMETRY_FIXTURE: &str = include_str!("../../../tests/fixtures/telemetry/valid.jsonl");

    #[test]
    fn missing_telemetry_file_loads_empty_inspection() -> std::io::Result<()> {
        let path = unique_temp_path("missing-meter.jsonl");

        let inspection = load_telemetry_inspection(&path)?;

        assert!(inspection.records.is_empty());
        assert!(inspection.diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn default_telemetry_path_uses_workspace_svdo_log() {
        let workspace = Some(std::path::PathBuf::from("/tmp/svdo-workspace"));

        assert_eq!(
            default_telemetry_path(&workspace),
            std::path::PathBuf::from("/tmp/svdo-workspace/.svdo/meter.jsonl")
        );
    }

    #[test]
    fn report_boundary_loads_fixture_from_workspace_path() -> std::io::Result<()> {
        let workspace = unique_temp_path("svdo-meter-report-workspace");
        let telemetry_path = write_workspace_telemetry(&workspace, REPORT_FIXTURE)?;

        let report = load_report(
            &telemetry_path,
            &ReportQuery {
                work: Some("ENG-142".to_owned()),
                label: Some("plan".to_owned()),
                since: None,
                ..ReportQuery::default()
            },
        )?;

        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].work, "ENG-142");
        assert_eq!(report.groups[0].runs, 2);
        assert!(render_json(&report).is_ok());
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn telemetry_boundary_loads_fixture_for_all_rendered_subcommands() -> std::io::Result<()> {
        let workspace = unique_temp_path("svdo-meter-telemetry-workspace");
        let telemetry_path = write_workspace_telemetry(&workspace, TELEMETRY_FIXTURE)?;

        let inspection = load_telemetry_inspection(&telemetry_path)?;
        let sessions = render_sessions(&inspection);
        let runs = render_runs(&inspection);
        let inspected = render_inspection(&inspection, "sess-telemetry-1");

        assert!(sessions.contains("sess-telemetry-1"));
        assert!(runs.contains("ENG-142"));
        assert!(inspected.contains("usage.reported"));
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    fn write_workspace_telemetry(
        workspace: &std::path::Path,
        contents: &str,
    ) -> std::io::Result<std::path::PathBuf> {
        let svdo_dir = workspace.join(".svdo");
        fs::create_dir_all(&svdo_dir)?;
        let telemetry_path = svdo_dir.join("meter.jsonl");
        fs::write(&telemetry_path, contents)?;
        Ok(telemetry_path)
    }

    fn unique_temp_path(file_name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("{nanos}-{file_name}"))
    }
}
