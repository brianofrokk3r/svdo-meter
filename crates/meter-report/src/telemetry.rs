use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use meter_core::{EventPayload, TokenUsage};

use crate::{ReportDiagnostic, TelemetryRecord};

const UNKNOWN_WORK: &str = "Unknown";

#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryInspection {
    pub records: Vec<InspectionRecord>,
    pub diagnostics: Vec<ReportDiagnostic>,
}

impl TelemetryInspection {
    pub fn from_jsonl_lines<I, S>(lines: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut records = Vec::new();
        let mut diagnostics = Vec::new();
        for (index, line) in lines.into_iter().enumerate() {
            let line_number = index.saturating_add(1);
            let line = line.as_ref();
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<TelemetryRecord>(line) {
                Ok(record) => records.push(InspectionRecord::from_record(line_number, record)),
                Err(error) => diagnostics.push(ReportDiagnostic {
                    line: line_number,
                    message: error.to_string(),
                }),
            }
        }
        records.sort_by_key(|record| (record.occurred_at, record.line));
        Self {
            records,
            diagnostics,
        }
    }

    pub fn session_summaries(&self) -> Vec<SessionSummary> {
        let associations = SessionAssociations::from_records(&self.records);
        let mut sessions = BTreeMap::<String, SessionSummaryAccumulator>::new();
        for record in &self.records {
            for session_id in associations.sessions_for(record) {
                sessions
                    .entry(session_id)
                    .or_insert_with_key(|session_id| {
                        SessionSummaryAccumulator::new(session_id.clone())
                    })
                    .apply(record);
            }
        }
        sessions
            .into_values()
            .map(SessionSummaryAccumulator::finish)
            .collect()
    }

    pub fn run_summaries(&self) -> Vec<RunSummary> {
        let associations = SessionAssociations::from_records(&self.records);
        let mut runs = BTreeMap::<String, RunSummaryAccumulator>::new();
        for record in &self.records {
            runs.entry(record.run_id.clone())
                .or_insert_with_key(|run_id| RunSummaryAccumulator::new(run_id.clone()))
                .apply(record, associations.sessions_for(record));
        }
        runs.into_values()
            .map(RunSummaryAccumulator::finish)
            .collect()
    }

