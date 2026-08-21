use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{DateTime, Utc};
use meter_core::{EventPayload, HarnessKind, RunId, RunMetrics, SessionId, TicketId, TokenUsage};
use serde::{Deserialize, Serialize};

const UNKNOWN_WORK: &str = "Unknown";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportQuery {
    pub work: Option<String>,
    pub label: Option<String>,
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceReport {
    pub groups: Vec<TraceGroup>,
    pub diagnostics: Vec<ReportDiagnostic>,
}

impl TraceReport {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceGroup {
    pub work: String,
    pub harnesses: Vec<String>,
    pub sessions: Vec<String>,
    pub runs: u64,
    pub agent_time_ms: Option<u64>,
    pub tokens: TokenTotals,
    pub records: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenTotals {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cache: Option<u64>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportDiagnostic {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TelemetryRecord {
    pub occurred_at: DateTime<Utc>,
    pub run_id: RunId,
    #[serde(default)]
    pub ticket_id: Option<TicketId>,
    #[serde(default)]
    pub label: Option<String>,
    pub harness: HarnessKind,
    #[serde(default)]
    pub session_id: Option<SessionId>,
    pub payload: EventPayload,
}

impl TelemetryRecord {
    fn work_key(&self) -> String {
        self.ticket_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| UNKNOWN_WORK.to_owned())
    }
}

pub fn report_from_jsonl_lines<I, S>(lines: I, query: &ReportQuery) -> TraceReport
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut reducer = TraceReducer::new(query.clone());
    let mut diagnostics = Vec::new();
    for (index, line) in lines.into_iter().enumerate() {
        apply_jsonl_line(
            &mut reducer,
            &mut diagnostics,
            index.saturating_add(1),
            line.as_ref(),
        );
    }
    reducer.finish(diagnostics)
}

pub fn apply_jsonl_line(
    reducer: &mut TraceReducer,
    diagnostics: &mut Vec<ReportDiagnostic>,
    line_number: usize,
    line: &str,
) {
    if line.trim().is_empty() {
        return;
    }
    match serde_json::from_str::<TelemetryRecord>(line) {
        Ok(record) => reducer.apply(record),
        Err(error) => diagnostics.push(ReportDiagnostic {
            line: line_number,
            message: error.to_string(),
        }),
    }
}

#[derive(Debug, Default)]
pub struct TraceReducer {
    query: ReportQuery,
    groups: BTreeMap<String, GroupAccumulator>,
}

impl TraceReducer {
    pub fn new(query: ReportQuery) -> Self {
        Self {
            query,
            groups: BTreeMap::new(),
        }
    }

    pub fn apply(&mut self, record: TelemetryRecord) {
        if !matches_query(&record, &self.query) {
            return;
        }
        let work = record.work_key();
        self.groups.entry(work).or_default().apply(&record);
    }

    pub fn finish(self, diagnostics: Vec<ReportDiagnostic>) -> TraceReport {
        let groups = self
            .groups
            .into_iter()
            .map(|(work, accumulator)| accumulator.finish(work))
            .collect();
        TraceReport {
            groups,
            diagnostics,
        }
    }
}

#[derive(Debug, Default)]
struct GroupAccumulator {
    harnesses: BTreeSet<String>,
    sessions: BTreeSet<String>,
    runs: HashSet<RunId>,
    records: u64,
    run_metrics: HashMap<RunId, RunAccumulator>,
}

impl GroupAccumulator {
    fn apply(&mut self, record: &TelemetryRecord) {
        self.records = self.records.saturating_add(1);
        self.harnesses.insert(record.harness.to_string());
        if let Some(session_id) = &record.session_id {
            self.sessions.insert(session_id.to_string());
        }
        self.runs.insert(record.run_id);
        self.run_metrics
            .entry(record.run_id)
            .or_default()
            .apply(&record.payload);
    }

    fn finish(self, work: String) -> TraceGroup {
        let mut agent_time_ms = 0u64;
        let mut has_agent_time = false;
        let mut tokens = TokenAccumulator::default();
        for run in self.run_metrics.into_values() {
            if let Some(wall_time_ms) = run.wall_time_ms {
                agent_time_ms = agent_time_ms.saturating_add(wall_time_ms);
                has_agent_time = true;
            }
            tokens.add_usage(&run.token_usage);
        }

        TraceGroup {
            work,
            harnesses: self.harnesses.into_iter().collect(),
            sessions: self.sessions.into_iter().collect(),
            runs: self.runs.len() as u64,
            agent_time_ms: has_agent_time.then_some(agent_time_ms),
            tokens: tokens.finish(),
            records: self.records,
        }
    }
}

#[derive(Debug, Default)]
struct RunAccumulator {
    token_usage: TokenUsage,
    wall_time_ms: Option<u64>,
}

impl RunAccumulator {
    fn apply(&mut self, payload: &EventPayload) {
        match payload {
            EventPayload::UsageReported(usage) => self.token_usage.add_assign(usage),
            EventPayload::RunCompleted(completed) => {
                self.apply_terminal_metrics(&completed.metrics)
            }
            EventPayload::RunFailed(failed) => self.apply_terminal_metrics(&failed.metrics),
            _ => {}
        }
    }

    fn apply_terminal_metrics(&mut self, metrics: &RunMetrics) {
        self.token_usage = metrics.token_usage.clone();
        self.wall_time_ms = Some(metrics.wall_time_ms);
    }
}

#[derive(Debug, Default)]
struct TokenAccumulator {
    input: Option<u64>,
    output: Option<u64>,
    cache: Option<u64>,
}

impl TokenAccumulator {
    fn add_usage(&mut self, usage: &TokenUsage) {
        add_opt(&mut self.input, usage.input_tokens);
        add_opt(&mut self.output, usage.output_tokens);
        let cache = sum_present([usage.cached_input_tokens, usage.cache_write_tokens]);
        add_opt(&mut self.cache, cache);
    }

    fn finish(self) -> TokenTotals {
        let total = match (self.input, self.output, self.cache) {
            (Some(input), Some(output), Some(cache)) => {
                Some(input.saturating_add(output).saturating_add(cache))
            }
            _ => None,
        };
        TokenTotals {
            input: self.input,
            output: self.output,
            cache: self.cache,
            total,
        }
    }
}

pub fn render_terminal(report: &TraceReport) -> String {
    if report.groups.is_empty() {
        return diagnostics_suffix(
            "SVDO Trace\n────────────────────────────\n\nNo telemetry found.",
            &report.diagnostics,
        );
    }

    let mut output = String::from("SVDO Trace\n────────────────────────────\n");
    for (index, group) in report.groups.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str(&format!(
            "\nWork\n  {}\n\nHarness\n  {}\n\nSession\n  {}\n\nRuns\n  {}\n\nAgent Time\n  {}\n\nTokens\n  Input   {}\n  Output  {}\n  Cache   {}\n  Total   {}\n",
            group.work,
            display_list(&group.harnesses),
            display_list(&group.sessions),
            group.runs,
            display_duration(group.agent_time_ms),
            display_tokens(group.tokens.input),
            display_tokens(group.tokens.output),
            display_tokens(group.tokens.cache),
            display_tokens(group.tokens.total),
        ));
    }
    diagnostics_suffix(output.trim_end(), &report.diagnostics)
}

pub fn render_csv(report: &TraceReport) -> String {
    let mut output = String::from(
        "work,harnesses,sessions,runs,agent_time_ms,input_tokens,output_tokens,cache_tokens,total_tokens,records\n",
    );
    for group in &report.groups {
        output.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            csv_escape(&group.work),
            csv_escape(&group.harnesses.join(";")),
            csv_escape(&group.sessions.join(";")),
            group.runs,
            group
                .agent_time_ms
                .map_or_else(String::new, |value| value.to_string()),
            group
                .tokens
                .input
                .map_or_else(String::new, |value| value.to_string()),
            group
                .tokens
                .output
                .map_or_else(String::new, |value| value.to_string()),
            group
                .tokens
                .cache
                .map_or_else(String::new, |value| value.to_string()),
            group
                .tokens
                .total
                .map_or_else(String::new, |value| value.to_string()),
            group.records,
        ));
    }
    output
}

