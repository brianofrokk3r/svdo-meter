use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use meter_core::{EventPayload, RunMetrics};
use serde::{Deserialize, Serialize};

use crate::telemetry::{RunSummary, RunTerminalStatus};
use crate::{ReportDiagnostic, TelemetryInspection, TelemetryRecord};

pub const UNAVAILABLE_MARKER: &str = "—";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComparisonDiscoveryQuery {
    pub work: Option<String>,
    pub harnesses: Vec<String>,
    pub models: Vec<String>,
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonDiscoveryReport {
    pub runs: Vec<ComparableRunSummary>,
    pub diagnostics: Vec<ReportDiagnostic>,
}

impl ComparisonDiscoveryReport {
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparableRunSummary {
    pub run_id: String,
    pub works: Vec<String>,
    pub labels: Vec<String>,
    pub harnesses: Vec<String>,
    pub requested_models: Vec<String>,
    pub resolved_models: Vec<String>,
    pub sessions: Vec<String>,
    pub first_event: Option<DateTime<Utc>>,
    pub last_event: Option<DateTime<Utc>>,
    pub records: u64,
    pub terminal_status: ComparisonMetric<TerminalStatus>,
    pub terminal_exit_code: ComparisonMetric<i32>,
    pub terminal_metrics: ComparableRunMetrics,
    #[serde(default)]
    pub estimated_cost_usd: ComparisonMetric<f64>,
    #[serde(default)]
    pub eval: ComparableEvalMetrics,
    #[serde(default)]
    pub rework_count: ComparisonMetric<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalStatus {
    Completed,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparableRunMetrics {
    pub wall_time_ms: ComparisonMetric<u64>,
    pub active_time_ms: ComparisonMetric<u64>,
    pub command_time_ms: ComparisonMetric<u64>,
    pub tool_time_ms: ComparisonMetric<u64>,
    pub turn_count: ComparisonMetric<u64>,
    pub provider_event_count: ComparisonMetric<u64>,
    pub commands_executed: ComparisonMetric<u64>,
    pub failed_commands: ComparisonMetric<u64>,
    pub files_changed: ComparisonMetric<u64>,
    pub tool_calls: ComparisonMetric<u64>,
    pub errors: ComparisonMetric<u64>,
    pub tokens: ComparableTokenUsage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparableTokenUsage {
    pub input_tokens: ComparisonMetric<u64>,
    pub cached_input_tokens: ComparisonMetric<u64>,
    pub cache_write_tokens: ComparisonMetric<u64>,
    pub output_tokens: ComparisonMetric<u64>,
    pub reasoning_tokens: ComparisonMetric<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ComparableEvalMetrics {
    pub score: ComparisonMetric<f64>,
    pub required_checks_passed: ComparisonMetric<u64>,
    pub required_checks_total: ComparisonMetric<u64>,
    pub violations: ComparisonMetric<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComparisonQuery {
    pub work: Option<String>,
    pub harnesses: Vec<String>,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub work: Option<String>,
    pub aggregate: bool,
    pub groups: Vec<ComparisonGroup>,
    pub diagnostics: Vec<ReportDiagnostic>,
}

impl ComparisonReport {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonGroup {
    pub key: ComparisonGroupKey,
    pub runs: u64,
    pub tasks: u64,
    pub ticket: TicketComparisonMetrics,
    pub aggregate: AggregateComparisonMetrics,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ComparisonGroupKey {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TicketComparisonMetrics {
    pub success_rate: ComparisonMetric<f64>,
    pub agent_time_ms: ComparisonMetric<u64>,
    pub turns: ComparisonMetric<u64>,
    pub commands: ComparisonMetric<u64>,
    pub tool_calls: ComparisonMetric<u64>,
    pub input_tokens: ComparisonMetric<u64>,
    pub output_tokens: ComparisonMetric<u64>,
    pub estimated_cost_usd: ComparisonMetric<f64>,
    pub eval_score: ComparisonMetric<f64>,
    pub required_checks: ComparisonMetric<RequiredCheckSummary>,
    pub violations: ComparisonMetric<u64>,
    pub cost_per_eval_point: ComparisonMetric<f64>,
    pub time_ms_per_eval_point: ComparisonMetric<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AggregateComparisonMetrics {
    pub task_count: u64,
    pub pass_rate: ComparisonMetric<f64>,
    pub median_eval_score: ComparisonMetric<f64>,
    pub median_agent_time_ms: ComparisonMetric<f64>,
    pub median_total_tokens: ComparisonMetric<f64>,
    pub median_rework: ComparisonMetric<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredCheckSummary {
    pub passed: u64,
    pub total: u64,
}

#[derive(Debug, Default)]
pub struct ComparisonReducer {
    query: ComparisonQuery,
    runs: Vec<ComparableRunSummary>,
    diagnostics: Vec<ReportDiagnostic>,
}

impl ComparisonReducer {
    pub fn new(query: ComparisonQuery) -> Self {
        Self {
            query,
            runs: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn apply(&mut self, run: ComparableRunSummary) {
        if matches_comparison_query(&run, &self.query) {
            self.runs.push(run);
        }
    }

    pub fn extend(&mut self, runs: impl IntoIterator<Item = ComparableRunSummary>) {
        for run in runs {
            self.apply(run);
        }
    }

    pub fn add_diagnostics(&mut self, diagnostics: impl IntoIterator<Item = ReportDiagnostic>) {
        self.diagnostics.extend(diagnostics);
    }

    pub fn finish(self) -> ComparisonReport {
        let aggregate = self.query.work.is_none();
        let dimensions = GroupDimensions::for_runs(&self.runs, &self.query, aggregate);
        let mut groups = BTreeMap::<ComparisonGroupKey, ComparisonGroupAccumulator>::new();
        for run in self.runs {
            let key = dimensions.key_for(&run);
            groups.entry(key).or_default().apply(run);
        }
        ComparisonReport {
            work: self.query.work,
            aggregate,
            groups: groups
                .into_iter()
                .map(|(key, accumulator)| accumulator.finish(key))
                .collect(),
            diagnostics: self.diagnostics,
        }
    }

    pub fn reduce(
        runs: impl IntoIterator<Item = ComparableRunSummary>,
        query: ComparisonQuery,
    ) -> ComparisonReport {
        let mut reducer = Self::new(query);
        reducer.extend(runs);
        reducer.finish()
    }
}

pub fn discover_comparable_runs_from_jsonl_lines<I, S>(
    lines: I,
    query: &ComparisonDiscoveryQuery,
) -> ComparisonDiscoveryReport
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let inspection = TelemetryInspection::from_jsonl_lines(lines);
    let runs = inspection
        .run_summaries()
        .into_iter()
        .map(ComparableRunSummary::from_run_summary)
        .filter(|run| matches_discovery_query(run, query))
        .collect();
    ComparisonDiscoveryReport {
        runs,
        diagnostics: inspection.diagnostics,
    }
}

pub fn apply_comparison_jsonl_line(
    reducer: &mut ComparisonDiscoveryReducer,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GroupDimensions {
    harness: bool,
    model: bool,
}

impl GroupDimensions {
    fn for_runs(runs: &[ComparableRunSummary], query: &ComparisonQuery, aggregate: bool) -> Self {
        let mut harnesses = BTreeSet::new();
        let mut models = BTreeSet::new();
        let mut models_by_harness = BTreeMap::<String, BTreeSet<String>>::new();
        let mut harnesses_by_model = BTreeMap::<String, BTreeSet<String>>::new();

        for run in runs {
            for harness in &run.harnesses {
                harnesses.insert(harness.clone());
                for model in comparison_models(run) {
                    models_by_harness
                        .entry(harness.clone())
                        .or_default()
                        .insert(model.clone());
                    harnesses_by_model
                        .entry(model)
                        .or_default()
                        .insert(harness.clone());
                }
            }
            models.extend(comparison_models(run));
        }

        let compare_harness = query.harnesses.len() > 1 || harnesses.len() > 1;
        if aggregate && query.models.is_empty() {
            return Self {
                harness: compare_harness || !models.is_empty(),
                model: false,
            };
        }
        let compare_model = query.models.len() > 1
            || (!compare_harness && models.len() > 1)
            || models_by_harness.values().any(|models| models.len() > 1);
        let needs_both = compare_harness
            && compare_model
            && (models_by_harness.values().any(|models| models.len() > 1)
                || harnesses_by_model
                    .values()
                    .any(|harnesses| harnesses.len() > 1));

        Self {
            harness: compare_harness || !compare_model || needs_both,
            model: (!compare_harness && compare_model) || needs_both,
        }
    }

    fn key_for(&self, run: &ComparableRunSummary) -> ComparisonGroupKey {
        ComparisonGroupKey {
            harness: self.harness.then(|| primary_harness(run)),
            model: self.model.then(|| primary_model(run)),
        }
    }
}

#[derive(Debug, Default)]
struct ComparisonGroupAccumulator {
    runs: Vec<ComparableRunSummary>,
}

impl ComparisonGroupAccumulator {
    fn apply(&mut self, run: ComparableRunSummary) {
        self.runs.push(run);
    }

    fn finish(self, key: ComparisonGroupKey) -> ComparisonGroup {
        let tasks = task_count(&self.runs);
        ComparisonGroup {
            key,
            runs: self.runs.len() as u64,
            tasks,
            ticket: TicketComparisonMetrics {
                success_rate: success_rate(self.runs.iter()),
                agent_time_ms: sum_u64(
                    self.runs
                        .iter()
                        .map(|run| run.terminal_metrics.wall_time_ms),
                ),
                turns: sum_u64(self.runs.iter().map(|run| run.terminal_metrics.turn_count)),
                commands: sum_u64(
                    self.runs
                        .iter()
                        .map(|run| run.terminal_metrics.commands_executed),
                ),
                tool_calls: sum_u64(self.runs.iter().map(|run| run.terminal_metrics.tool_calls)),
                input_tokens: sum_u64(
                    self.runs
                        .iter()
                        .map(|run| run.terminal_metrics.tokens.input_tokens),
                ),
                output_tokens: sum_u64(
                    self.runs
                        .iter()
                        .map(|run| run.terminal_metrics.tokens.output_tokens),
                ),
                estimated_cost_usd: sum_f64(self.runs.iter().map(|run| run.estimated_cost_usd)),
                eval_score: mean_f64(self.runs.iter().map(|run| run.eval.score)),
                required_checks: required_checks(&self.runs),
                violations: sum_u64(self.runs.iter().map(|run| run.eval.violations)),
                cost_per_eval_point: ratio(
                    sum_f64(self.runs.iter().map(|run| run.estimated_cost_usd)),
                    mean_f64(self.runs.iter().map(|run| run.eval.score)),
                ),
                time_ms_per_eval_point: ratio(
                    sum_u64(
                        self.runs
                            .iter()
                            .map(|run| run.terminal_metrics.wall_time_ms),
                    )
                    .map(|value| value as f64),
                    mean_f64(self.runs.iter().map(|run| run.eval.score)),
                ),
            },
            aggregate: AggregateComparisonMetrics {
                task_count: tasks,
                pass_rate: task_pass_rate(&self.runs),
                median_eval_score: median_f64(self.runs.iter().map(|run| run.eval.score)),
                median_agent_time_ms: median_u64(
                    self.runs
                        .iter()
                        .map(|run| run.terminal_metrics.wall_time_ms),
                ),
                median_total_tokens: median_u64(self.runs.iter().map(total_tokens)),
                median_rework: median_u64(self.runs.iter().map(|run| run.rework_count)),
            },
        }
    }
}

#[derive(Debug, Default)]
pub struct ComparisonDiscoveryReducer {
    query: ComparisonDiscoveryQuery,
    runs: BTreeMap<String, ComparableRunAccumulator>,
}

impl ComparisonDiscoveryReducer {
    pub fn new(query: ComparisonDiscoveryQuery) -> Self {
        Self {
            query,
            runs: BTreeMap::new(),
        }
    }

    pub fn apply(&mut self, record: TelemetryRecord) {
        let run_id = record.run_id.to_string();
        self.runs
            .entry(run_id.clone())
            .or_insert_with_key(|run_id| ComparableRunAccumulator::new(run_id.clone()))
            .apply(record);
    }

    pub fn finish(self, diagnostics: Vec<ReportDiagnostic>) -> ComparisonDiscoveryReport {
        let runs = self
            .runs
            .into_values()
            .map(ComparableRunAccumulator::finish)
            .filter(|run| matches_discovery_query(run, &self.query))
            .collect();
        ComparisonDiscoveryReport { runs, diagnostics }
    }
}

impl ComparableRunSummary {
    pub fn from_run_summary(summary: RunSummary) -> Self {
        let terminal_status = match summary.terminal_status {
            Some(RunTerminalStatus::Completed) => {
                ComparisonMetric::Observed(TerminalStatus::Completed)
            }
            Some(RunTerminalStatus::Failed) => ComparisonMetric::Observed(TerminalStatus::Failed),
            None => ComparisonMetric::Unavailable,
        };
        let terminal_metrics = summary
            .terminal_metrics
            .as_ref()
            .map(|metrics| {
                ComparableRunMetrics::from_metrics_with_observability(
                    metrics,
                    &summary.harnesses,
                    summary.observed_command_events,
                )
            })
            .unwrap_or_default();

        Self {
            run_id: summary.run_id,
            works: summary.works,
            labels: summary.labels,
            harnesses: summary.harnesses,
            requested_models: summary.requested_models,
            resolved_models: summary.resolved_models,
            sessions: summary.sessions,
            first_event: summary.first_event,
            last_event: summary.last_event,
            records: summary.records,
            terminal_status,
            terminal_exit_code: ComparisonMetric::from_option(summary.terminal_exit_code),
            terminal_metrics,
            estimated_cost_usd: ComparisonMetric::Unavailable,
            eval: ComparableEvalMetrics::default(),
            rework_count: ComparisonMetric::Unavailable,
        }
    }
}

#[derive(Debug)]
struct ComparableRunAccumulator {
    run_id: String,
    works: BTreeSet<String>,
    labels: BTreeSet<String>,
    harnesses: BTreeSet<String>,
    requested_models: BTreeSet<String>,
    resolved_models: BTreeSet<String>,
    sessions: BTreeSet<String>,
    first_event: Option<DateTime<Utc>>,
    last_event: Option<DateTime<Utc>>,
    records: u64,
    terminal: Option<TerminalObservation>,
}

impl ComparableRunAccumulator {
    fn new(run_id: String) -> Self {
        Self {
            run_id,
            works: BTreeSet::new(),
            labels: BTreeSet::new(),
            harnesses: BTreeSet::new(),
            requested_models: BTreeSet::new(),
            resolved_models: BTreeSet::new(),
            sessions: BTreeSet::new(),
            first_event: None,
            last_event: None,
            records: 0,
            terminal: None,
        }
    }

    fn apply(&mut self, record: TelemetryRecord) {
        self.records = self.records.saturating_add(1);
        if let Some(ticket_id) = &record.ticket_id {
            self.works.insert(ticket_id.to_string());
        }
        if let Some(label) = &record.label {
            self.labels.insert(label.clone());
        }
        self.harnesses.insert(record.harness.to_string());
        if let Some(model) = &record.requested_model {
            self.requested_models.insert(model.to_string());
        }
        if let Some(model) = &record.resolved_model {
            self.resolved_models.insert(model.to_string());
        }
        if let Some(session_id) = &record.session_id {
            self.sessions.insert(session_id.to_string());
        }
        self.first_event = min_time(self.first_event, record.occurred_at);
        self.last_event = max_time(self.last_event, record.occurred_at);
        self.apply_terminal(&record);
    }

    fn apply_terminal(&mut self, record: &TelemetryRecord) {
        let next = match &record.payload {
            EventPayload::RunCompleted(completed) => Some(TerminalObservation {
                occurred_at: record.occurred_at,
                status: TerminalStatus::Completed,
                exit_code: ComparisonMetric::from_option(completed.exit_code),
                metrics: ComparableRunMetrics::from_metrics(&completed.metrics),
            }),
            EventPayload::RunFailed(failed) => Some(TerminalObservation {
                occurred_at: record.occurred_at,
                status: TerminalStatus::Failed,
                exit_code: ComparisonMetric::from_option(failed.exit_code),
                metrics: ComparableRunMetrics::from_metrics(&failed.metrics),
            }),
            _ => None,
        };
        let Some(next) = next else {
            return;
        };
        if self
            .terminal
            .as_ref()
            .is_none_or(|current| next.occurred_at >= current.occurred_at)
        {
            self.terminal = Some(next);
        }
    }

    fn finish(self) -> ComparableRunSummary {
        let terminal = self.terminal;
        ComparableRunSummary {
            run_id: self.run_id,
            works: self.works.into_iter().collect(),
            labels: self.labels.into_iter().collect(),
            harnesses: self.harnesses.into_iter().collect(),
            requested_models: self.requested_models.into_iter().collect(),
            resolved_models: self.resolved_models.into_iter().collect(),
            sessions: self.sessions.into_iter().collect(),
            first_event: self.first_event,
            last_event: self.last_event,
            records: self.records,
            terminal_status: terminal
                .as_ref()
                .map(|terminal| terminal.status)
                .map_or(ComparisonMetric::Unavailable, ComparisonMetric::Observed),
            terminal_exit_code: terminal
                .as_ref()
                .map(|terminal| terminal.exit_code)
                .unwrap_or(ComparisonMetric::Unavailable),
            terminal_metrics: terminal
                .map(|terminal| terminal.metrics)
                .unwrap_or_default(),
            estimated_cost_usd: ComparisonMetric::Unavailable,
            eval: ComparableEvalMetrics::default(),
            rework_count: ComparisonMetric::Unavailable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalObservation {
    occurred_at: DateTime<Utc>,
    status: TerminalStatus,
    exit_code: ComparisonMetric<i32>,
    metrics: ComparableRunMetrics,
}

impl ComparableRunMetrics {
    fn from_metrics(metrics: &RunMetrics) -> Self {
        Self::from_metrics_with_observability(metrics, &[], metrics.commands_executed)
    }

    fn from_metrics_with_observability(
        metrics: &RunMetrics,
        harnesses: &[String],
        observed_command_events: u64,
    ) -> Self {
        let commands_executed = if harnesses.iter().any(|harness| harness == "opencode")
            && observed_command_events == 0
        {
            ComparisonMetric::Unavailable
        } else {
            ComparisonMetric::Observed(metrics.commands_executed)
        };
        Self {
            wall_time_ms: ComparisonMetric::Observed(metrics.wall_time_ms),
            active_time_ms: ComparisonMetric::Observed(metrics.active_time_ms),
            command_time_ms: ComparisonMetric::Observed(metrics.command_time_ms),
            tool_time_ms: ComparisonMetric::Observed(metrics.tool_time_ms),
            turn_count: ComparisonMetric::Observed(metrics.turn_count),
            provider_event_count: ComparisonMetric::Observed(metrics.provider_event_count),
            commands_executed,
            failed_commands: ComparisonMetric::Observed(metrics.failed_commands),
            files_changed: ComparisonMetric::Observed(metrics.files_changed),
            tool_calls: ComparisonMetric::Observed(metrics.tool_calls),
            errors: ComparisonMetric::Observed(metrics.errors),
            tokens: ComparableTokenUsage::from_metrics(metrics),
        }
    }
}

impl ComparableTokenUsage {
    fn from_metrics(metrics: &RunMetrics) -> Self {
        Self {
            input_tokens: ComparisonMetric::from_option(metrics.token_usage.input_tokens),
            cached_input_tokens: ComparisonMetric::from_option(
                metrics.token_usage.cached_input_tokens,
            ),
            cache_write_tokens: ComparisonMetric::from_option(
                metrics.token_usage.cache_write_tokens,
            ),
            output_tokens: ComparisonMetric::from_option(metrics.token_usage.output_tokens),
            reasoning_tokens: ComparisonMetric::from_option(metrics.token_usage.reasoning_tokens),
        }
    }
}

pub fn render_comparison_discovery(report: &ComparisonDiscoveryReport) -> String {
    let mut output = String::from("SVDO Comparison Discovery\n─────────────────────────\n");
    if report.runs.is_empty() {
        output.push_str("\nNo comparable runs found.");
        return diagnostics_suffix(output.trim_end(), &report.diagnostics);
    }

    for run in &report.runs {
        output.push_str(&format!(
            "\nRun\n  {}\n  Work        {}\n  Harness     {}\n  Model       {}\n  Session     {}\n  Last Event  {}\n  Status      {}\n  Records     {}\n  Turns       {}\n  Commands    {}\n  Tool Calls  {}\n  Input       {}\n  Output      {}\n",
            run.run_id,
            display_list(&run.works),
            display_list(&run.harnesses),
            display_list(&display_models(run)),
            display_list(&run.sessions),
            display_time(run.last_event),
            display_terminal_status(&run.terminal_status),
            run.records,
            render_count_metric(&run.terminal_metrics.turn_count),
            render_count_metric(&run.terminal_metrics.commands_executed),
            render_count_metric(&run.terminal_metrics.tool_calls),
            render_count_metric(&run.terminal_metrics.tokens.input_tokens),
            render_count_metric(&run.terminal_metrics.tokens.output_tokens),
        ));
    }
    diagnostics_suffix(output.trim_end(), &report.diagnostics)
}

pub fn render_comparison_terminal(report: &ComparisonReport) -> String {
    let title = match &report.work {
        Some(work) => format!("SVDO Comparison — {work}"),
        None => "SVDO Comparison".to_owned(),
    };
    let mut output = format!("{title}\n{}\n", "─".repeat(56));
    if report.groups.is_empty() {
        output.push_str("\nNo comparison data found.");
        return diagnostics_suffix(output.trim_end(), &report.diagnostics);
    }

    let mut table = ComparisonTable::new(report.groups.iter().map(group_heading).collect());
    if report.aggregate {
        add_aggregate_rows(&mut table, &report.groups);
    } else {
        add_ticket_rows(&mut table, &report.groups);
    }

    output.push('\n');
    output.push_str(&table.render());
    diagnostics_suffix(output.trim_end(), &report.diagnostics)
}

fn add_ticket_rows(table: &mut ComparisonTable, groups: &[ComparisonGroup]) {
    if groups.iter().any(|group| group.key.harness.is_some()) {
        table.add_row(
            "Harness",
            groups
                .iter()
                .map(|group| {
                    group
                        .key
                        .harness
                        .as_deref()
                        .map(format_identity)
                        .unwrap_or_else(|| UNAVAILABLE_MARKER.to_owned())
                })
                .collect(),
        );
    }
    if groups.iter().any(|group| group.key.model.is_some()) {
        table.add_row(
            "Model",
            groups
                .iter()
                .map(|group| {
                    group
                        .key
                        .model
                        .as_deref()
                        .map(format_model)
                        .unwrap_or_else(|| UNAVAILABLE_MARKER.to_owned())
                })
                .collect(),
        );
    }
    table.add_row(
        "Runs",
        groups.iter().map(|group| group.runs.to_string()).collect(),
    );

    add_section_rows(
        table,
        "Performance",
        groups,
        [
            metric_row("Success rate", |group| {
                format_percent_metric(&group.ticket.success_rate)
            }),
            metric_row("Agent time", |group| {
                format_duration_metric(&group.ticket.agent_time_ms)
            }),
            metric_row("Turns", |group| render_count_metric(&group.ticket.turns)),
            metric_row("Commands", |group| {
                render_count_metric(&group.ticket.commands)
            }),
            metric_row("Tool calls", |group| {
                render_count_metric(&group.ticket.tool_calls)
            }),
        ],
    );
    add_section_rows(
        table,
        "Cost",
        groups,
        [
            metric_row("Input tokens", |group| {
                format_compact_count_metric(&group.ticket.input_tokens)
            }),
            metric_row("Output tokens", |group| {
                format_compact_count_metric(&group.ticket.output_tokens)
            }),
            metric_row("Est. cost", |group| {
                format_cost_metric(&group.ticket.estimated_cost_usd)
            }),
        ],
    );
    add_section_rows(
        table,
        "Quality",
        groups,
        [
            metric_row("Eval score", |group| {
                format_score_metric(&group.ticket.eval_score)
            }),
            metric_row("Required checks", |group| {
                format_required_checks_metric(&group.ticket.required_checks)
            }),
            metric_row("Violations", |group| {
                render_count_metric(&group.ticket.violations)
            }),
        ],
    );
    add_section_rows(
        table,
        "Efficiency",
        groups,
        [
            metric_row("Cost / eval pt", |group| {
                format_cost_metric(&group.ticket.cost_per_eval_point)
            }),
            metric_row("Time / eval pt", |group| {
                format_minutes_metric(&group.ticket.time_ms_per_eval_point)
            }),
        ],
    );
}

fn add_aggregate_rows(table: &mut ComparisonTable, groups: &[ComparisonGroup]) {
    table.add_row(
        "Tasks",
        groups
            .iter()
            .map(|group| group.aggregate.task_count.to_string())
            .collect(),
    );
    table.add_row(
        "Pass rate",
        groups
            .iter()
            .map(|group| format_percent_metric(&group.aggregate.pass_rate))
            .collect(),
    );
    table.add_row(
        "Median eval",
        groups
            .iter()
            .map(|group| format_score_metric(&group.aggregate.median_eval_score))
            .collect(),
    );
    table.add_row(
        "Median time",
        groups
            .iter()
            .map(|group| format_rounded_minutes_metric(&group.aggregate.median_agent_time_ms))
            .collect(),
    );
    table.add_row(
        "Median tokens",
        groups
            .iter()
            .map(|group| format_compact_float_metric(&group.aggregate.median_total_tokens))
            .collect(),
    );
    table.add_row(
        "Median rework",
        groups
            .iter()
            .map(|group| format_integer_float_metric(&group.aggregate.median_rework))
            .collect(),
    );
}

fn add_section_rows<const N: usize>(
    table: &mut ComparisonTable,
    section: &str,
    groups: &[ComparisonGroup],
    rows: [MetricRow; N],
) {
    let mut rendered_rows = Vec::new();
    for row in rows {
        let cells = groups
            .iter()
            .map(|group| (row.render)(group))
            .collect::<Vec<_>>();
        if cells.iter().any(|cell| cell != UNAVAILABLE_MARKER) {
            rendered_rows.push((row.label, cells));
        }
    }
    if rendered_rows.is_empty() {
        return;
    }

    table.add_section(section);
    for (label, cells) in rendered_rows {
        table.add_row(&format!("  {label}"), cells);
    }
}

struct MetricRow {
    label: &'static str,
    render: fn(&ComparisonGroup) -> String,
}

fn metric_row(label: &'static str, render: fn(&ComparisonGroup) -> String) -> MetricRow {
    MetricRow { label, render }
}

#[derive(Debug)]
enum ComparisonTableRow {
    Data { label: String, cells: Vec<String> },
    Section(String),
}

#[derive(Debug)]
struct ComparisonTable {
    headings: Vec<String>,
    rows: Vec<ComparisonTableRow>,
}

impl ComparisonTable {
    fn new(headings: Vec<String>) -> Self {
        Self {
            headings,
            rows: Vec::new(),
        }
    }

    fn add_row(&mut self, label: &str, cells: Vec<String>) {
        self.rows.push(ComparisonTableRow::Data {
            label: label.to_owned(),
            cells,
        });
    }

    fn add_section(&mut self, section: &str) {
        self.rows
            .push(ComparisonTableRow::Section(section.to_owned()));
    }

    fn render(&self) -> String {
        let label_width = self
            .rows
            .iter()
            .filter_map(|row| match row {
                ComparisonTableRow::Data { label, .. } => Some(label.chars().count()),
                ComparisonTableRow::Section(_) => None,
            })
            .max()
            .unwrap_or(0);
        let column_widths = self.column_widths();
        let mut output = String::new();

        output.push_str(&pad_cell("", label_width));
        if !self.headings.is_empty() {
            output.push_str("  ");
            output.push_str(&self.render_cells(&self.headings, &column_widths));
        }

        for row in &self.rows {
            match row {
                ComparisonTableRow::Section(section) => {
                    output.push_str("\n\n");
                    output.push_str(section);
                }
                ComparisonTableRow::Data { label, cells } => {
                    output.push('\n');
                    output.push_str(&pad_cell(label, label_width));
                    output.push_str("  ");
                    output.push_str(&self.render_cells(cells, &column_widths));
                }
            }
        }

        output
    }

    fn column_widths(&self) -> Vec<usize> {
        let mut widths = self
            .headings
            .iter()
            .map(|heading| heading.chars().count())
            .collect::<Vec<_>>();
        for row in &self.rows {
            let ComparisonTableRow::Data { cells, .. } = row else {
                continue;
            };
            for (index, cell) in cells.iter().enumerate() {
                if index >= widths.len() {
                    widths.push(0);
                }
                widths[index] = widths[index].max(cell.chars().count());
            }
        }
        widths
    }

    fn render_cells(&self, cells: &[String], widths: &[usize]) -> String {
        cells
            .iter()
            .enumerate()
            .map(|(index, cell)| pad_cell(cell, widths[index]))
            .collect::<Vec<_>>()
            .join("  ")
    }
}

fn pad_cell(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(value.chars().count());
    format!("{value}{}", " ".repeat(padding))
}

fn group_heading(group: &ComparisonGroup) -> String {
    match (&group.key.harness, &group.key.model) {
        (Some(harness), Some(model)) => {
            format!("{} / {}", format_identity(harness), format_model(model))
        }
        (Some(harness), None) => format_identity(harness),
        (None, Some(model)) => format_model(model),
        (None, None) => "All".to_owned(),
    }
}

fn format_identity(value: &str) -> String {
    match value {
        "codex" => "Codex".to_owned(),
        "claude" => "Claude".to_owned(),
        "opencode" => "OpenCode".to_owned(),
        "gemini" => "Gemini".to_owned(),
        other => other.to_owned(),
    }
}

fn format_model(value: &str) -> String {
    model_basename(value).to_owned()
}

fn format_percent_metric(value: &ComparisonMetric<f64>) -> String {
    render_metric(value, |value| format!("{:.0}%", value * 100.0))
}

fn format_duration_metric(value: &ComparisonMetric<u64>) -> String {
    render_metric(value, |value| format_duration_ms(*value))
}

fn format_minutes_metric(value: &ComparisonMetric<f64>) -> String {
    render_metric(value, |value| format!("{:.1}m", value / 60_000.0))
}

fn format_rounded_minutes_metric(value: &ComparisonMetric<f64>) -> String {
    render_metric(value, |value| format!("{:.0}m", value / 60_000.0))
}

fn format_compact_count_metric(value: &ComparisonMetric<u64>) -> String {
    render_metric(value, |value| format_compact_number(*value as f64))
}

fn format_compact_float_metric(value: &ComparisonMetric<f64>) -> String {
    render_metric(value, |value| format_compact_number(*value))
}

fn format_integer_float_metric(value: &ComparisonMetric<f64>) -> String {
    render_metric(value, |value| {
        if value.fract() == 0.0 {
            format!("{value:.0}")
        } else {
            format!("{value:.1}")
        }
    })
}

fn format_cost_metric(value: &ComparisonMetric<f64>) -> String {
    render_metric(value, |value| format!("${value:.2}"))
}

fn format_score_metric(value: &ComparisonMetric<f64>) -> String {
    render_metric(value, |value| {
        let formatted = format!("{value:.2}");
        formatted.strip_prefix('0').unwrap_or(&formatted).to_owned()
    })
}

fn format_required_checks_metric(value: &ComparisonMetric<RequiredCheckSummary>) -> String {
    render_metric(value, |value| format!("{}/{}", value.passed, value.total))
}

fn format_duration_ms(milliseconds: u64) -> String {
    let total_seconds = milliseconds / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}

fn format_compact_number(value: f64) -> String {
    if value.abs() >= 1_000_000.0 {
        trim_trailing_decimal(format!("{:.1}m", value / 1_000_000.0))
    } else if value.abs() >= 1_000.0 {
        trim_trailing_decimal(format!("{:.1}k", value / 1_000.0))
    } else if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn trim_trailing_decimal(value: String) -> String {
    value.replace(".0k", "k").replace(".0m", "m")
}

fn matches_discovery_query(run: &ComparableRunSummary, query: &ComparisonDiscoveryQuery) -> bool {
    if let Some(work) = &query.work
        && !run.works.iter().any(|candidate| candidate == work)
    {
        return false;
    }
    if !query.harnesses.is_empty()
        && !run
            .harnesses
            .iter()
            .any(|candidate| query.harnesses.iter().any(|filter| filter == candidate))
    {
        return false;
    }
    if !query.models.is_empty()
        && !run
            .requested_models
            .iter()
            .chain(&run.resolved_models)
            .any(|candidate| {
                query
                    .models
                    .iter()
                    .any(|filter| model_matches(candidate, filter))
            })
    {
        return false;
    }
    if let Some(since) = query.since
        && !run.last_event.is_some_and(|last_event| last_event >= since)
    {
        return false;
    }
    true
}

fn matches_comparison_query(run: &ComparableRunSummary, query: &ComparisonQuery) -> bool {
    if let Some(work) = &query.work
        && !run.works.iter().any(|candidate| candidate == work)
    {
        return false;
    }
    if !query.harnesses.is_empty()
        && !run
            .harnesses
            .iter()
            .any(|candidate| query.harnesses.iter().any(|filter| filter == candidate))
    {
        return false;
    }
    if !query.models.is_empty()
        && !comparison_models(run).iter().any(|candidate| {
            query
                .models
                .iter()
                .any(|filter| model_matches(candidate, filter))
        })
    {
        return false;
    }
    true
}

fn primary_harness(run: &ComparableRunSummary) -> String {
    run.harnesses
        .first()
        .cloned()
        .unwrap_or_else(|| "Unknown".to_owned())
}

fn primary_model(run: &ComparableRunSummary) -> String {
    comparison_models(run)
        .into_iter()
        .next()
        .unwrap_or_else(|| "Unknown".to_owned())
}

fn comparison_models(run: &ComparableRunSummary) -> Vec<String> {
    run.resolved_models
        .iter()
        .chain(&run.requested_models)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn model_matches(candidate: &str, filter: &str) -> bool {
    candidate == filter
        || model_basename(candidate) == filter
        || candidate == model_basename(filter)
}

fn model_basename(value: &str) -> &str {
    value
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(value)
}

fn success_rate<'a>(
    runs: impl IntoIterator<Item = &'a ComparableRunSummary>,
) -> ComparisonMetric<f64> {
    let mut successes = 0u64;
    let mut observed = 0u64;
    for run in runs {
        match run.terminal_status {
            ComparisonMetric::Observed(TerminalStatus::Completed) => {
                successes = successes.saturating_add(1);
                observed = observed.saturating_add(1);
            }
            ComparisonMetric::Observed(TerminalStatus::Failed) => {
                observed = observed.saturating_add(1);
            }
            ComparisonMetric::Unavailable => {}
        }
    }
    rate(successes, observed)
}

fn task_pass_rate(runs: &[ComparableRunSummary]) -> ComparisonMetric<f64> {
    let mut tasks = BTreeMap::<String, TaskOutcomeAccumulator>::new();
    for run in runs {
        for work in &run.works {
            tasks
                .entry(work.clone())
                .or_default()
                .apply(run.terminal_status);
        }
    }

    let mut passed = 0u64;
    let mut observed = 0u64;
    for task in tasks.into_values() {
        if let Some(task_passed) = task.finish() {
            observed = observed.saturating_add(1);
            if task_passed {
                passed = passed.saturating_add(1);
            }
        }
    }

    if observed > 0 {
        rate(passed, observed)
    } else {
        success_rate(runs.iter())
    }
}

#[derive(Debug, Default)]
struct TaskOutcomeAccumulator {
    completed: bool,
    failed: bool,
}

impl TaskOutcomeAccumulator {
    fn apply(&mut self, status: ComparisonMetric<TerminalStatus>) {
        match status {
            ComparisonMetric::Observed(TerminalStatus::Completed) => self.completed = true,
            ComparisonMetric::Observed(TerminalStatus::Failed) => self.failed = true,
            ComparisonMetric::Unavailable => {}
        }
    }

    fn finish(self) -> Option<bool> {
        if self.completed || self.failed {
            Some(self.completed && !self.failed)
        } else {
            None
        }
    }
}

fn task_count(runs: &[ComparableRunSummary]) -> u64 {
    runs.iter()
        .flat_map(|run| run.works.iter().cloned())
        .collect::<BTreeSet<_>>()
        .len() as u64
}

fn required_checks(runs: &[ComparableRunSummary]) -> ComparisonMetric<RequiredCheckSummary> {
    let mut passed = 0u64;
    let mut total = 0u64;
    let mut observed = false;
    for run in runs {
        let (Some(run_passed), Some(run_total)) = (
            run.eval.required_checks_passed.as_observed(),
            run.eval.required_checks_total.as_observed(),
        ) else {
            continue;
        };
        passed = passed.saturating_add(*run_passed);
        total = total.saturating_add(*run_total);
        observed = true;
    }
    if observed {
        ComparisonMetric::Observed(RequiredCheckSummary { passed, total })
    } else {
        ComparisonMetric::Unavailable
    }
}

fn total_tokens(run: &ComparableRunSummary) -> ComparisonMetric<u64> {
    sum_u64([
        run.terminal_metrics.tokens.input_tokens,
        run.terminal_metrics.tokens.cached_input_tokens,
        run.terminal_metrics.tokens.cache_write_tokens,
        run.terminal_metrics.tokens.output_tokens,
        run.terminal_metrics.tokens.reasoning_tokens,
    ])
}

fn rate(numerator: u64, denominator: u64) -> ComparisonMetric<f64> {
    if denominator == 0 {
        ComparisonMetric::Unavailable
    } else {
        ComparisonMetric::Observed(numerator as f64 / denominator as f64)
    }
}

fn ratio(
    numerator: ComparisonMetric<f64>,
    denominator: ComparisonMetric<f64>,
) -> ComparisonMetric<f64> {
    match (numerator, denominator) {
        (ComparisonMetric::Observed(numerator), ComparisonMetric::Observed(denominator))
            if denominator > 0.0 && denominator.is_finite() && numerator.is_finite() =>
        {
            ComparisonMetric::Observed(numerator / denominator)
        }
        _ => ComparisonMetric::Unavailable,
    }
}

fn min_time(current: Option<DateTime<Utc>>, next: DateTime<Utc>) -> Option<DateTime<Utc>> {
    Some(current.map_or(next, |current| current.min(next)))
}

fn max_time(current: Option<DateTime<Utc>>, next: DateTime<Utc>) -> Option<DateTime<Utc>> {
    Some(current.map_or(next, |current| current.max(next)))
}

fn display_models(run: &ComparableRunSummary) -> Vec<String> {
    run.requested_models
        .iter()
        .chain(&run.resolved_models)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn display_terminal_status(value: &ComparisonMetric<TerminalStatus>) -> String {
    render_metric(value, |status| match status {
        TerminalStatus::Completed => "completed".to_owned(),
        TerminalStatus::Failed => "failed".to_owned(),
    })
}

fn display_time(value: Option<DateTime<Utc>>) -> String {
    value.map_or_else(|| UNAVAILABLE_MARKER.to_owned(), |value| value.to_rfc3339())
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        UNAVAILABLE_MARKER.to_owned()
    } else {
        values.join(", ")
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case")]
pub enum ComparisonMetric<T> {
    Observed(T),
    Unavailable,
}

impl<T> ComparisonMetric<T> {
    pub fn observed(value: T) -> Self {
        Self::Observed(value)
    }

    pub fn unavailable() -> Self {
        Self::Unavailable
    }

    pub fn from_option(value: Option<T>) -> Self {
        value.map_or(Self::Unavailable, Self::Observed)
    }

    pub fn as_observed(&self) -> Option<&T> {
        match self {
            Self::Observed(value) => Some(value),
            Self::Unavailable => None,
        }
    }

    pub fn into_observed(self) -> Option<T> {
        match self {
            Self::Observed(value) => Some(value),
            Self::Unavailable => None,
        }
    }

    pub fn is_observed(&self) -> bool {
        matches!(self, Self::Observed(_))
    }

    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable)
    }

    pub fn map<U>(self, map_value: impl FnOnce(T) -> U) -> ComparisonMetric<U> {
        match self {
            Self::Observed(value) => ComparisonMetric::Observed(map_value(value)),
            Self::Unavailable => ComparisonMetric::Unavailable,
        }
    }
}

impl<T> From<Option<T>> for ComparisonMetric<T> {
    fn from(value: Option<T>) -> Self {
        Self::from_option(value)
    }
}

impl<T> Default for ComparisonMetric<T> {
    fn default() -> Self {
        Self::Unavailable
    }
}

pub fn sum_u64(values: impl IntoIterator<Item = ComparisonMetric<u64>>) -> ComparisonMetric<u64> {
    let mut sum = 0u64;
    let mut observed = false;
    for value in observed_values(values) {
        sum = sum.saturating_add(value);
        observed = true;
    }
    if observed {
        ComparisonMetric::Observed(sum)
    } else {
        ComparisonMetric::Unavailable
    }
}

pub fn sum_f64(values: impl IntoIterator<Item = ComparisonMetric<f64>>) -> ComparisonMetric<f64> {
    let mut sum = 0.0;
    let mut observed = false;
    for value in observed_values(values) {
        sum += value;
        observed = true;
    }
    if observed {
        ComparisonMetric::Observed(sum)
    } else {
        ComparisonMetric::Unavailable
    }
}

pub fn mean_u64(values: impl IntoIterator<Item = ComparisonMetric<u64>>) -> ComparisonMetric<f64> {
    let mut sum = 0u64;
    let mut count = 0u64;
    for value in observed_values(values) {
        sum = sum.saturating_add(value);
        count = count.saturating_add(1);
    }
    if count == 0 {
        ComparisonMetric::Unavailable
    } else {
        ComparisonMetric::Observed(sum as f64 / count as f64)
    }
}

pub fn mean_f64(values: impl IntoIterator<Item = ComparisonMetric<f64>>) -> ComparisonMetric<f64> {
    let mut sum = 0.0;
    let mut count = 0u64;
    for value in observed_values(values) {
        sum += value;
        count = count.saturating_add(1);
    }
    if count == 0 {
        ComparisonMetric::Unavailable
    } else {
        ComparisonMetric::Observed(sum / count as f64)
    }
}

pub fn median_u64(
    values: impl IntoIterator<Item = ComparisonMetric<u64>>,
) -> ComparisonMetric<f64> {
    median_f64(
        values
            .into_iter()
            .map(|value| value.map(|value| value as f64)),
    )
}

pub fn median_f64(
    values: impl IntoIterator<Item = ComparisonMetric<f64>>,
) -> ComparisonMetric<f64> {
    let mut observed = observed_values(values).collect::<Vec<_>>();
    if observed.is_empty() {
        return ComparisonMetric::Unavailable;
    }
    observed.sort_by(f64::total_cmp);
    let midpoint = observed.len() / 2;
    if observed.len() % 2 == 0 {
        ComparisonMetric::Observed((observed[midpoint - 1] + observed[midpoint]) / 2.0)
    } else {
        ComparisonMetric::Observed(observed[midpoint])
    }
}

pub fn render_metric<T>(
    value: &ComparisonMetric<T>,
    render_observed: impl FnOnce(&T) -> String,
) -> String {
    match value {
        ComparisonMetric::Observed(value) => render_observed(value),
        ComparisonMetric::Unavailable => UNAVAILABLE_MARKER.to_owned(),
    }
}

pub fn render_count_metric(value: &ComparisonMetric<u64>) -> String {
    render_metric(value, |value| value.to_string())
}

pub fn render_decimal_metric(value: &ComparisonMetric<f64>, precision: usize) -> String {
    render_metric(value, |value| format!("{value:.precision$}"))
}

fn observed_values<T>(
    values: impl IntoIterator<Item = ComparisonMetric<T>>,
) -> impl Iterator<Item = T> {
    values
        .into_iter()
        .filter_map(ComparisonMetric::into_observed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPARABLE_RUNS: &str =
        include_str!("../../../tests/fixtures/comparison/comparable_runs.jsonl");

    #[test]
    fn comparison_metric_represents_observed_nonzero_zero_and_unavailable() {
        let nonzero = ComparisonMetric::observed(17u64);
        let zero = ComparisonMetric::observed(0u64);
        let unavailable = ComparisonMetric::<u64>::unavailable();

        assert_eq!(nonzero.as_observed(), Some(&17));
        assert_eq!(zero.as_observed(), Some(&0));
        assert!(unavailable.as_observed().is_none());
        assert!(nonzero.is_observed());
        assert!(zero.is_observed());
        assert!(unavailable.is_unavailable());
    }

    #[test]
    fn from_option_preserves_zero_as_observed() {
        assert_eq!(
            ComparisonMetric::from_option(Some(0u64)),
            ComparisonMetric::Observed(0)
        );
        assert_eq!(
            ComparisonMetric::from_option(None::<u64>),
            ComparisonMetric::Unavailable
        );
    }

    #[test]
    fn aggregation_skips_unavailable_without_coercing_to_zero() {
        let values = [
            ComparisonMetric::observed(10u64),
            ComparisonMetric::unavailable(),
            ComparisonMetric::observed(20u64),
        ];

        assert_eq!(sum_u64(values), ComparisonMetric::Observed(30));
        assert_eq!(
            mean_u64(values),
            ComparisonMetric::Observed(15.0),
            "unavailable values must not contribute to the divisor"
        );
    }

    #[test]
    fn aggregation_includes_observed_zero_values() {
        let values = [
            ComparisonMetric::observed(0u64),
            ComparisonMetric::unavailable(),
            ComparisonMetric::observed(10u64),
        ];

        assert_eq!(sum_u64(values), ComparisonMetric::Observed(10));
        assert_eq!(mean_u64(values), ComparisonMetric::Observed(5.0));
        assert_eq!(median_u64(values), ComparisonMetric::Observed(5.0));
    }

    #[test]
    fn all_unavailable_aggregation_stays_unavailable() {
        let values = [
            ComparisonMetric::<u64>::unavailable(),
            ComparisonMetric::<u64>::unavailable(),
        ];

        assert_eq!(sum_u64(values), ComparisonMetric::Unavailable);
        assert_eq!(mean_u64(values), ComparisonMetric::Unavailable);
        assert_eq!(median_u64(values), ComparisonMetric::Unavailable);
    }

    #[test]
    fn rendering_displays_unavailable_marker_and_zero_value() {
        assert_eq!(
            render_count_metric(&ComparisonMetric::unavailable()),
            UNAVAILABLE_MARKER
        );
        assert_eq!(render_count_metric(&ComparisonMetric::observed(0)), "0");
        assert_eq!(
            render_decimal_metric(&ComparisonMetric::observed(0.0), 2),
            "0.00"
        );
    }

    #[test]
    fn median_f64_skips_unavailable_values() {
        let values = [
            ComparisonMetric::observed(0.94),
            ComparisonMetric::unavailable(),
            ComparisonMetric::observed(0.88),
            ComparisonMetric::observed(0.91),
        ];

        assert_eq!(median_f64(values), ComparisonMetric::Observed(0.91));
    }

    #[test]
    fn comparison_reducer_groups_ticket_runs_by_harness_and_aggregates_metrics() {
        let report = ComparisonReducer::reduce(
            [
                comparable_run("codex-a", "ENG-142", "codex", "openai/gpt-5.6")
                    .with_completed()
                    .with_wall_time(700_000)
                    .with_turns(10)
                    .with_tool_calls(20)
                    .with_input_tokens(100_000)
                    .with_output_tokens(20_000)
                    .with_cost(1.0)
                    .with_eval(0.8, 3, 4, 1)
                    .build(),
                comparable_run("codex-b", "ENG-142", "codex", "openai/gpt-5.6")
                    .with_completed()
                    .with_wall_time(300_000)
                    .with_turns(8)
                    .with_tool_calls(11)
                    .with_input_tokens(82_000)
                    .with_output_tokens(11_000)
                    .with_cost(0.42)
                    .with_eval(1.0, 5, 4, 0)
                    .build(),
                comparable_run("claude-a", "ENG-142", "claude", "anthropic/claude-sonnet-5")
                    .with_failed()
                    .with_wall_time(642_000)
                    .with_turns(14)
                    .with_tool_calls(27)
                    .with_input_tokens(145_000)
                    .with_output_tokens(28_000)
                    .with_eval(0.5, 2, 4, 3)
                    .build(),
            ],
            ComparisonQuery {
                work: Some("ENG-142".to_owned()),
                ..ComparisonQuery::default()
            },
        );

        assert_eq!(report.groups.len(), 2);
        assert_eq!(report.groups[0].key.harness.as_deref(), Some("claude"));
        assert_eq!(report.groups[1].key.harness.as_deref(), Some("codex"));

        let codex = &report.groups[1];
        assert_eq!(codex.runs, 2);
        assert_eq!(codex.ticket.success_rate, ComparisonMetric::Observed(1.0));
        assert_eq!(
            codex.ticket.agent_time_ms,
            ComparisonMetric::Observed(1_000_000)
        );
        assert_eq!(codex.ticket.turns, ComparisonMetric::Observed(18));
        assert_eq!(codex.ticket.tool_calls, ComparisonMetric::Observed(31));
        assert_eq!(
            codex.ticket.input_tokens,
            ComparisonMetric::Observed(182_000)
        );
        assert_eq!(
            codex.ticket.output_tokens,
            ComparisonMetric::Observed(31_000)
        );
        assert_eq!(
            codex.ticket.required_checks,
            ComparisonMetric::Observed(RequiredCheckSummary {
                passed: 8,
                total: 8
            })
        );
        assert_eq!(codex.ticket.violations, ComparisonMetric::Observed(1));
        assert_eq!(codex.ticket.eval_score, ComparisonMetric::Observed(0.9));
        assert_eq!(
            codex.ticket.cost_per_eval_point,
            ComparisonMetric::Observed(1.42 / 0.9)
        );
    }

    #[test]
    fn comparison_reducer_groups_models_within_one_harness_and_applies_filters() {
        let report = ComparisonReducer::reduce(
            [
                comparable_run("openai", "ENG-142", "opencode", "openai/gpt-5.6")
                    .with_completed()
                    .build(),
                comparable_run(
                    "anthropic",
                    "ENG-142",
                    "opencode",
                    "anthropic/claude-sonnet-5",
                )
                .with_completed()
                .build(),
                comparable_run("claude", "ENG-142", "claude", "anthropic/claude-sonnet-5")
                    .with_completed()
                    .build(),
            ],
            ComparisonQuery {
                harnesses: vec!["opencode".to_owned()],
                models: vec![
                    "openai/gpt-5.6".to_owned(),
                    "anthropic/claude-sonnet-5".to_owned(),
                ],
                ..ComparisonQuery::default()
            },
        );

        assert_eq!(
            report
                .groups
                .iter()
                .map(|group| (&group.key.harness, &group.key.model))
                .collect::<Vec<_>>(),
            vec![
                (&None, &Some("anthropic/claude-sonnet-5".to_owned())),
                (&None, &Some("openai/gpt-5.6".to_owned())),
            ]
        );
        assert!(report.groups.iter().all(|group| group.runs == 1));
    }

    #[test]
    fn comparison_reducer_computes_aggregate_task_rates_medians_and_rework() {
        let report = ComparisonReducer::reduce(
            [
                comparable_run("codex-1", "ENG-1", "codex", "openai/gpt-5.6")
                    .with_completed()
                    .with_wall_time(12_000)
                    .with_input_tokens(100)
                    .with_output_tokens(50)
                    .with_eval(0.8, 1, 1, 0)
                    .with_rework(0)
                    .build(),
                comparable_run("codex-2", "ENG-2", "codex", "openai/gpt-5.6")
                    .with_failed()
                    .with_wall_time(20_000)
                    .with_input_tokens(300)
                    .with_output_tokens(100)
                    .with_eval(1.0, 1, 1, 0)
                    .with_rework(2)
                    .build(),
                comparable_run("codex-3", "ENG-3", "codex", "openai/gpt-5.6")
                    .with_completed()
                    .with_wall_time(10_000)
                    .with_input_tokens(50)
                    .with_output_tokens(50)
                    .with_eval(0.6, 0, 1, 1)
                    .with_rework(1)
                    .build(),
                comparable_run("claude-1", "ENG-1", "claude", "anthropic/claude-sonnet-5")
                    .with_completed()
                    .with_wall_time(8_000)
                    .with_input_tokens(75)
                    .with_output_tokens(25)
                    .with_eval(0.9, 1, 1, 0)
                    .with_rework(0)
                    .build(),
            ],
            ComparisonQuery::default(),
        );

        let codex = report
            .groups
            .iter()
            .find(|group| group.key.harness.as_deref() == Some("codex"))
            .expect("codex group");
        assert!(report.aggregate);
        assert_eq!(codex.aggregate.task_count, 3);
        assert_eq!(
            codex.aggregate.pass_rate,
            ComparisonMetric::Observed(2.0 / 3.0)
        );
        assert_eq!(
            codex.aggregate.median_eval_score,
            ComparisonMetric::Observed(0.8)
        );
        assert_eq!(
            codex.aggregate.median_agent_time_ms,
            ComparisonMetric::Observed(12_000.0)
        );
        assert_eq!(
            codex.aggregate.median_total_tokens,
            ComparisonMetric::Observed(150.0)
        );
        assert_eq!(
            codex.aggregate.median_rework,
            ComparisonMetric::Observed(1.0)
        );
    }

    #[test]
    fn comparison_reducer_skips_unavailable_but_retains_observed_zeroes() {
        let report = ComparisonReducer::reduce(
            [
                comparable_run("zeroes", "ENG-142", "codex", "openai/gpt-5.6")
                    .with_completed()
                    .with_commands(0)
                    .with_tool_calls(0)
                    .with_wall_time(0)
                    .with_cost(0.0)
                    .with_eval(0.0, 0, 0, 0)
                    .build(),
                comparable_run("missing", "ENG-142", "codex", "openai/gpt-5.6").build(),
            ],
            ComparisonQuery {
                work: Some("ENG-142".to_owned()),
                ..ComparisonQuery::default()
            },
        );

        let group = &report.groups[0];
        assert_eq!(group.ticket.success_rate, ComparisonMetric::Observed(1.0));
        assert_eq!(group.ticket.commands, ComparisonMetric::Observed(0));
        assert_eq!(group.ticket.tool_calls, ComparisonMetric::Observed(0));
        assert_eq!(group.ticket.agent_time_ms, ComparisonMetric::Observed(0));
        assert_eq!(
            group.ticket.cost_per_eval_point,
            ComparisonMetric::Unavailable
        );
        assert_eq!(
            group.ticket.time_ms_per_eval_point,
            ComparisonMetric::Unavailable
        );
        assert_eq!(
            render_count_metric(&group.ticket.commands),
            "0",
            "observed zero must render distinctly from unavailable"
        );
    }

    #[test]
    fn renders_ticket_specific_comparison_report() {
        let report = ComparisonReducer::reduce(
            [
                comparable_run("codex-a", "ENG-142", "codex", "openai/gpt-5.6")
                    .with_completed()
                    .with_wall_time(738_000)
                    .with_turns(18)
                    .with_commands(17)
                    .with_tool_calls(31)
                    .with_input_tokens(182_000)
                    .with_output_tokens(31_000)
                    .with_cost(1.42)
                    .with_eval(0.94, 8, 8, 1)
                    .build(),
                comparable_run("claude-a", "ENG-142", "claude", "anthropic/claude-sonnet-5")
                    .with_completed()
                    .with_wall_time(642_000)
                    .with_turns(14)
                    .with_commands(14)
                    .with_tool_calls(27)
                    .with_input_tokens(145_000)
                    .with_output_tokens(28_000)
                    .with_cost(1.19)
                    .with_eval(0.97, 8, 8, 0)
                    .build(),
            ],
            ComparisonQuery {
                work: Some("ENG-142".to_owned()),
                ..ComparisonQuery::default()
            },
        );

        let output = render_comparison_terminal(&report);

        assert!(output.starts_with("SVDO Comparison — ENG-142"));
        assert!(output.lines().any(|line| line.contains("Claude")
            && line.contains("Codex")
            && !line.contains("Harness")));
        assert_row_values(&output, "Harness", &["Claude", "Codex"]);
        assert_row_values(&output, "Runs", &["1", "1"]);
        assert!(output.contains("Performance"));
        assert_row_values(&output, "Success rate", &["100%", "100%"]);
        assert_row_values(&output, "Agent time", &["10m", "42s", "12m", "18s"]);
        assert!(output.contains("Cost"));
        assert_row_values(&output, "Input tokens", &["145k", "182k"]);
        assert!(output.contains("Quality"));
        assert_row_values(&output, "Required checks", &["8/8", "8/8"]);
        assert!(output.contains("Efficiency"));
        assert_row_values(&output, "Cost / eval pt", &["$1.23", "$1.51"]);
    }

    #[test]
    fn renders_aggregate_comparison_report() {
        let report = ComparisonReducer::reduce(
            [
                comparable_run("codex-1", "ENG-1", "codex", "openai/gpt-5.6")
                    .with_completed()
                    .with_wall_time(840_000)
                    .with_input_tokens(100_000)
                    .with_output_tokens(84_000)
                    .with_eval(0.91, 1, 1, 0)
                    .with_rework(1)
                    .build(),
                comparable_run("codex-2", "ENG-2", "codex", "openai/gpt-5.6")
                    .with_failed()
                    .with_wall_time(960_000)
                    .with_input_tokens(120_000)
                    .with_output_tokens(83_000)
                    .with_eval(0.88, 1, 1, 0)
                    .with_rework(1)
                    .build(),
                comparable_run("claude-1", "ENG-1", "claude", "anthropic/claude-sonnet-5")
                    .with_completed()
                    .with_wall_time(720_000)
                    .with_input_tokens(100_000)
                    .with_output_tokens(51_000)
                    .with_eval(0.94, 1, 1, 0)
                    .with_rework(0)
                    .build(),
            ],
            ComparisonQuery::default(),
        );

        let output = render_comparison_terminal(&report);

        assert!(output.starts_with("SVDO Comparison\n"));
        assert!(output.contains("               Claude  Codex"));
        assert!(output.contains("Tasks          1       2"));
        assert!(output.contains("Pass rate      100%    50%"));
        assert!(output.contains("Median eval    .94     .90"));
        assert!(output.contains("Median time    12m     15m"));
        assert!(output.contains("Median tokens  151k    193.5k"));
        assert!(output.contains("Median rework  0       1"));
    }

    #[test]
    fn renderer_distinguishes_unavailable_from_observed_zero() {
        let report = ComparisonReducer::reduce(
            [
                comparable_run("missing", "ENG-142", "claude", "anthropic/claude-sonnet-5")
                    .with_completed()
                    .build(),
                comparable_run("zeroes", "ENG-142", "codex", "openai/gpt-5.6")
                    .with_completed()
                    .with_commands(0)
                    .with_tool_calls(0)
                    .with_wall_time(0)
                    .with_input_tokens(0)
                    .with_output_tokens(0)
                    .with_cost(0.0)
                    .with_eval(0.0, 0, 0, 0)
                    .build(),
            ],
            ComparisonQuery {
                work: Some("ENG-142".to_owned()),
                ..ComparisonQuery::default()
            },
        );

        let output = render_comparison_terminal(&report);

        assert_row_values(&output, "Agent time", &["—", "0s"]);
        assert_row_values(&output, "Commands", &["—", "0"]);
        assert_row_values(&output, "Est. cost", &["—", "$0.00"]);
        assert_row_values(&output, "Eval score", &["—", ".00"]);
    }

    #[test]
    fn renderer_omits_sections_with_no_relevant_rows() {
        let report = ComparisonReducer::reduce(
            [comparable_run("minimal", "ENG-142", "codex", "openai/gpt-5.6").build()],
            ComparisonQuery {
                work: Some("ENG-142".to_owned()),
                ..ComparisonQuery::default()
            },
        );

        let output = render_comparison_terminal(&report);

        assert!(output.contains("SVDO Comparison — ENG-142"));
        assert!(!output.contains("Performance"));
        assert!(!output.contains("Cost"));
        assert!(!output.contains("Quality"));
        assert!(!output.contains("Efficiency"));
    }

    #[test]
    fn discovers_runs_for_work_identifier_with_diagnostics() {
        let report = discover_comparable_runs_from_jsonl_lines(
            COMPARABLE_RUNS.lines(),
            &ComparisonDiscoveryQuery {
                work: Some("ENG-142".to_owned()),
                ..ComparisonDiscoveryQuery::default()
            },
        );

        assert_eq!(report.runs.len(), 3);
        assert_eq!(report.diagnostics.len(), 1);
        assert!(report.runs.iter().all(|run| run.works == vec!["ENG-142"]));
        assert!(report.runs.iter().any(|run| run.harnesses == vec!["codex"]));
        assert!(
            report
                .runs
                .iter()
                .any(|run| run.harnesses == vec!["claude"])
        );
        assert!(
            report
                .runs
                .iter()
                .any(|run| run.harnesses == vec!["opencode"])
        );
    }

    #[test]
    fn filters_aggregate_runs_by_repeated_harness() {
        let report = discover_comparable_runs_from_jsonl_lines(
            COMPARABLE_RUNS.lines(),
            &ComparisonDiscoveryQuery {
                harnesses: vec!["codex".to_owned(), "opencode".to_owned()],
                ..ComparisonDiscoveryQuery::default()
            },
        );

        assert_eq!(report.runs.len(), 3);
        assert!(
            report
                .runs
                .iter()
                .all(|run| run.harnesses != vec!["claude"])
        );
    }

    #[test]
    fn filters_by_repeated_requested_or_resolved_model() {
        let report = discover_comparable_runs_from_jsonl_lines(
            COMPARABLE_RUNS.lines(),
            &ComparisonDiscoveryQuery {
                models: vec![
                    "openai/gpt-5.6".to_owned(),
                    "anthropic/claude-sonnet-5".to_owned(),
                ],
                ..ComparisonDiscoveryQuery::default()
            },
        );

        let run_ids = report
            .runs
            .iter()
            .map(|run| run.run_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            run_ids,
            vec![
                "018f6f1b-97f1-7c04-9a96-111111111111",
                "018f6f1b-97f1-7c04-9a96-222222222222",
                "018f6f1b-97f1-7c04-9a96-333333333333",
                "018f6f1b-97f1-7c04-9a96-444444444444",
            ]
        );
    }

    #[test]
    fn combined_filters_use_run_last_event_for_since_window() {
        let since = DateTime::parse_from_rfc3339("2026-08-01T00:00:00Z")
            .unwrap_or_else(|error| panic!("{error}"))
            .with_timezone(&Utc);
        let report = discover_comparable_runs_from_jsonl_lines(
            COMPARABLE_RUNS.lines(),
            &ComparisonDiscoveryQuery {
                work: Some("ENG-142".to_owned()),
                harnesses: vec!["opencode".to_owned()],
                models: vec!["anthropic/claude-sonnet-5".to_owned()],
                since: Some(since),
            },
        );

        assert_eq!(report.runs.len(), 1);
        let run = &report.runs[0];
        assert_eq!(
            run.run_id,
            "018f6f1b-97f1-7c04-9a96-444444444444".to_owned()
        );
        assert_eq!(
            run.last_event,
            Some(
                DateTime::parse_from_rfc3339("2026-08-23T09:00:00Z")
                    .unwrap_or_else(|error| panic!("{error}"))
                    .with_timezone(&Utc)
            )
        );
    }

    #[test]
    fn unavailable_and_observed_zero_metrics_remain_distinct() {
        let report = discover_comparable_runs_from_jsonl_lines(
            COMPARABLE_RUNS.lines(),
            &ComparisonDiscoveryQuery {
                work: Some("ENG-142".to_owned()),
                ..ComparisonDiscoveryQuery::default()
            },
        );

        let codex = report
            .runs
            .iter()
            .find(|run| run.harnesses == vec!["codex"])
            .expect("codex run");
        assert_eq!(
            codex.terminal_metrics.commands_executed,
            ComparisonMetric::Observed(0)
        );
        assert_eq!(
            codex.terminal_metrics.tokens.cached_input_tokens,
            ComparisonMetric::Observed(0)
        );

        let opencode = report
            .runs
            .iter()
            .find(|run| run.run_id == "018f6f1b-97f1-7c04-9a96-444444444444")
            .expect("unterminated opencode run");
        assert_eq!(
            opencode.terminal_metrics.commands_executed,
            ComparisonMetric::Unavailable
        );
        assert!(render_comparison_discovery(&report).contains("Commands    —"));
    }

    fn assert_row_values(output: &str, label: &str, values: &[&str]) {
        let row = output
            .lines()
            .find(|line| line.contains(label))
            .unwrap_or_else(|| panic!("missing row `{label}` in:\n{output}"));
        for value in values {
            assert!(
                row.split_whitespace().any(|cell| cell == *value),
                "missing cell `{value}` in row `{row}`"
            );
        }
    }

    #[derive(Debug)]
    struct ComparableRunBuilder {
        run: ComparableRunSummary,
    }

    fn comparable_run(
        run_id: &str,
        work: &str,
        harness: &str,
        model: &str,
    ) -> ComparableRunBuilder {
        ComparableRunBuilder {
            run: ComparableRunSummary {
                run_id: run_id.to_owned(),
                works: vec![work.to_owned()],
                labels: Vec::new(),
                harnesses: vec![harness.to_owned()],
                requested_models: Vec::new(),
                resolved_models: vec![model.to_owned()],
                sessions: Vec::new(),
                first_event: None,
                last_event: None,
                records: 1,
                terminal_status: ComparisonMetric::Unavailable,
                terminal_exit_code: ComparisonMetric::Unavailable,
                terminal_metrics: ComparableRunMetrics::default(),
                estimated_cost_usd: ComparisonMetric::Unavailable,
                eval: ComparableEvalMetrics::default(),
                rework_count: ComparisonMetric::Unavailable,
            },
        }
    }

    impl ComparableRunBuilder {
        fn with_completed(mut self) -> Self {
            self.run.terminal_status = ComparisonMetric::Observed(TerminalStatus::Completed);
            self
        }

        fn with_failed(mut self) -> Self {
            self.run.terminal_status = ComparisonMetric::Observed(TerminalStatus::Failed);
            self
        }

        fn with_wall_time(mut self, value: u64) -> Self {
            self.run.terminal_metrics.wall_time_ms = ComparisonMetric::Observed(value);
            self
        }

        fn with_turns(mut self, value: u64) -> Self {
            self.run.terminal_metrics.turn_count = ComparisonMetric::Observed(value);
            self
        }

        fn with_commands(mut self, value: u64) -> Self {
            self.run.terminal_metrics.commands_executed = ComparisonMetric::Observed(value);
            self
        }

        fn with_tool_calls(mut self, value: u64) -> Self {
            self.run.terminal_metrics.tool_calls = ComparisonMetric::Observed(value);
            self
        }

        fn with_input_tokens(mut self, value: u64) -> Self {
            self.run.terminal_metrics.tokens.input_tokens = ComparisonMetric::Observed(value);
            self
        }

        fn with_output_tokens(mut self, value: u64) -> Self {
            self.run.terminal_metrics.tokens.output_tokens = ComparisonMetric::Observed(value);
            self
        }

        fn with_cost(mut self, value: f64) -> Self {
            self.run.estimated_cost_usd = ComparisonMetric::Observed(value);
            self
        }

        fn with_eval(
            mut self,
            score: f64,
            required_passed: u64,
            required_total: u64,
            violations: u64,
        ) -> Self {
            self.run.eval = ComparableEvalMetrics {
                score: ComparisonMetric::Observed(score),
                required_checks_passed: ComparisonMetric::Observed(required_passed),
                required_checks_total: ComparisonMetric::Observed(required_total),
                violations: ComparisonMetric::Observed(violations),
            };
            self
        }

        fn with_rework(mut self, value: u64) -> Self {
            self.run.rework_count = ComparisonMetric::Observed(value);
            self
        }

        fn build(self) -> ComparableRunSummary {
            self.run
        }
    }
}