    pub fn inspect(&self, id: &str) -> Vec<InspectionRecord> {
        let associations = SessionAssociations::from_records(&self.records);
        self.records
            .iter()
            .filter(|record| record.run_id == id || associations.record_matches_session(record, id))
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InspectionRecord {
    pub line: usize,
    pub occurred_at: DateTime<Utc>,
    pub run_id: String,
    pub work: String,
    pub label: Option<String>,
    pub harness: String,
    pub session_id: Option<String>,
    pub payload: EventPayload,
}

impl InspectionRecord {
    fn from_record(line: usize, record: TelemetryRecord) -> Self {
        Self {
            line,
            occurred_at: record.occurred_at,
            run_id: record.run_id.to_string(),
            work: record
                .ticket_id
                .map(|ticket_id| ticket_id.to_string())
                .unwrap_or_else(|| UNKNOWN_WORK.to_owned()),
            label: record.label,
            harness: record.harness.to_string(),
            session_id: record.session_id.map(|session_id| session_id.to_string()),
            payload: record.payload,
        }
    }

    fn event_name(&self) -> &'static str {
        match &self.payload {
            EventPayload::RunStarted(_) => "run.started",
            EventPayload::SessionDiscovered(_) => "session.discovered",
            EventPayload::HarnessEvent(_) => "harness.event",
            EventPayload::UsageReported(_) => "usage.reported",
            EventPayload::CommandStarted(_) => "command.started",
            EventPayload::CommandCompleted(_) => "command.completed",
            EventPayload::FilesChanged(_) => "files.changed",
            EventPayload::ToolStarted(_) => "tool.started",
            EventPayload::ToolCompleted(_) => "tool.completed",
            EventPayload::RunCompleted(_) => "run.completed",
            EventPayload::RunFailed(_) => "run.failed",
        }
    }

    fn payload_detail(&self) -> String {
        match &self.payload {
            EventPayload::RunStarted(started) => {
                format!("prompt_recorded={}", started.prompt_recorded)
            }
            EventPayload::SessionDiscovered(discovered) => {
                format!("source={}", discovered.source)
            }
            EventPayload::HarnessEvent(event) => format!(
                "source_event={}, raw_retained={}",
                event.source_event, event.retained_raw_payload
            ),
            EventPayload::UsageReported(usage) => token_detail(usage),
            EventPayload::CommandStarted(command) => format!(
                "command_id={}, kind={}",
                display_option(command.command_id.as_deref()),
                display_option(command.command_kind.as_deref())
            ),
            EventPayload::CommandCompleted(command) => format!(
                "command_id={}, success={}, exit_code={}, duration_ms={}",
                display_option(command.command_id.as_deref()),
                command.success,
                display_i32(command.exit_code),
                display_u64(command.duration_ms)
            ),
            EventPayload::FilesChanged(files) => format!("count={}", files.count),
            EventPayload::ToolStarted(tool) => format!(
                "tool_id={}, name={}",
                display_option(tool.tool_id.as_deref()),
                display_option(tool.tool_name.as_deref())
            ),
            EventPayload::ToolCompleted(tool) => format!(
                "tool_id={}, name={}, success={}, duration_ms={}",
                display_option(tool.tool_id.as_deref()),
                display_option(tool.tool_name.as_deref()),
                tool.success,
                display_u64(tool.duration_ms)
            ),
            EventPayload::RunCompleted(completed) => format!(
                "exit_code={}, wall_time_ms={}, {}",
                display_i32(completed.exit_code),
                completed.metrics.wall_time_ms,
                token_detail(&completed.metrics.token_usage)
            ),
            EventPayload::RunFailed(failed) => format!(
                "reason={}, exit_code={}, wall_time_ms={}, {}",
                failed.reason,
                display_i32(failed.exit_code),
                failed.metrics.wall_time_ms,
                token_detail(&failed.metrics.token_usage)
            ),
        }
    }

    fn token_warning(&self) -> Option<String> {
        match &self.payload {
            EventPayload::UsageReported(usage) => token_warning(usage),
            EventPayload::RunCompleted(completed) => token_warning(&completed.metrics.token_usage),
            EventPayload::RunFailed(failed) => token_warning(&failed.metrics.token_usage),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub session_id: String,
    pub runs: Vec<String>,
    pub works: Vec<String>,
    pub labels: Vec<String>,
    pub harnesses: Vec<String>,
    pub sources: Vec<String>,
    pub first_seen: Option<DateTime<Utc>>,
    pub records: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub run_id: String,
    pub works: Vec<String>,
    pub labels: Vec<String>,
    pub harnesses: Vec<String>,
    pub sessions: Vec<String>,
    pub first_event: Option<DateTime<Utc>>,
    pub last_event: Option<DateTime<Utc>>,
    pub records: u64,
    pub token_warnings: Vec<String>,
}

#[derive(Debug, Default)]
struct SessionAssociations {
    by_run: BTreeMap<String, BTreeSet<String>>,
}

impl SessionAssociations {
    fn from_records(records: &[InspectionRecord]) -> Self {
        let mut by_run = BTreeMap::<String, BTreeSet<String>>::new();
        for record in records {
            if let Some(session_id) = &record.session_id {
                by_run
                    .entry(record.run_id.clone())
                    .or_default()
                    .insert(session_id.clone());
            }
        }
        Self { by_run }
    }

    fn sessions_for(&self, record: &InspectionRecord) -> BTreeSet<String> {
        let mut sessions = self.by_run.get(&record.run_id).cloned().unwrap_or_default();
        if let Some(session_id) = &record.session_id {
            sessions.insert(session_id.clone());
        }
        sessions
    }

    fn record_matches_session(&self, record: &InspectionRecord, session_id: &str) -> bool {
        record.session_id.as_deref() == Some(session_id)
            || self
                .by_run
                .get(&record.run_id)
                .is_some_and(|sessions| sessions.contains(session_id))
    }
}

#[derive(Debug)]
struct SessionSummaryAccumulator {
    summary: SessionSummary,
}

impl SessionSummaryAccumulator {
    fn new(session_id: String) -> Self {
        Self {
            summary: SessionSummary {
                session_id,
                runs: Vec::new(),
                works: Vec::new(),
                labels: Vec::new(),
                harnesses: Vec::new(),
                sources: Vec::new(),
                first_seen: None,
                records: 0,
            },
        }
    }

    fn apply(&mut self, record: &InspectionRecord) {
        self.summary.records = self.summary.records.saturating_add(1);
        push_unique(&mut self.summary.runs, &record.run_id);
        push_unique(&mut self.summary.works, &record.work);
        if let Some(label) = &record.label {
            push_unique(&mut self.summary.labels, label);
        }
        push_unique(&mut self.summary.harnesses, &record.harness);
        if let EventPayload::SessionDiscovered(discovered) = &record.payload {
            push_unique(&mut self.summary.sources, &discovered.source);
        }
        self.summary.first_seen = min_time(self.summary.first_seen, record.occurred_at);
    }

    fn finish(self) -> SessionSummary {
        self.summary
    }
}

#[derive(Debug)]
struct RunSummaryAccumulator {
    summary: RunSummary,
}

impl RunSummaryAccumulator {
    fn new(run_id: String) -> Self {
        Self {
            summary: RunSummary {
                run_id,
                works: Vec::new(),
                labels: Vec::new(),
                harnesses: Vec::new(),
                sessions: Vec::new(),
                first_event: None,
                last_event: None,
                records: 0,
                token_warnings: Vec::new(),
            },
        }
    }

    fn apply(&mut self, record: &InspectionRecord, sessions: BTreeSet<String>) {
        self.summary.records = self.summary.records.saturating_add(1);
        push_unique(&mut self.summary.works, &record.work);
        if let Some(label) = &record.label {
            push_unique(&mut self.summary.labels, label);
        }
        push_unique(&mut self.summary.harnesses, &record.harness);
        for session in sessions {
            push_unique(&mut self.summary.sessions, &session);
        }
        if let Some(warning) = record.token_warning() {
            push_unique(&mut self.summary.token_warnings, &warning);
        }
        self.summary.first_event = min_time(self.summary.first_event, record.occurred_at);
        self.summary.last_event = max_time(self.summary.last_event, record.occurred_at);
    }

    fn finish(self) -> RunSummary {
        self.summary
    }
}

pub fn render_sessions(inspection: &TelemetryInspection) -> String {
    let sessions = inspection.session_summaries();
    if sessions.is_empty() {
        return diagnostics_suffix(
            "SVDO Telemetry Sessions\n\nNo sessions found in .svdo/meter.jsonl.",
            &inspection.diagnostics,
        );
    }

    let mut output = String::from("SVDO Telemetry Sessions\n");
    for session in sessions {
        output.push_str(&format!(
            "\nSession\n  {}\n  First Seen  {}\n  Work        {}\n  Label       {}\n  Harness     {}\n  Runs        {}\n  Source      {}\n  Records     {}\n",
            session.session_id,
            display_time(session.first_seen),
            display_list(&session.works),
            display_list(&session.labels),
            display_list(&session.harnesses),
            display_list(&session.runs),
            display_list(&session.sources),
            session.records
        ));
    }
    diagnostics_suffix(output.trim_end(), &inspection.diagnostics)
}

pub fn render_runs(inspection: &TelemetryInspection) -> String {
    let runs = inspection.run_summaries();
    if runs.is_empty() {
        return diagnostics_suffix(
            "SVDO Telemetry Runs\n\nNo runs found in .svdo/meter.jsonl.",
            &inspection.diagnostics,
        );
    }

    let mut output = String::from("SVDO Telemetry Runs\n");
    for run in runs {
        output.push_str(&format!(
            "\nRun\n  {}\n  First Event {}\n  Last Event  {}\n  Work        {}\n  Label       {}\n  Harness     {}\n  Session     {}\n  Records     {}\n  Tokens      {}\n",
            run.run_id,
            display_time(run.first_event),
            display_time(run.last_event),
            display_list(&run.works),
            display_list(&run.labels),
            display_list(&run.harnesses),
            display_list(&run.sessions),
            run.records,
            display_token_warnings(&run.token_warnings)
        ));
    }
    diagnostics_suffix(output.trim_end(), &inspection.diagnostics)
}

pub fn render_inspection(inspection: &TelemetryInspection, id: &str) -> String {
    let records = inspection.inspect(id);
    if records.is_empty() {
        return diagnostics_suffix(
            &format!("SVDO Telemetry Inspect\n\nNo telemetry events found for `{id}`."),
            &inspection.diagnostics,
        );
    }

    let mut output = format!("SVDO Telemetry Inspect\n\nIdentifier\n  {id}\n");
    for record in records {
        output.push_str(&format!(
            "\nEvent\n  Line        {}\n  Time        {}\n  Type        {}\n  Work        {}\n  Run         {}\n  Session     {}\n  Harness     {}\n  Label       {}\n  Details     {}\n",
            record.line,
            record.occurred_at.to_rfc3339(),
            record.event_name(),
            record.work,
            record.run_id,
            display_option(record.session_id.as_deref()),
            record.harness,
            display_option(record.label.as_deref()),
            record.payload_detail()
        ));
        if let Some(warning) = record.token_warning() {
            output.push_str(&format!("\n  Token Check {warning}\n"));
        }
    }
    diagnostics_suffix(output.trim_end(), &inspection.diagnostics)
}

fn token_detail(usage: &TokenUsage) -> String {
    format!(
        "tokens input={}, cached_input={}, cache_write={}, output={}, reasoning={}",
        display_u64(usage.input_tokens),
        display_u64(usage.cached_input_tokens),
        display_u64(usage.cache_write_tokens),
        display_u64(usage.output_tokens),
        display_u64(usage.reasoning_tokens)
    )
}

fn token_warning(usage: &TokenUsage) -> Option<String> {
    let mut missing = Vec::new();
    if usage.input_tokens.is_none() {
        missing.push("input_tokens");
    }
    if usage.cached_input_tokens.is_none() {
        missing.push("cached_input_tokens");
    }
    if usage.cache_write_tokens.is_none() {
        missing.push("cache_write_tokens");
    }
    if usage.output_tokens.is_none() {
        missing.push("output_tokens");
    }
    if usage.reasoning_tokens.is_none() {
        missing.push("reasoning_tokens");
    }
    (!missing.is_empty()).then(|| format!("Missing token fields: {}", missing.join(", ")))
}

fn min_time(current: Option<DateTime<Utc>>, next: DateTime<Utc>) -> Option<DateTime<Utc>> {
    Some(current.map_or(next, |current| current.min(next)))
}

fn max_time(current: Option<DateTime<Utc>>, next: DateTime<Utc>) -> Option<DateTime<Utc>> {
    Some(current.map_or(next, |current| current.max(next)))
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_owned());
        values.sort();
    }
}

fn display_time(value: Option<DateTime<Utc>>) -> String {
    value.map_or_else(|| "Unavailable".to_owned(), |value| value.to_rfc3339())
}

fn display_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "Unavailable".to_owned(), |value| value.to_string())
}