pub fn render_json(report: &TraceReport) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report)
}

fn matches_query(record: &TelemetryRecord, query: &ReportQuery) -> bool {
    if let Some(work) = &query.work
        && record.work_key() != *work
    {
        return false;
    }
    if let Some(label) = &query.label
        && record.label.as_ref() != Some(label)
    {
        return false;
    }
    if let Some(since) = query.since
        && record.occurred_at < since
    {
        return false;
    }
    true
}

fn add_opt(target: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        *target = Some(target.unwrap_or(0).saturating_add(value));
    }
}

fn sum_present(values: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    let mut total = 0u64;
    let mut present = false;
    for value in values.into_iter().flatten() {
        total = total.saturating_add(value);
        present = true;
    }
    present.then_some(total)
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "Unavailable".to_owned()
    } else {
        values.join(", ")
    }
}

fn display_tokens(value: Option<u64>) -> String {
    value.map_or_else(|| "Unavailable".to_owned(), format_number)
}

fn display_duration(value: Option<u64>) -> String {
    let Some(milliseconds) = value else {
        return "Unavailable".to_owned();
    };
    let total_seconds = milliseconds / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}

fn format_number(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::new();
    for (index, character) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            output.push(',');
        }
        output.push(character);
    }
    output.chars().rev().collect()
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

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SINGLE_WORK: &str = include_str!("../../../tests/fixtures/report/single_work.jsonl");
    const GROUPED_WITH_UNKNOWN_AND_MALFORMED: &str =
        include_str!("../../../tests/fixtures/report/grouped_with_unknown_and_malformed.jsonl");
    const MISSING_TOKEN_COMPONENTS: &str =
        include_str!("../../../tests/fixtures/report/missing_token_components.jsonl");

    const RUN_1: &str = "018f6f1b-97f1-7c04-9a96-111111111111";
    const RUN_2: &str = "018f6f1b-97f1-7c04-9a96-222222222222";
    const EVENT_1: &str = "018f6f1b-97f1-7c04-9a96-aaaaaaaaaaaa";
    const EVENT_2: &str = "018f6f1b-97f1-7c04-9a96-bbbbbbbbbbbb";
    const EVENT_3: &str = "018f6f1b-97f1-7c04-9a96-cccccccccccc";

    #[test]
    fn reports_single_work_item_token_breakdown() {
        let report = report_from_jsonl_lines(
            SINGLE_WORK.lines(),
            &ReportQuery {
                work: Some("ENG-142".to_owned()),
                ..ReportQuery::default()
            },
        );

        assert_eq!(report.groups.len(), 1);
        let group = &report.groups[0];
        assert_eq!(group.work, "ENG-142");
        assert_eq!(group.runs, 2);
        assert_eq!(group.agent_time_ms, Some(1_122_000));
        assert_eq!(group.tokens.input, Some(120_000));
        assert_eq!(group.tokens.output, Some(42_000));
        assert_eq!(group.tokens.cache, Some(32_213));
        assert_eq!(group.tokens.total, Some(194_213));

        let terminal = render_terminal(&report);
        assert!(terminal.contains("SVDO Trace"));
        assert!(terminal.contains("ENG-142"));
        assert!(terminal.contains("Input   120,000"));
        assert!(terminal.contains("Total   194,213"));
    }

    #[test]
    fn groups_by_work_and_uses_unknown_for_missing_ticket() {
        let report = report_from_jsonl_lines(
            GROUPED_WITH_UNKNOWN_AND_MALFORMED.lines(),
            &ReportQuery::default(),
        );

        let works = report
            .groups
            .iter()
            .map(|group| group.work.as_str())
            .collect::<Vec<_>>();
        assert_eq!(works, vec!["ENG-142", "ENG-143", "Unknown"]);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].line, 2);
    }

    #[test]
    fn filters_by_label_and_since() {
        let report = report_from_jsonl_lines(
            [
                terminal_event_at(
                    EVENT_1,
                    RUN_1,
                    Some("ENG-1"),
                    Some("plan"),
                    "2026-08-10T00:00:00Z",
                    1_000,
                    1,
                    2,
                    3,
                ),
                terminal_event_at(
                    EVENT_2,
                    RUN_2,
                    Some("ENG-2"),
                    Some("build"),
                    "2026-08-20T00:00:00Z",
                    1_000,
                    4,
                    5,
                    6,
                ),
                terminal_event_at(
                    EVENT_3,
                    RUN_2,
                    Some("ENG-3"),
                    Some("plan"),
                    "2026-08-21T00:00:00Z",
                    1_000,
                    7,
                    8,
                    9,
                ),
            ],
            &ReportQuery {
                label: Some("plan".to_owned()),
                since: DateTime::parse_from_rfc3339("2026-08-15T00:00:00Z")
                    .map(|timestamp| timestamp.with_timezone(&Utc))
                    .ok(),
                ..ReportQuery::default()
            },
        );

        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].work, "ENG-3");
    }

    #[test]
    fn missing_token_components_stay_missing() {
        let report =
            report_from_jsonl_lines(MISSING_TOKEN_COMPONENTS.lines(), &ReportQuery::default());

        let tokens = &report.groups[0].tokens;
        assert_eq!(tokens.input, Some(10));
        assert_eq!(tokens.output, None);
        assert_eq!(tokens.cache, None);
        assert_eq!(tokens.total, None);

        let json = render_json(&report).unwrap_or_else(|error| panic!("{error}"));
        assert!(json.contains("\"output\": null"));
        let csv = render_csv(&report);
        assert!(csv.contains("ENG-142,codex,sess-1,1,1000,10,,,"));
    }

    #[test]
    fn malformed_records_become_diagnostics() {
        let report = report_from_jsonl_lines(
            GROUPED_WITH_UNKNOWN_AND_MALFORMED.lines(),
            &ReportQuery::default(),
        );

        assert_eq!(report.groups.len(), 3);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].line, 2);
        assert!(render_terminal(&report).contains("Skipped line 2"));
    }

    #[test]
    fn empty_telemetry_renders_clear_result() {
        let report = report_from_jsonl_lines(std::iter::empty::<&str>(), &ReportQuery::default());

        assert!(report.is_empty());
        assert!(render_terminal(&report).contains("No telemetry found."));
        assert_eq!(
            render_csv(&report),
            "work,harnesses,sessions,runs,agent_time_ms,input_tokens,output_tokens,cache_tokens,total_tokens,records\n"
        );
    }

    fn terminal_event_at(
        event_id: &str,
        run_id: &str,
        ticket_id: Option<&str>,
        label: Option<&str>,
        occurred_at: &str,
        wall_time_ms: u64,
        input: u64,
        output: u64,
        cache: u64,
    ) -> String {
        let mut event = json!({
            "schema_version": 1,
            "event_id": event_id,
            "event_type": "run.completed",
            "occurred_at": occurred_at,
            "observed_at": occurred_at,
            "run_id": run_id,
            "harness": "codex",
            "session_id": "sess-1",
            "payload": {
                "type": "run_completed",
                "data": {
                    "metrics": {
                        "wall_time_ms": wall_time_ms,
                        "active_time_ms": 0,
                        "command_time_ms": 0,
                        "tool_time_ms": 0,
                        "turn_count": 0,
                        "provider_event_count": 0,
                        "commands_executed": 0,
                        "failed_commands": 0,
                        "files_changed": 0,
                        "tool_calls": 0,
                        "errors": 0,
                        "token_usage": {
                            "input_tokens": input,
                            "cached_input_tokens": cache,
                            "output_tokens": output
                        }
                    },
                    "exit_code": 0
                }
            }
        });
        if let Some(ticket_id) = ticket_id {
            event["ticket_id"] = json!(ticket_id);
        }
        if let Some(label) = label {
            event["label"] = json!(label);
        }
        event.to_string()
    }
}