fn display_i32(value: Option<i32>) -> String {
    value.map_or_else(|| "Unavailable".to_owned(), |value| value.to_string())
}

fn display_option(value: Option<&str>) -> String {
    value.unwrap_or("Unavailable").to_owned()
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "Unavailable".to_owned()
    } else {
        values.join(", ")
    }
}

fn display_token_warnings(warnings: &[String]) -> String {
    if warnings.is_empty() {
        "complete".to_owned()
    } else {
        warnings.join("; ")
    }
}

fn diagnostics_suffix(base: &str, diagnostics: &[ReportDiagnostic]) -> String {
    let mut output = base.to_owned();
    if !diagnostics.is_empty() {
        output.push_str("\n\nDiagnostics\n");
        for diagnostic in diagnostics {
            output.push_str(&format!(
                "  Skipped line {}: {}\n",
                diagnostic.line, diagnostic.message
            ));
        }
        return output.trim_end().to_owned();
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = include_str!("../../../tests/fixtures/telemetry/valid.jsonl");
    const MALFORMED: &str = include_str!("../../../tests/fixtures/telemetry/malformed.jsonl");
    const MISSING_TOKENS: &str =
        include_str!("../../../tests/fixtures/telemetry/missing_token_fields.jsonl");

    #[test]
    fn lists_sessions_from_discovery_events() {
        let inspection = TelemetryInspection::from_jsonl_lines(VALID.lines());

        let sessions = inspection.session_summaries();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "sess-telemetry-1");
        assert_eq!(sessions[0].sources, vec!["codex-jsonl".to_owned()]);
        assert_eq!(
            sessions[0].runs,
            vec!["018f6f1b-97f1-7c04-9a96-111111111111".to_owned()]
        );

        let output = render_sessions(&inspection);
        assert!(output.contains("SVDO Telemetry Sessions"));
        assert!(output.contains("sess-telemetry-1"));
        assert!(output.contains("codex-jsonl"));
    }

    #[test]
    fn lists_runs_with_session_association() {
        let inspection = TelemetryInspection::from_jsonl_lines(VALID.lines());

        let runs = inspection.run_summaries();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].works, vec!["ENG-142".to_owned()]);
        assert_eq!(runs[0].sessions, vec!["sess-telemetry-1".to_owned()]);

        let output = render_runs(&inspection);
        assert!(output.contains("018f6f1b-97f1-7c04-9a96-111111111111"));
        assert!(output.contains("ENG-142"));
    }

    #[test]
    fn inspects_run_and_session_ids_in_order() {
        let inspection = TelemetryInspection::from_jsonl_lines(VALID.lines());

        let run_output = render_inspection(&inspection, "018f6f1b-97f1-7c04-9a96-111111111111");
        assert!(run_output.contains("run.started"));
        assert!(run_output.contains("session.discovered"));
        assert!(run_output.contains("run.completed"));

        let session_output = render_inspection(&inspection, "sess-telemetry-1");
        assert!(session_output.contains("run.started"));
        assert!(session_output.contains("run.completed"));
    }

    #[test]
    fn malformed_lines_are_diagnostics_without_losing_valid_records() {
        let inspection = TelemetryInspection::from_jsonl_lines(MALFORMED.lines());

        assert_eq!(inspection.records.len(), 2);
        assert_eq!(inspection.diagnostics.len(), 1);
        assert_eq!(inspection.diagnostics[0].line, 2);

        let output = render_runs(&inspection);
        assert!(output.contains("Skipped line 2"));
        assert!(output.contains("ENG-142"));
    }

    #[test]
    fn missing_token_fields_are_called_out() {
        let inspection = TelemetryInspection::from_jsonl_lines(MISSING_TOKENS.lines());

        let output = render_inspection(&inspection, "018f6f1b-97f1-7c04-9a96-333333333333");
        assert!(output.contains("Missing token fields"));
        assert!(output.contains("output_tokens"));
        assert!(output.contains("reasoning_tokens"));
    }

    #[test]
    fn empty_telemetry_renders_clear_output() {
        let inspection = TelemetryInspection::from_jsonl_lines(std::iter::empty::<&str>());

        assert!(render_sessions(&inspection).contains("No sessions found"));
        assert!(render_runs(&inspection).contains("No runs found"));
        assert!(render_inspection(&inspection, "missing").contains("No telemetry events found"));
    }

    #[test]
    fn unknown_identifier_renders_clear_output_with_diagnostics() {
        let inspection = TelemetryInspection::from_jsonl_lines(MALFORMED.lines());

        let output = render_inspection(&inspection, "missing");
        assert!(output.contains("No telemetry events found for `missing`"));
        assert!(output.contains("Skipped line 2"));
    }
}
