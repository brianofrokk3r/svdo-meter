use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use meter_adapters::{claude_argv, codex_argv, opencode_argv};
use meter_core::{
    ClaudeConfig, ClaudeRunOptions, CodexConfig, ExecutionPermissionMode, HarnessKind, ModelName,
    OpenCodeConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cli::{JudgeBackend, ReportFormat};

const MAX_JUDGE_OUTPUT_BYTES: usize = 1024 * 1024;
const DEFAULT_TELEMETRY_MIN_COUNT: u64 = 1;
const MAX_TYPESAFE_SNAPSHOT_BYTES: usize = 96 * 1024;
const MAX_TYPESAFE_SOURCE_SNAPSHOT_BYTES: usize = 64 * 1024;
const TYPESAFE_MAX_ATTEMPTS: u32 = 3;
const TYPESAFE_INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const DEFAULT_TYPESAFE_MODEL: &str = "jev-latest";
const DEFAULT_TYPESAFE_API_KEY_ENV: &str = "TYPESAFE_API_KEY";
const DEFAULT_TYPESAFE_URL: &str = "https://api.typesafe.ai/v1/systemone";

#[derive(Debug, Clone)]
pub struct EvalDefinition {
    pub id: String,
    pub task: String,
    pub judge: Option<EvalJudgeConfig>,
    pub checks: Vec<CheckDefinition>,
    pub threshold: f64,
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct EvalJudgeConfig {
    pub backend: Option<EvalJudgeBackend>,
    pub model: Option<String>,
    pub api_key_env: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalJudgeBackend {
    TypeSafe,
}

#[derive(Debug, Clone)]
pub struct CheckDefinition {
    pub id: String,
    pub check_type: CheckType,
    pub command: Option<String>,
    pub required: bool,
    pub weight: f64,
    pub min_score: Option<f64>,
    pub standard: Option<String>,
    pub rubric: Option<String>,
    pub event_type: Option<String>,
    pub tool_name: Option<String>,
    pub min_count: u64,
}

#[derive(Debug, Clone, Default)]
pub struct JudgeConfig {
    runner: Option<JudgeRunner>,
}

#[derive(Debug)]
pub struct JudgeCliConfig {
    pub harness: Option<HarnessKind>,
    pub model: Option<String>,
    pub command: Option<PathBuf>,
    pub args: Vec<String>,
    pub backend: Option<JudgeBackend>,
    pub typesafe_model: String,
    pub typesafe_api_key_env: String,
    pub typesafe_url: String,
}

#[derive(Debug, Clone)]
enum JudgeRunner {
    Codex(CodexJudge),
    Claude(ClaudeJudge),
    OpenCode(OpenCodeJudge),
    Command(JudgeCommand),
    TypeSafe(TypeSafeJudge),
}

#[derive(Debug, Clone)]
struct CodexJudge {
    model: Option<ModelName>,
}

#[derive(Debug, Clone)]
struct ClaudeJudge {
    model: Option<ModelName>,
}

#[derive(Debug, Clone)]
struct OpenCodeJudge {
    model: Option<ModelName>,
}

#[derive(Debug, Clone)]
struct JudgeCommand {
    program: PathBuf,
    args: Vec<String>,
}

#[derive(Debug, Clone)]
struct TypeSafeJudge {
    endpoint: String,
    model: String,
    api_key_env: String,
}

#[derive(Debug)]
struct JudgeExecution {
    success: bool,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    command: String,
    harness: &'static str,
    model: Option<String>,
}

#[derive(Debug)]
struct StreamedCommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug)]
struct LatestTelemetry {
    run_id: String,
    events: Vec<TelemetryEvent>,
}

#[derive(Debug)]
struct TelemetryEvent {
    run_id: String,
    event_type: String,
    occurred_at: DateTime<Utc>,
    tool_name: Option<String>,
}

#[derive(Debug)]
enum LatestRunTelemetry {
    MissingFiles,
    NoReadableEvents,
    Found(LatestTelemetry),
}

#[derive(Debug)]
struct PreparedJudgeCheck {
    check_id: String,
    standard: Option<String>,
    standard_path: Option<String>,
    standard_contents: Option<String>,
    rubric: Option<PreparedRubric>,
    criteria: Vec<String>,
}

#[derive(Debug, Clone)]
struct PreparedRubric {
    id: String,
    path: String,
    instructions: String,
    criteria: Vec<String>,
}

#[derive(Debug, Serialize)]
struct JudgeRequest<'a> {
    eval_id: &'a str,
    task: &'a str,
    check_id: &'a str,
    standard: Option<&'a str>,
    standard_path: Option<String>,
    standard_contents: Option<String>,
    rubric: Option<&'a str>,
    rubric_path: Option<String>,
    rubric_contents: Option<String>,
    workspace: String,
}

#[derive(Debug, Serialize)]
struct TypeSafeRequest {
    state: TypeSafeState,
    model: String,
    questions: BTreeMap<String, TypeSafeScoreQuestion>,
}

#[derive(Debug, Serialize)]
struct TypeSafeState {
    eval_id: String,
    task: String,
    workspace: String,
    checks: BTreeMap<String, TypeSafeCheckState>,
    workspace_snapshot: TypeSafeWorkspaceSnapshot,
}

#[derive(Debug, Serialize)]
struct TypeSafeCheckState {
    check_id: String,
    standard: Option<String>,
    standard_path: Option<String>,
    standard_contents: Option<String>,
    rubric: Option<TypeSafeRubricState>,
}

#[derive(Debug, Serialize)]
struct TypeSafeRubricState {
    id: String,
    path: String,
    instructions: String,
    criteria: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TypeSafeWorkspaceSnapshot {
    status: Option<String>,
    diff_stat: Option<String>,
    diff: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    source_files: BTreeMap<String, String>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct TypeSafeScoreQuestion {
    #[serde(rename = "type")]
    question_type: &'static str,
    instructions: String,
    criteria: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TypeSafeResponse {
    #[serde(default)]
    model: Option<String>,
    answers: BTreeMap<String, TypeSafeScoreAnswer>,
    #[serde(default)]
    usage: Option<TypeSafeUsage>,
}

#[derive(Debug, Deserialize)]
struct TypeSafeScoreAnswer {
    #[serde(rename = "type")]
    answer_type: String,
    score: f64,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    probabilities: BTreeMap<String, f64>,
    #[serde(default)]
    legend: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSafeScoreMetadata {
    pub provider: String,
    pub question_id: String,
    pub raw_score: f64,
    pub normalized_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub probabilities: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub legend: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub criteria: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TypeSafeUsage {
    #[serde(default, alias = "input")]
    input_tokens: Option<u64>,
    #[serde(default, alias = "output")]
    output_tokens: Option<u64>,
    #[serde(default, alias = "cache_read")]
    cache_read_tokens: Option<u64>,
    #[serde(default)]
    total_tokens: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JudgeResponse {
    score: f64,
    #[serde(default)]
    passed: Option<bool>,
    #[serde(default)]
    violations: Vec<String>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    token_usage: Option<TokenUsage>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    harness: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckType {
    Command,
    Judge,
    Telemetry,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalRunReport {
    pub passed: bool,
    pub duration_ms: u128,
    pub results: Vec<EvalResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalResult {
    pub id: String,
    pub task: String,
    pub source: Option<String>,
    pub overall_score: f64,
    pub threshold: f64,
    pub passed: bool,
    pub required_failure: bool,
    pub duration_ms: u128,
    pub checks: Vec<CheckResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub id: String,
    #[serde(rename = "type")]
    pub check_type: CheckType,
    pub outcome: CheckOutcome,
    pub required: bool,
    pub weight: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    pub duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typesafe: Option<TypeSafeScoreMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckOutcome {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

impl JudgeConfig {
    pub fn from_cli(config: JudgeCliConfig) -> anyhow::Result<Self> {
        if matches!(config.backend, Some(JudgeBackend::TypeSafe)) {
            return Ok(Self {
                runner: Some(JudgeRunner::TypeSafe(TypeSafeJudge::new(
                    config.typesafe_url,
                    config.typesafe_model,
                    config.typesafe_api_key_env,
                )?)),
            });
        }
        if config.command.is_some() {
            return Ok(Self {
                runner: config.command.map(|program| {
                    JudgeRunner::Command(JudgeCommand {
                        program,
                        args: config.args,
                    })
                }),
            });
        }
        let Some(harness) = config.harness else {
            return Ok(Self::default());
        };
        let model = config
            .model
            .map(ModelName::new)
            .transpose()
            .context("invalid eval judge --model value")?;
        match harness {
            HarnessKind::Codex => Ok(Self {
                runner: Some(JudgeRunner::Codex(CodexJudge { model })),
            }),
            HarnessKind::Claude => Ok(Self {
                runner: Some(JudgeRunner::Claude(ClaudeJudge { model })),
            }),
            HarnessKind::OpenCode => Ok(Self {
                runner: Some(JudgeRunner::OpenCode(OpenCodeJudge { model })),
            }),
            HarnessKind::Gemini => {
                bail!("eval judge --harness currently supports codex, claude, and opencode")
            }
        }
    }

    fn for_definition(&self, definition: &EvalDefinition) -> anyhow::Result<Self> {
        if self.runner.is_some() {
            return Ok(self.clone());
        }
        let Some(judge) = &definition.judge else {
            return Ok(Self::default());
        };
        match judge.backend {
            Some(EvalJudgeBackend::TypeSafe) => Ok(Self {
                runner: Some(JudgeRunner::TypeSafe(TypeSafeJudge::new(
                    judge
                        .url
                        .clone()
                        .unwrap_or_else(|| DEFAULT_TYPESAFE_URL.to_owned()),
                    judge
                        .model
                        .clone()
                        .unwrap_or_else(|| DEFAULT_TYPESAFE_MODEL.to_owned()),
                    judge
                        .api_key_env
                        .clone()
                        .unwrap_or_else(|| DEFAULT_TYPESAFE_API_KEY_ENV.to_owned()),
                )?)),
            }),
            None => Ok(Self::default()),
        }
    }
}

pub fn run(
    workspace: &Path,
    requested_eval: Option<&str>,
    judge_config: &JudgeConfig,
) -> anyhow::Result<EvalRunReport> {
    let started = Instant::now();
    let definitions = load_requested_definitions(workspace, requested_eval)?;
    let mut results = Vec::with_capacity(definitions.len());
    for definition in definitions {
        results.push(run_definition(workspace, definition, judge_config)?);
    }
    Ok(EvalRunReport {
        passed: results.iter().all(|result| result.passed),
        duration_ms: elapsed_ms(started.elapsed()),
        results,
    })
}

pub fn load_requested_definitions(
    workspace: &Path,
    requested_eval: Option<&str>,
) -> anyhow::Result<Vec<EvalDefinition>> {
    let eval_dir = workspace.join(".svdo").join("evals");
    let definitions = load_definitions(&eval_dir)?;
    if definitions.is_empty() {
        bail!("no eval definitions found in `{}`", eval_dir.display());
    }
    let Some(requested_eval) = requested_eval else {
        return Ok(definitions);
    };
    let matches = definitions
        .into_iter()
        .filter(|definition| definition.matches(requested_eval))
        .collect::<Vec<_>>();
    match matches.len() {
        0 => bail!(
            "eval `{requested_eval}` was not found in `{}`",
            eval_dir.display()
        ),
        1 => Ok(matches),
        _ => bail!("eval `{requested_eval}` matched more than one definition"),
    }
}

pub fn load_definitions(eval_dir: &Path) -> anyhow::Result<Vec<EvalDefinition>> {
    if !eval_dir.exists() {
        bail!("eval directory `{}` does not exist", eval_dir.display());
    }
    let mut paths = fs::read_dir(eval_dir)
        .with_context(|| format!("failed to read eval directory `{}`", eval_dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read eval directory `{}`", eval_dir.display()))?;
    paths.sort();

    let mut definitions = Vec::new();
    for path in paths.into_iter().filter(|path| is_yaml(path)) {
        let value = fs::read_to_string(&path)
            .with_context(|| format!("failed to read eval definition `{}`", path.display()))?;
        let mut definition = parse_definition(&value)
            .with_context(|| format!("failed to parse eval definition `{}`", path.display()))?;
        validate_definition(&definition, &path)?;
        definition.source_path = Some(path);
        definitions.push(definition);
    }
    Ok(definitions)
}

pub fn run_definition(
    workspace: &Path,
    definition: EvalDefinition,
    judge_config: &JudgeConfig,
) -> anyhow::Result<EvalResult> {
    let started = Instant::now();
    let judge_config = judge_config.for_definition(&definition)?;
    let mut checks = Vec::with_capacity(definition.checks.len());
    if let Some(JudgeRunner::TypeSafe(judge)) = &judge_config.runner {
        let judge_results = match run_typesafe_judge_checks(workspace, &definition, judge) {
            Ok(results) => results,
            Err(error) => typesafe_judge_failure_results(
                &definition,
                elapsed_ms(started.elapsed()),
                format!("TypeSafe judge failed: {error:#}"),
                Some(judge.model.clone()),
            ),
        };
        for check in &definition.checks {
            if check.check_type == CheckType::Judge {
                let result = judge_results
                    .get(&check.id)
                    .with_context(|| {
                        format!("missing TypeSafe result for judge check `{}`", check.id)
                    })?
                    .clone();
                checks.push(result);
            } else {
                checks.push(run_check(workspace, &definition, check, &judge_config)?);
            }
        }
    } else {
        for check in &definition.checks {
            checks.push(run_check(workspace, &definition, check, &judge_config)?);
        }
    }
    Ok(aggregate_result(
        definition,
        checks,
        elapsed_ms(started.elapsed()),
    ))
}

pub fn aggregate_result(
    definition: EvalDefinition,
    checks: Vec<CheckResult>,
    duration_ms: u128,
) -> EvalResult {
    let mut weighted_score = 0.0;
    let mut total_weight = 0.0;
    let mut violations = Vec::new();
    let mut required_failure = false;

    for check in &checks {
        if check.required && check.outcome == CheckOutcome::Failed {
            required_failure = true;
        }
        if check.outcome == CheckOutcome::Failed {
            violations.extend(
                check
                    .violations
                    .iter()
                    .map(|violation| format!("{}: {violation}", check.id)),
            );
        }
        if let Some(score) = check.score {
            weighted_score += score * check.weight;
            total_weight += check.weight;
        }
    }

    let overall_score = if total_weight > 0.0 {
        weighted_score / total_weight
    } else {
        0.0
    };
    let passed = !required_failure && overall_score >= definition.threshold;
    if !passed && overall_score < definition.threshold {
        violations.push(format!(
            "overall score {:.2} is below threshold {:.2}",
            overall_score, definition.threshold
        ));
    }

    EvalResult {
        id: definition.id,
        task: definition.task,
        source: definition
            .source_path
            .as_ref()
            .map(|path| path.display().to_string()),
        overall_score,
        threshold: definition.threshold,
        passed,
        required_failure,
        duration_ms,
        checks,
        violations,
        token_usage: None,
        model: None,
        harness: None,
        session_id: None,
    }
}

pub fn render(report: &EvalRunReport, format: ReportFormat) -> anyhow::Result<String> {
    match format {
        ReportFormat::Terminal => Ok(render_terminal(report)),
        ReportFormat::Json => {
            serde_json::to_string_pretty(report).context("failed to render JSON eval report")
        }
        ReportFormat::Csv => Ok(render_csv(report)),
    }
}

fn run_check(
    workspace: &Path,
    definition: &EvalDefinition,
    check: &CheckDefinition,
    judge_config: &JudgeConfig,
) -> anyhow::Result<CheckResult> {
    match check.check_type {
        CheckType::Command => run_command_check(workspace, check),
        CheckType::Judge => run_judge_check(workspace, definition, check, judge_config),
        CheckType::Telemetry => run_telemetry_check(workspace, check),
    }
}

fn run_command_check(workspace: &Path, check: &CheckDefinition) -> anyhow::Result<CheckResult> {
    let command = check
        .command
        .as_deref()
        .with_context(|| format!("command check `{}` is missing command", check.id))?;
    let started = Instant::now();
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(workspace)
        .output()
        .with_context(|| format!("failed to execute command check `{}`", check.id))?;
    let duration_ms = elapsed_ms(started.elapsed());
    let success = output.status.success();
    let captured = (!success).then(|| failure_output(&output.stdout, &output.stderr));
    let mut violations = Vec::new();
    if !success {
        violations.push(format!(
            "command exited with status {}",
            output.status.code().map_or_else(
                || "terminated by signal".to_owned(),
                |code| code.to_string()
            )
        ));
    }
    Ok(CheckResult {
        id: check.id.clone(),
        check_type: CheckType::Command,
        outcome: if success {
            CheckOutcome::Passed
        } else {
            CheckOutcome::Failed
        },
        required: check.required,
        weight: check.weight,
        score: Some(if success { 1.0 } else { 0.0 }),
        duration_ms,
        command: Some(command.to_owned()),
        exit_code: output.status.code(),
        standard: None,
        standard_path: None,
        violations,
        output: captured,
        token_usage: None,
        model: None,
        harness: Some("command".to_owned()),
        session_id: None,
        typesafe: None,
    })
}

fn run_telemetry_check(workspace: &Path, check: &CheckDefinition) -> anyhow::Result<CheckResult> {
    let started = Instant::now();
    let event_type = check
        .event_type
        .as_deref()
        .with_context(|| format!("telemetry check `{}` is missing event_type", check.id))?;
    let telemetry_dir = workspace.join(".svdo").join("meter");
    let telemetry = load_latest_run_telemetry(&telemetry_dir)?;
    let duration_ms = elapsed_ms(started.elapsed());
    let failure = |violation: String| CheckResult {
        id: check.id.clone(),
        check_type: CheckType::Telemetry,
        outcome: CheckOutcome::Failed,
        required: check.required,
        weight: check.weight,
        score: Some(0.0),
        duration_ms,
        command: None,
        exit_code: None,
        standard: None,
        standard_path: None,
        violations: vec![violation],
        output: None,
        token_usage: None,
        model: None,
        harness: Some("telemetry".to_owned()),
        session_id: None,
        typesafe: None,
    };

    let latest = match telemetry {
        LatestRunTelemetry::MissingFiles => {
            return Ok(failure(format!(
                "telemetry was missing: no telemetry files found in `{}`",
                telemetry_dir.display()
            )));
        }
        LatestRunTelemetry::NoReadableEvents => {
            return Ok(failure(format!(
                "telemetry was missing: no readable telemetry events found in `{}`",
                telemetry_dir.display()
            )));
        }
        LatestRunTelemetry::Found(latest) => latest,
    };

    let matching_count = latest
        .events
        .iter()
        .filter(|event| telemetry_event_matches(event, event_type, check.tool_name.as_deref()))
        .count() as u64;
    let predicate = telemetry_predicate_label(event_type, check.tool_name.as_deref());
    if matching_count == 0 {
        return Ok(failure(format!(
            "no matching telemetry events found in latest run `{}` for {predicate}",
            latest.run_id
        )));
    }
    if matching_count < check.min_count {
        return Ok(failure(format!(
            "matching telemetry events for {predicate} in latest run `{}` were below min_count: found {}, required {}",
            latest.run_id, matching_count, check.min_count
        )));
    }

    Ok(CheckResult {
        id: check.id.clone(),
        check_type: CheckType::Telemetry,
        outcome: CheckOutcome::Passed,
        required: check.required,
        weight: check.weight,
        score: Some(1.0),
        duration_ms,
        command: None,
        exit_code: None,
        standard: None,
        standard_path: None,
        violations: Vec::new(),
        output: Some(format!(
            "found {matching_count} matching telemetry event(s) in latest run `{}`",
            latest.run_id
        )),
        token_usage: None,
        model: None,
        harness: Some("telemetry".to_owned()),
        session_id: None,
        typesafe: None,
    })
}

fn load_latest_run_telemetry(telemetry_dir: &Path) -> anyhow::Result<LatestRunTelemetry> {
    let paths = telemetry_paths(telemetry_dir)?;
    if paths.is_empty() {
        return Ok(LatestRunTelemetry::MissingFiles);
    }

    let mut events = Vec::new();
    for path in paths {
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read telemetry `{}`", path.display()));
            }
        };
        events.extend(contents.lines().filter_map(parse_telemetry_event));
    }
    if events.is_empty() {
        return Ok(LatestRunTelemetry::NoReadableEvents);
    }

    let latest_run_id = events
        .iter()
        .max_by(|left, right| {
            left.occurred_at
                .cmp(&right.occurred_at)
                .then_with(|| left.run_id.cmp(&right.run_id))
        })
        .map(|event| event.run_id.clone())
        .expect("events is not empty");
    let latest_events = events
        .into_iter()
        .filter(|event| event.run_id == latest_run_id)
        .collect();
    Ok(LatestRunTelemetry::Found(LatestTelemetry {
        run_id: latest_run_id,
        events: latest_events,
    }))
}

fn telemetry_paths(telemetry_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(telemetry_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to read telemetry directory `{}`",
                    telemetry_dir.display()
                )
            });
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| {
            format!(
                "failed to read telemetry directory `{}`",
                telemetry_dir.display()
            )
        })?;
        if entry.file_type()?.is_file()
            && entry.path().extension().and_then(OsStr::to_str) == Some("jsonl")
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn parse_telemetry_event(line: &str) -> Option<TelemetryEvent> {
    if line.trim().is_empty() {
        return None;
    }
    let value = serde_json::from_str::<Value>(line).ok()?;
    let run_id = value.get("run_id")?.as_str()?.to_owned();
    let event_type = value.get("event_type")?.as_str()?.to_owned();
    let occurred_at = value
        .get("occurred_at")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<DateTime<Utc>>().ok())?;
    let tool_name = telemetry_tool_name(&value);
    Some(TelemetryEvent {
        run_id,
        event_type,
        occurred_at,
        tool_name,
    })
}

fn telemetry_tool_name(value: &Value) -> Option<String> {
    value
        .pointer("/payload/data/tool_name")
        .or_else(|| value.pointer("/payload/data/name"))
        .or_else(|| value.get("tool_name"))
        .or_else(|| value.get("name"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn telemetry_event_matches(
    event: &TelemetryEvent,
    event_type: &str,
    tool_name: Option<&str>,
) -> bool {
    event.event_type == event_type
        && tool_name.is_none_or(|expected| event.tool_name.as_deref() == Some(expected))
}

fn telemetry_predicate_label(event_type: &str, tool_name: Option<&str>) -> String {
    match tool_name {
        Some(tool_name) => format!("event_type `{event_type}` and tool_name `{tool_name}`"),
        None => format!("event_type `{event_type}`"),
    }
}

fn run_judge_check(
    workspace: &Path,
    definition: &EvalDefinition,
    check: &CheckDefinition,
    judge_config: &JudgeConfig,
) -> anyhow::Result<CheckResult> {
    let started = Instant::now();
    let standard_path = check
        .standard
        .as_deref()
        .map(|standard| resolve_standard(workspace, standard))
        .transpose()?;
    let Some(runner) = &judge_config.runner else {
        return Ok(CheckResult {
            id: check.id.clone(),
            check_type: CheckType::Judge,
            outcome: CheckOutcome::Skipped,
            required: check.required,
            weight: check.weight,
            score: None,
            duration_ms: elapsed_ms(started.elapsed()),
            command: None,
            exit_code: None,
            standard: check.standard.clone(),
            standard_path: standard_path.map(|path| path.display().to_string()),
            violations: vec!["judge checks require a configured judge harness".to_owned()],
            output: None,
            token_usage: None,
            model: None,
            harness: Some("judge-unavailable".to_owned()),
            session_id: None,
            typesafe: None,
        });
    };

    let standard_contents = standard_path
        .as_deref()
        .map(|path| {
            fs::read_to_string(path)
                .with_context(|| format!("failed to read standard `{}`", path.display()))
        })
        .transpose()?;
    let rubric_path = check
        .rubric
        .as_deref()
        .map(|rubric| resolve_rubric(workspace, rubric))
        .transpose()?;
    let rubric_contents = rubric_path
        .as_deref()
        .map(|path| {
            fs::read_to_string(path)
                .with_context(|| format!("failed to read rubric `{}`", path.display()))
        })
        .transpose()?;
    let request = JudgeRequest {
        eval_id: &definition.id,
        task: &definition.task,
        check_id: &check.id,
        standard: check.standard.as_deref(),
        standard_path: standard_path
            .as_ref()
            .map(|path| path.display().to_string()),
        standard_contents,
        rubric: check.rubric.as_deref(),
        rubric_path: rubric_path.as_ref().map(|path| path.display().to_string()),
        rubric_contents,
        workspace: workspace.display().to_string(),
    };
    let execution = runner
        .run(&request, workspace, &check.id)
        .with_context(|| format!("failed to execute judge check `{}`", check.id));
    let execution = execution?;
    let duration_ms = elapsed_ms(started.elapsed());
    if !execution.success {
        return Ok(CheckResult {
            id: check.id.clone(),
            check_type: CheckType::Judge,
            outcome: CheckOutcome::Failed,
            required: check.required,
            weight: check.weight,
            score: Some(0.0),
            duration_ms,
            command: Some(execution.command),
            exit_code: execution.exit_code,
            standard: check.standard.clone(),
            standard_path: request.standard_path.clone(),
            violations: vec![format!(
                "judge exited with status {}",
                execution.exit_code.map_or_else(
                    || "terminated by signal".to_owned(),
                    |code| code.to_string()
                )
            )],
            output: Some(failure_output(&execution.stdout, &execution.stderr)),
            token_usage: None,
            model: execution.model,
            harness: Some(execution.harness.to_owned()),
            session_id: None,
            typesafe: None,
        });
    }
    let stdout = String::from_utf8_lossy(&execution.stdout);
    let response = match parse_judge_response(&stdout, &check.id) {
        Ok(response) if (0.0..=1.0).contains(&response.score) => response,
        Ok(response) => {
            return Ok(invalid_judge_response_result(
                check,
                &request,
                execution,
                duration_ms,
                format!(
                    "judge response for eval `{}` check `{}` emitted score {}; expected 0.0 through 1.0",
                    definition.id, check.id, response.score
                ),
            ));
        }
        Err(error) => {
            return Ok(invalid_judge_response_result(
                check,
                &request,
                execution,
                duration_ms,
                format!(
                    "invalid judge response for eval `{}` check `{}`: {error}",
                    definition.id, check.id
                ),
            ));
        }
    };
    let passed = response.passed.unwrap_or(response.score >= 1.0);
    let mut violations = response.violations;
    if !passed && violations.is_empty() {
        violations.push(format!("judge score {:.2} did not pass", response.score));
    }
    Ok(CheckResult {
        id: check.id.clone(),
        check_type: CheckType::Judge,
        outcome: if passed {
            CheckOutcome::Passed
        } else {
            CheckOutcome::Failed
        },
        required: check.required,
        weight: check.weight,
        score: Some(response.score),
        duration_ms,
        command: Some(execution.command),
        exit_code: execution.exit_code,
        standard: check.standard.clone(),
        standard_path: request.standard_path.clone(),
        violations,
        output: response.output.map(|value| truncate(&value, 4_000)),
        token_usage: response.token_usage,
        model: response.model.or(execution.model),
        harness: response
            .harness
            .or_else(|| Some(execution.harness.to_owned())),
        session_id: response.session_id,
        typesafe: None,
    })
}

fn typesafe_judge_failure_results(
    definition: &EvalDefinition,
    duration_ms: u128,
    reason: String,
    model: Option<String>,
) -> BTreeMap<String, CheckResult> {
    definition
        .checks
        .iter()
        .filter(|check| check.check_type == CheckType::Judge)
        .map(|check| {
            (
                check.id.clone(),
                CheckResult {
                    id: check.id.clone(),
                    check_type: CheckType::Judge,
                    outcome: CheckOutcome::Failed,
                    required: check.required,
                    weight: check.weight,
                    score: Some(0.0),
                    duration_ms,
                    command: Some("typesafe systemone".to_owned()),
                    exit_code: None,
                    standard: check.standard.clone(),
                    standard_path: None,
                    violations: vec![reason.clone()],
                    output: None,
                    token_usage: None,
                    model: model.clone(),
                    harness: Some("typesafe".to_owned()),
                    session_id: None,
                    typesafe: None,
                },
            )
        })
        .collect()
}

fn run_typesafe_judge_checks(
    workspace: &Path,
    definition: &EvalDefinition,
    judge: &TypeSafeJudge,
) -> anyhow::Result<BTreeMap<String, CheckResult>> {
    let started = Instant::now();
    let mut prepared = Vec::new();
    for check in definition
        .checks
        .iter()
        .filter(|check| check.check_type == CheckType::Judge)
    {
        let standard_path = check
            .standard
            .as_deref()
            .map(|standard| resolve_standard(workspace, standard))
            .transpose()?;
        let standard_contents = standard_path
            .as_deref()
            .map(|path| {
                fs::read_to_string(path)
                    .with_context(|| format!("failed to read standard `{}`", path.display()))
            })
            .transpose()?;
        let rubric = check
            .rubric
            .as_deref()
            .map(|rubric| load_typesafe_rubric(workspace, rubric))
            .transpose()?;
        let criteria = rubric
            .as_ref()
            .map(|rubric| rubric.criteria.clone())
            .unwrap_or_else(default_typesafe_criteria);
        prepared.push(PreparedJudgeCheck {
            check_id: check.id.clone(),
            standard: check.standard.clone(),
            standard_path: standard_path.map(|path| path.display().to_string()),
            standard_contents,
            rubric,
            criteria,
        });
    }

    if prepared.is_empty() {
        return Ok(BTreeMap::new());
    }

    let request = build_typesafe_request(workspace, definition, judge, &prepared)?;
    let response = judge.run(&request)?;
    let duration_ms = elapsed_ms(started.elapsed());
    let mut results = BTreeMap::new();

    for check in definition
        .checks
        .iter()
        .filter(|check| check.check_type == CheckType::Judge)
    {
        let prepared_check = prepared
            .iter()
            .find(|prepared| prepared.check_id == check.id)
            .with_context(|| format!("missing prepared TypeSafe check `{}`", check.id))?;
        let answer = response.answers.get(&check.id).with_context(|| {
            format!(
                "TypeSafe response did not include an answer for judge check `{}`",
                check.id
            )
        })?;
        let result = normalize_typesafe_check_result(
            check,
            prepared_check,
            answer,
            response.model.as_deref(),
            response.usage.as_ref(),
            duration_ms,
            &judge.model,
            definition.threshold,
        )?;
        results.insert(check.id.clone(), result);
    }
    Ok(results)
}

fn build_typesafe_request(
    workspace: &Path,
    definition: &EvalDefinition,
    judge: &TypeSafeJudge,
    checks: &[PreparedJudgeCheck],
) -> anyhow::Result<TypeSafeRequest> {
    let mut state_checks = BTreeMap::new();
    let mut questions = BTreeMap::new();
    for check in checks {
        state_checks.insert(
            check.check_id.clone(),
            TypeSafeCheckState {
                check_id: check.check_id.clone(),
                standard: check.standard.clone(),
                standard_path: check.standard_path.clone(),
                standard_contents: check.standard_contents.clone(),
                rubric: check.rubric.as_ref().map(|rubric| TypeSafeRubricState {
                    id: rubric.id.clone(),
                    path: rubric.path.clone(),
                    instructions: rubric.instructions.clone(),
                    criteria: rubric.criteria.clone(),
                }),
            },
        );
        let instructions = check.rubric.as_ref().map_or_else(
            || {
                format!(
                    "Evaluate how well the local workspace snapshot satisfies judge check `{}` for eval `{}`. Use the task, check standard/rubric, and workspace snapshot in state. Return a score on the ordered satisfaction criteria.",
                    check.check_id, definition.id
                )
            },
            |rubric| rubric.instructions.clone(),
        );
        questions.insert(
            check.check_id.clone(),
            TypeSafeScoreQuestion {
                question_type: "score",
                instructions,
                criteria: check.criteria.clone(),
            },
        );
    }

    Ok(TypeSafeRequest {
        state: TypeSafeState {
            eval_id: definition.id.clone(),
            task: definition.task.clone(),
            workspace: workspace.display().to_string(),
            checks: state_checks,
            workspace_snapshot: collect_typesafe_workspace_snapshot(workspace)?,
        },
        model: judge.model.clone(),
        questions,
    })
}

fn normalize_typesafe_check_result(
    check: &CheckDefinition,
    prepared: &PreparedJudgeCheck,
    answer: &TypeSafeScoreAnswer,
    response_model: Option<&str>,
    usage: Option<&TypeSafeUsage>,
    duration_ms: u128,
    requested_model: &str,
    default_passing_score: f64,
) -> anyhow::Result<CheckResult> {
    if answer.answer_type != "score" {
        bail!(
            "TypeSafe answer for judge check `{}` had type `{}`; expected `score`",
            check.id,
            answer.answer_type
        );
    }
    let top_level = prepared.criteria.len().saturating_sub(1);
    if top_level == 0 {
        bail!(
            "TypeSafe judge check `{}` has invalid criteria; expected at least two levels",
            check.id
        );
    }
    if !(0.0..=(top_level as f64)).contains(&answer.score) {
        bail!(
            "TypeSafe answer for judge check `{}` emitted score {}; expected 0.0 through {}",
            check.id,
            answer.score,
            top_level
        );
    }
    let normalized_score = answer.score / top_level as f64;
    let passing_score = check.min_score.unwrap_or(default_passing_score);
    let passed = normalized_score >= passing_score;
    let mut violations = Vec::new();
    if !passed {
        violations.push(format!(
            "TypeSafe score {:.2} normalized to {:.2} is below passing threshold {:.2}",
            answer.score, normalized_score, passing_score
        ));
    }

    Ok(CheckResult {
        id: check.id.clone(),
        check_type: CheckType::Judge,
        outcome: if passed {
            CheckOutcome::Passed
        } else {
            CheckOutcome::Failed
        },
        required: check.required,
        weight: check.weight,
        score: Some(normalized_score),
        duration_ms,
        command: Some("typesafe systemone".to_owned()),
        exit_code: None,
        standard: check.standard.clone(),
        standard_path: prepared.standard_path.clone(),
        violations,
        output: None,
        token_usage: usage.map(TokenUsage::from),
        model: response_model
            .map(str::to_owned)
            .or_else(|| Some(requested_model.to_owned())),
        harness: Some("typesafe".to_owned()),
        session_id: None,
        typesafe: Some(TypeSafeScoreMetadata {
            provider: "typesafe".to_owned(),
            question_id: check.id.clone(),
            raw_score: answer.score,
            normalized_score,
            confidence: answer.confidence,
            probabilities: answer.probabilities.clone(),
            legend: answer.legend.clone(),
            criteria: prepared.criteria.clone(),
        }),
    })
}

fn default_typesafe_criteria() -> Vec<String> {
    vec![
        "No evidence the workspace satisfies the task or referenced standard.".to_owned(),
        "Minimal or mostly unrelated progress; major expected behavior is absent.".to_owned(),
        "Partial satisfaction with important gaps, regressions, or unverified behavior.".to_owned(),
        "Substantial satisfaction with minor gaps or limited uncertainty.".to_owned(),
        "Complete satisfaction of the task and referenced standard with no material gaps found."
            .to_owned(),
    ]
}

fn collect_typesafe_workspace_snapshot(
    workspace: &Path,
) -> anyhow::Result<TypeSafeWorkspaceSnapshot> {
    let status = git_output(workspace, ["status", "--short", "--", "."])?;
    let diff_stat = git_output(workspace, ["diff", "--stat", "--", "."])?;
    let diff = git_output(workspace, ["diff", "--", "."])?;
    let (diff, truncated) = truncate_snapshot(diff, MAX_TYPESAFE_SNAPSHOT_BYTES);
    let (source_files, source_truncated) = collect_typesafe_source_files(workspace)?;
    Ok(TypeSafeWorkspaceSnapshot {
        status,
        diff_stat,
        diff,
        source_files,
        truncated: truncated || source_truncated,
    })
}

fn collect_typesafe_source_files(
    workspace: &Path,
) -> anyhow::Result<(BTreeMap<String, String>, bool)> {
    let mut paths = Vec::new();
    collect_typesafe_source_paths(workspace, workspace, &mut paths)?;
    paths.sort();

    let mut files = BTreeMap::new();
    let mut used_bytes = 0usize;
    let mut truncated = false;
    for path in paths {
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read source file `{}`", path.display()))?;
        let Ok(contents) = String::from_utf8(bytes) else {
            continue;
        };
        let relative = path
            .strip_prefix(workspace)
            .unwrap_or(&path)
            .display()
            .to_string();
        let remaining = MAX_TYPESAFE_SOURCE_SNAPSHOT_BYTES.saturating_sub(used_bytes);
        if remaining == 0 {
            truncated = true;
            break;
        }
        let (contents, file_truncated) = truncate_source_file(contents, remaining);
        used_bytes += relative.len() + contents.len();
        truncated |= file_truncated;
        files.insert(relative, contents);
        if file_truncated {
            break;
        }
    }

    Ok((files, truncated))
}

fn collect_typesafe_source_paths(
    workspace: &Path,
    dir: &Path,
    paths: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)
        .with_context(|| format!("failed to read directory `{}`", dir.display()))?
    {
        let path = entry
            .with_context(|| format!("failed to read directory entry in `{}`", dir.display()))?
            .path();
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if path.is_dir() {
            if matches!(
                name,
                ".git" | ".svdo" | "target" | "node_modules" | "__pycache__"
            ) {
                continue;
            }
            collect_typesafe_source_paths(workspace, &path, paths)?;
        } else if path.is_file() && is_typesafe_source_file(workspace, &path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn is_typesafe_source_file(workspace: &Path, path: &Path) -> bool {
    if path == workspace.join("Cargo.lock") {
        return false;
    }
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some(
            "c" | "cc"
                | "cpp"
                | "cs"
                | "go"
                | "h"
                | "hpp"
                | "java"
                | "js"
                | "jsx"
                | "json"
                | "kt"
                | "md"
                | "php"
                | "py"
                | "rb"
                | "rs"
                | "sh"
                | "swift"
                | "toml"
                | "ts"
                | "tsx"
                | "txt"
                | "yaml"
                | "yml"
        )
    )
}

fn truncate_source_file(value: String, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value, false);
    }
    let mut boundary = max_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    (
        format!(
            "{}\n\n[truncated TypeSafe source snapshot to {max_bytes} bytes]",
            &value[..boundary]
        ),
        true,
    )
}

fn git_output<const N: usize>(workspace: &Path, args: [&str; N]) -> anyhow::Result<Option<String>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .with_context(|| {
            format!(
                "failed to collect TypeSafe judge workspace snapshot in `{}`",
                workspace.display()
            )
        })?;
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!value.is_empty()).then_some(value))
}

fn truncate_snapshot(value: Option<String>, max_bytes: usize) -> (Option<String>, bool) {
    let Some(value) = value else {
        return (None, false);
    };
    if value.len() <= max_bytes {
        return (Some(value), false);
    }
    let mut boundary = max_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    (
        Some(format!(
            "{}\n\n[truncated TypeSafe judge workspace diff to {max_bytes} bytes]",
            &value[..boundary]
        )),
        true,
    )
}

fn invalid_judge_response_result(
    check: &CheckDefinition,
    request: &JudgeRequest<'_>,
    execution: JudgeExecution,
    duration_ms: u128,
    reason: String,
) -> CheckResult {
    let output = failure_output(&execution.stdout, &execution.stderr);
    CheckResult {
        id: check.id.clone(),
        check_type: CheckType::Judge,
        outcome: CheckOutcome::Failed,
        required: check.required,
        weight: check.weight,
        score: Some(0.0),
        duration_ms,
        command: Some(execution.command),
        exit_code: execution.exit_code,
        standard: check.standard.clone(),
        standard_path: request.standard_path.clone(),
        violations: vec![format!("{reason}; output excerpt: {output}")],
        output: Some(output),
        token_usage: None,
        model: execution.model,
        harness: Some(execution.harness.to_owned()),
        session_id: None,
        typesafe: None,
    }
}

fn resolve_standard(workspace: &Path, standard: &str) -> anyhow::Result<PathBuf> {
    let standards_dir = workspace.join(".svdo").join("standards");
    let candidates = [
        standards_dir.join(standard),
        standards_dir.join(format!("{standard}.md")),
        standards_dir.join(format!("{standard}.yaml")),
        standards_dir.join(format!("{standard}.yml")),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .with_context(|| {
            format!(
                "standard `{standard}` referenced by judge check was not found in `{}`",
                standards_dir.display()
            )
        })
}

fn load_typesafe_rubric(workspace: &Path, rubric: &str) -> anyhow::Result<PreparedRubric> {
    let path = resolve_rubric(workspace, rubric)?;
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read rubric `{}`", path.display()))?;
    let definition = parse_rubric_definition(&contents)
        .with_context(|| format!("failed to parse rubric `{}`", path.display()))?;
    validate_rubric_definition(&definition, rubric, &path)?;
    Ok(PreparedRubric {
        id: rubric.to_owned(),
        path: path.display().to_string(),
        instructions: definition.instructions,
        criteria: definition.criteria,
    })
}

fn resolve_rubric(workspace: &Path, rubric: &str) -> anyhow::Result<PathBuf> {
    let rubrics_dir = workspace.join(".svdo").join("rubrics");
    let standards_dir = workspace.join(".svdo").join("standards");
    let candidates = [
        rubrics_dir.join(rubric),
        rubrics_dir.join(format!("{rubric}.yaml")),
        rubrics_dir.join(format!("{rubric}.yml")),
        standards_dir.join(format!("{rubric}.rubric.yaml")),
        standards_dir.join(format!("{rubric}.rubric.yml")),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .with_context(|| {
            format!(
                "rubric `{rubric}` referenced by judge check was not found in `{}` or as `*.rubric.yaml` under `{}`",
                rubrics_dir.display(),
                standards_dir.display()
            )
        })
}

#[derive(Debug)]
struct RubricDefinition {
    instructions: String,
    criteria: Vec<String>,
}

fn parse_rubric_definition(value: &str) -> anyhow::Result<RubricDefinition> {
    let lines = value.lines().collect::<Vec<_>>();
    let mut index = 0;
    let mut instructions = None;
    let mut criteria = Vec::new();

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            index += 1;
            continue;
        }
        if line.starts_with(char::is_whitespace) {
            bail!("unexpected indented rubric line `{trimmed}`");
        }
        if let Some(value) = trimmed.strip_prefix("instructions:") {
            let value = value.trim();
            if value == "|" {
                let (block, next_index) = parse_block(&lines, index + 1);
                instructions = Some(block);
                index = next_index;
            } else {
                instructions = Some(unquote(value).to_owned());
                index += 1;
            }
        } else if trimmed == "criteria:" {
            let (parsed_criteria, next_index) = parse_rubric_criteria(&lines, index + 1)?;
            criteria = parsed_criteria;
            index = next_index;
        } else {
            bail!("unsupported rubric field `{trimmed}`");
        }
    }

    Ok(RubricDefinition {
        instructions: instructions.context("rubric is missing instructions")?,
        criteria,
    })
}

fn parse_rubric_criteria(lines: &[&str], mut index: usize) -> anyhow::Result<(Vec<String>, usize)> {
    let mut criteria = Vec::new();
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            index += 1;
            continue;
        }
        if !line.starts_with("  ") {
            break;
        }
        let Some(value) = line.strip_prefix("  - ") else {
            bail!("expected rubric criteria item, found `{trimmed}`");
        };
        criteria.push(unquote(value.trim()).to_owned());
        index += 1;
    }
    Ok((criteria, index))
}

fn validate_rubric_definition(
    rubric: &RubricDefinition,
    id: &str,
    path: &Path,
) -> anyhow::Result<()> {
    if rubric.instructions.trim().is_empty() {
        bail!(
            "rubric `{id}` in `{}` has empty instructions",
            path.display()
        );
    }
    if !(2..=10).contains(&rubric.criteria.len()) {
        bail!(
            "rubric `{id}` in `{}` must define between 2 and 10 ordered criteria levels; found {}",
            path.display(),
            rubric.criteria.len()
        );
    }
    if rubric
        .criteria
        .iter()
        .any(|criterion| criterion.trim().is_empty())
    {
        bail!(
            "rubric `{id}` in `{}` has an empty criterion",
            path.display()
        );
    }
    Ok(())
}

impl JudgeCommand {
    fn run(&self, request_path: &Path, workspace: &Path) -> std::io::Result<Output> {
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .arg(request_path)
            .env("SVDO_METER_JUDGE_REQUEST", request_path)
            .current_dir(workspace)
            .output()
    }

    fn display_command(&self, request_path: &Path) -> String {
        let mut parts = Vec::with_capacity(self.args.len() + 2);
        parts.push(self.program.display().to_string());
        parts.extend(self.args.iter().cloned());
        parts.push(request_path.display().to_string());
        parts.join(" ")
    }
}

impl JudgeRunner {
    fn run(
        &self,
        request: &JudgeRequest<'_>,
        workspace: &Path,
        check_id: &str,
    ) -> anyhow::Result<JudgeExecution> {
        match self {
            Self::Codex(judge) => judge.run(request, workspace),
            Self::Claude(judge) => judge.run(request, workspace),
            Self::OpenCode(judge) => judge.run(request, workspace),
            Self::TypeSafe(_) => bail!("TypeSafe judge checks are executed in a batched request"),
            Self::Command(command) => {
                let request_path = write_judge_request(request, check_id)?;
                let output = command.run(&request_path, workspace);
                let remove_result = remove_judge_request(&request_path);
                let output = output?;
                remove_result?;
                Ok(JudgeExecution {
                    success: output.status.success(),
                    exit_code: output.status.code(),
                    stdout: output.stdout,
                    stderr: output.stderr,
                    command: command.display_command(&request_path),
                    harness: "judge-command",
                    model: None,
                })
            }
        }
    }
}

impl TypeSafeJudge {
    fn new(endpoint: String, model: String, api_key_env: String) -> anyhow::Result<Self> {
        if model.trim().is_empty() {
            bail!("TypeSafe judge backend requires a non-empty model value");
        }
        if api_key_env.trim().is_empty() {
            bail!("TypeSafe judge backend requires a non-empty api_key_env value");
        }
        if endpoint.trim().is_empty() {
            bail!("TypeSafe judge backend requires a non-empty url value");
        }
        Ok(Self {
            endpoint,
            model,
            api_key_env,
        })
    }

    fn run(&self, request: &TypeSafeRequest) -> anyhow::Result<TypeSafeResponse> {
        let api_key = std::env::var(&self.api_key_env).with_context(|| {
            format!(
                "TypeSafe judge backend requires an API key in environment variable `{}`. Set `{}` or pass --typesafe-api-key-env with the variable name to use.",
                self.api_key_env, self.api_key_env
            )
        })?;
        if api_key.trim().is_empty() {
            bail!(
                "TypeSafe judge backend found `{}` but it is empty. Set it to a valid TypeSafe API key.",
                self.api_key_env
            );
        }

        let endpoint = self.endpoint.clone();
        let request_body =
            serde_json::to_string(request).context("failed to serialize TypeSafe request JSON")?;
        std::thread::spawn(move || call_typesafe_endpoint(endpoint, api_key, request_body))
            .join()
            .map_err(|panic| {
                let message = panic
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("unknown panic");
                anyhow::anyhow!("TypeSafe System One request worker panicked: {message}")
            })?
    }
}

fn call_typesafe_endpoint(
    endpoint: String,
    api_key: String,
    request_body: String,
) -> anyhow::Result<TypeSafeResponse> {
    call_typesafe_endpoint_with_retry(
        endpoint,
        api_key,
        request_body,
        TYPESAFE_MAX_ATTEMPTS,
        TYPESAFE_INITIAL_BACKOFF,
    )
}

fn call_typesafe_endpoint_with_retry(
    endpoint: String,
    api_key: String,
    request_body: String,
    max_attempts: u32,
    initial_backoff: Duration,
) -> anyhow::Result<TypeSafeResponse> {
    let client = reqwest::blocking::Client::new();
    call_typesafe_endpoint_with_transport(max_attempts, initial_backoff, || {
        let response = client
            .post(&endpoint)
            .bearer_auth(&api_key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request_body.clone())
            .send()
            .with_context(|| format!("failed to call TypeSafe System One endpoint `{endpoint}`"))?;
        let status = response.status();
        let body = response
            .text()
            .context("failed to read TypeSafe System One response body")?;
        Ok((status, body))
    })
}

fn call_typesafe_endpoint_with_transport<F>(
    max_attempts: u32,
    initial_backoff: Duration,
    mut send: F,
) -> anyhow::Result<TypeSafeResponse>
where
    F: FnMut() -> anyhow::Result<(reqwest::StatusCode, String)>,
{
    let max_attempts = max_attempts.max(1);
    let mut backoff = initial_backoff;
    let mut last_retryable_failure = None;
    for attempt in 1..=max_attempts {
        let (status, body) = send()?;
        if status.is_success() {
            return parse_typesafe_response(&body);
        }
        if !is_retryable_typesafe_status(status) || attempt == max_attempts {
            bail!(
                "TypeSafe System One request failed with HTTP status {status}: {}",
                truncate(&body, 4_000)
            );
        }
        last_retryable_failure = Some((status, body));
        if !backoff.is_zero() {
            std::thread::sleep(backoff);
            backoff = backoff.saturating_mul(2);
        }
    }
    let (status, body) =
        last_retryable_failure.expect("retry loop records a retryable failure before exhausting");
    bail!(
        "TypeSafe System One request failed with HTTP status {status}: {}",
        truncate(&body, 4_000)
    )
}

fn is_retryable_typesafe_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 529
}

fn parse_typesafe_response(body: &str) -> anyhow::Result<TypeSafeResponse> {
    serde_json::from_str(body).with_context(|| {
        format!(
            "failed to parse TypeSafe System One response JSON: {}",
            truncate(body, 4_000)
        )
    })
}

impl From<&TypeSafeUsage> for TokenUsage {
    fn from(usage: &TypeSafeUsage) -> Self {
        Self {
            input: usage.input_tokens,
            output: usage.output_tokens,
            cache_read: usage.cache_read_tokens,
            total: usage.total_tokens.or_else(|| {
                match (
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.cache_read_tokens,
                ) {
                    (None, None, None) => None,
                    (input, output, cache_read) => {
                        Some(input.unwrap_or(0) + output.unwrap_or(0) + cache_read.unwrap_or(0))
                    }
                }
            }),
        }
    }
}

impl OpenCodeJudge {
    fn run(&self, request: &JudgeRequest<'_>, workspace: &Path) -> anyhow::Result<JudgeExecution> {
        let request_json =
            serde_json::to_string_pretty(request).context("failed to serialize judge request")?;
        let prompt = judge_prompt(&request_json);
        let config = OpenCodeConfig::default();
        let args = opencode_argv(
            Some(workspace),
            self.model.as_ref(),
            None,
            ExecutionPermissionMode::Standard,
            None,
            &prompt,
        );
        let mut command = Command::new(&config.binary);
        command.args(&args).current_dir(workspace);
        let output = run_streamed_command(&mut command)
            .with_context(|| format!("failed to execute `{}`", config.binary.display()))?;
        let display = std::iter::once(config.binary.display().to_string())
            .chain(args.iter().map(|arg| arg.to_string_lossy().into_owned()))
            .collect::<Vec<_>>()
            .join(" ");
        Ok(JudgeExecution {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
            command: display,
            harness: "opencode",
            model: self.model.as_ref().map(|model| model.as_str().to_owned()),
        })
    }
}

fn run_streamed_command(command: &mut Command) -> anyhow::Result<StreamedCommandOutput> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .context("failed to capture judge stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("failed to capture judge stderr")?;
    let stdout_reader = thread::spawn(move || collect_bounded_output(stdout));
    let stderr_reader = thread::spawn(move || collect_bounded_output(stderr));
    let status = child.wait()?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| anyhow::anyhow!("judge stdout reader panicked"))?
        .context("failed to read judge stdout")?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("judge stderr reader panicked"))?
        .context("failed to read judge stderr")?;
    Ok(StreamedCommandOutput {
        status,
        stdout,
        stderr,
    })
}

fn collect_bounded_output(mut stream: impl Read) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(output);
        }
        append_bounded_tail(&mut output, &buffer[..read], MAX_JUDGE_OUTPUT_BYTES);
    }
}

fn append_bounded_tail(output: &mut Vec<u8>, chunk: &[u8], max_bytes: usize) {
    if chunk.len() >= max_bytes {
        output.clear();
        output.extend_from_slice(&chunk[chunk.len() - max_bytes..]);
        return;
    }
    let excess = output
        .len()
        .saturating_add(chunk.len())
        .saturating_sub(max_bytes);
    if excess > 0 {
        output.drain(..excess);
    }
    output.extend_from_slice(chunk);
}

impl ClaudeJudge {
    fn run(&self, request: &JudgeRequest<'_>, workspace: &Path) -> anyhow::Result<JudgeExecution> {
        let request_json =
            serde_json::to_string_pretty(request).context("failed to serialize judge request")?;
        let prompt = judge_prompt(&request_json);
        let config = ClaudeConfig::default();
        let options = ClaudeRunOptions::default();
        let args = claude_argv(self.model.as_ref(), None, &options, &prompt)
            .map_err(|error| anyhow::anyhow!(error))
            .context("failed to build Claude judge arguments")?;
        let mut command = Command::new(&config.binary);
        let output = command
            .args(&args)
            .current_dir(workspace)
            .output()
            .with_context(|| format!("failed to execute `{}`", config.binary.display()))?;
        let display = std::iter::once(config.binary.display().to_string())
            .chain(args.iter().map(|arg| arg.to_string_lossy().into_owned()))
            .collect::<Vec<_>>()
            .join(" ");
        Ok(JudgeExecution {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
            command: display,
            harness: "claude",
            model: self.model.as_ref().map(|model| model.as_str().to_owned()),
        })
    }
}

impl CodexJudge {
    fn run(&self, request: &JudgeRequest<'_>, workspace: &Path) -> anyhow::Result<JudgeExecution> {
        let request_json =
            serde_json::to_string_pretty(request).context("failed to serialize judge request")?;
        let prompt = judge_prompt(&request_json);
        let config = CodexConfig::default();
        let args = codex_judge_argv(&config, Some(workspace), self.model.as_ref(), &prompt);
        let mut command = Command::new(&config.binary);
        let output = command
            .args(&args)
            .output()
            .with_context(|| format!("failed to execute `{}`", config.binary.display()))?;
        let display = std::iter::once(config.binary.display().to_string())
            .chain(args.iter().map(|arg| arg.to_string_lossy().into_owned()))
            .collect::<Vec<_>>()
            .join(" ");
        Ok(JudgeExecution {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
            command: display,
            harness: "codex",
            model: self.model.as_ref().map(|model| model.as_str().to_owned()),
        })
    }
}

fn codex_judge_argv(
    config: &CodexConfig,
    workspace: Option<&Path>,
    model: Option<&ModelName>,
    prompt: &str,
) -> Vec<OsString> {
    let mut args = codex_argv(config, workspace, model, None, prompt);
    let prompt_index = args.len().saturating_sub(1);
    args.insert(prompt_index, OsString::from("--skip-git-repo-check"));
    args
}

fn judge_prompt(request_json: &str) -> String {
    format!(
        "You are evaluating an SVDO repository alignment check.\n\
Return only one JSON object with this schema:\n\
{{\"score\": number from 0.0 to 1.0, \"passed\": boolean, \"violations\": string[]}}\n\
Do not include Markdown or explanatory text outside the JSON object.\n\n\
Judge request:\n{request_json}"
    )
}

fn parse_judge_response(stdout: &str, check_id: &str) -> anyhow::Result<JudgeResponse> {
    let trimmed = stdout.trim();
    if let Ok(response) = serde_json::from_str::<JudgeResponse>(trimmed) {
        return Ok(response);
    }
    for line in trimmed
        .lines()
        .rev()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Ok(response) = serde_json::from_str::<JudgeResponse>(line) {
            return Ok(response);
        }
        if let Ok(value) = serde_json::from_str::<Value>(line)
            && let Some(response) = judge_response_from_value(&value)?
        {
            return Ok(response);
        }
    }
    if let Some(response) = judge_response_from_text(trimmed)? {
        return Ok(response);
    }
    bail!("judge check `{check_id}` must emit JSON with a numeric `score` field")
}

fn judge_response_from_value(value: &Value) -> anyhow::Result<Option<JudgeResponse>> {
    if value.get("score").is_some() {
        return serde_json::from_value(value.clone())
            .map(Some)
            .context("failed to parse judge response");
    }
    for key in [
        "message",
        "content",
        "output",
        "text",
        "final_answer",
        "result",
    ] {
        if let Some(text) = value.get(key).and_then(Value::as_str)
            && let Some(response) = judge_response_from_text(text)?
        {
            return Ok(Some(response));
        }
    }
    for key in ["message", "item", "data", "response", "part"] {
        if let Some(nested) = value.get(key)
            && let Some(response) = judge_response_from_value(nested)?
        {
            return Ok(Some(response));
        }
    }
    for key in ["content", "parts"] {
        if let Some(items) = value.get(key).and_then(Value::as_array) {
            for item in items {
                if let Some(response) = judge_response_from_value(item)? {
                    return Ok(Some(response));
                }
            }
        }
    }
    Ok(None)
}

fn judge_response_from_text(text: &str) -> anyhow::Result<Option<JudgeResponse>> {
    let trimmed = text.trim();
    if let Ok(response) = serde_json::from_str::<JudgeResponse>(trimmed) {
        return Ok(Some(response));
    }
    let Some(start) = trimmed.find('{') else {
        return Ok(None);
    };
    let Some(end) = trimmed.rfind('}') else {
        return Ok(None);
    };
    if start >= end {
        return Ok(None);
    }
    serde_json::from_str::<JudgeResponse>(&trimmed[start..=end])
        .map(Some)
        .or(Ok(None))
}

fn write_judge_request(request: &JudgeRequest<'_>, check_id: &str) -> anyhow::Result<PathBuf> {
    let bytes = serde_json::to_vec_pretty(request).context("failed to serialize judge request")?;
    let mut last_error = None;
    for attempt in 0..10 {
        let path = std::env::temp_dir().join(format!(
            "svdo-meter-judge-{}-{}-{}-{}.json",
            std::process::id(),
            timestamp_nanos(),
            attempt,
            sanitize_file_component(check_id)
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write as _;

                file.write_all(&bytes).with_context(|| {
                    format!("failed to write judge request `{}`", path.display())
                })?;
                return Ok(path);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists && attempt < 9 => {
                last_error = Some(error);
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to create judge request in `{}`",
                        std::env::temp_dir().display()
                    )
                });
            }
        }
    }
    Err(last_error.unwrap_or_else(|| std::io::Error::other("temporary file already exists")))
        .context("failed to create unique judge request file")
}

fn remove_judge_request(path: &Path) -> anyhow::Result<()> {
    fs::remove_file(path)
        .with_context(|| format!("failed to remove judge request `{}`", path.display()))
}

fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn sanitize_file_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "check".to_owned()
    } else {
        sanitized
    }
}

fn validate_definition(definition: &EvalDefinition, path: &Path) -> anyhow::Result<()> {
    if definition.id.trim().is_empty() {
        bail!("eval definition `{}` has an empty id", path.display());
    }
    if definition.task.trim().is_empty() {
        bail!("eval definition `{}` has an empty task", path.display());
    }
    if definition.checks.is_empty() {
        bail!("eval definition `{}` has no checks", path.display());
    }
    if let Some(judge) = &definition.judge {
        match judge.backend {
            Some(EvalJudgeBackend::TypeSafe) => {
                if judge
                    .model
                    .as_deref()
                    .is_some_and(|value| value.trim().is_empty())
                {
                    bail!(
                        "eval definition `{}` TypeSafe judge model must not be empty",
                        path.display()
                    );
                }
                if judge
                    .api_key_env
                    .as_deref()
                    .is_some_and(|value| value.trim().is_empty())
                {
                    bail!(
                        "eval definition `{}` TypeSafe judge api_key_env must not be empty",
                        path.display()
                    );
                }
                if judge
                    .url
                    .as_deref()
                    .is_some_and(|value| value.trim().is_empty())
                {
                    bail!(
                        "eval definition `{}` TypeSafe judge url must not be empty",
                        path.display()
                    );
                }
            }
            None => {
                bail!(
                    "eval definition `{}` judge config is missing backend",
                    path.display()
                );
            }
        }
    }
    if !(0.0..=1.0).contains(&definition.threshold) {
        bail!(
            "eval definition `{}` threshold must be between 0.0 and 1.0",
            path.display()
        );
    }
    for check in &definition.checks {
        if check.id.trim().is_empty() {
            bail!(
                "eval definition `{}` has a check with an empty id",
                path.display()
            );
        }
        if check.weight < 0.0 {
            bail!(
                "eval definition `{}` check `{}` has a negative weight",
                path.display(),
                check.id
            );
        }
        if check
            .min_score
            .is_some_and(|min_score| !(0.0..=1.0).contains(&min_score))
        {
            bail!(
                "eval definition `{}` check `{}` min_score must be between 0.0 and 1.0",
                path.display(),
                check.id
            );
        }
        if check
            .rubric
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            bail!(
                "eval definition `{}` check `{}` has an empty rubric",
                path.display(),
                check.id
            );
        }
        if check.min_count == 0 {
            bail!(
                "eval definition `{}` check `{}` min_count must be at least 1",
                path.display(),
                check.id
            );
        }
        if check.check_type == CheckType::Command
            && check.command.as_deref().unwrap_or("").trim().is_empty()
        {
            bail!(
                "eval definition `{}` command check `{}` is missing command",
                path.display(),
                check.id
            );
        }
        if check.check_type == CheckType::Telemetry
            && check.event_type.as_deref().unwrap_or("").trim().is_empty()
        {
            bail!(
                "eval definition `{}` telemetry check `{}` is missing event_type",
                path.display(),
                check.id
            );
        }
    }
    Ok(())
}

pub fn parse_definition(value: &str) -> anyhow::Result<EvalDefinition> {
    let lines = value.lines().collect::<Vec<_>>();
    let mut index = 0;
    let mut id = None;
    let mut task = None;
    let mut judge = None;
    let mut threshold = default_threshold();
    let mut checks = Vec::new();

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            index += 1;
            continue;
        }
        if line.starts_with(char::is_whitespace) {
            bail!("unexpected indented line `{trimmed}`");
        }
        if let Some(value) = trimmed.strip_prefix("id:") {
            id = Some(unquote(value.trim()).to_owned());
            index += 1;
        } else if let Some(value) = trimmed.strip_prefix("task:") {
            let value = value.trim();
            if value == "|" {
                let (block, next_index) = parse_block(&lines, index + 1);
                task = Some(block);
                index = next_index;
            } else {
                task = Some(unquote(value).to_owned());
                index += 1;
            }
        } else if trimmed == "checks:" {
            let (parsed_checks, next_index) = parse_checks(&lines, index + 1)?;
            checks = parsed_checks;
            index = next_index;
        } else if trimmed == "judge:" {
            let (parsed_judge, next_index) = parse_judge_config(&lines, index + 1)?;
            judge = Some(parsed_judge);
            index = next_index;
        } else if let Some(value) = trimmed.strip_prefix("threshold:") {
            threshold = value
                .trim()
                .parse::<f64>()
                .context("threshold must be a number")?;
            index += 1;
        } else {
            bail!("unsupported eval field `{trimmed}`");
        }
    }

    Ok(EvalDefinition {
        id: id.context("eval definition is missing id")?,
        task: task.context("eval definition is missing task")?,
        judge,
        checks,
        threshold,
        source_path: None,
    })
}

fn parse_judge_config(
    lines: &[&str],
    mut index: usize,
) -> anyhow::Result<(EvalJudgeConfig, usize)> {
    let mut judge = EvalJudgeConfig::default();
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            index += 1;
            continue;
        }
        if !line.starts_with("  ") {
            break;
        }
        let field = line
            .strip_prefix("  ")
            .with_context(|| format!("unsupported judge indentation `{trimmed}`"))?;
        apply_judge_field(&mut judge, field.trim())?;
        index += 1;
    }
    Ok((judge, index))
}

fn apply_judge_field(judge: &mut EvalJudgeConfig, field: &str) -> anyhow::Result<()> {
    let Some((key, value)) = field.split_once(':') else {
        bail!("expected judge field `key: value`, found `{field}`");
    };
    let value = unquote(value.trim());
    match key.trim() {
        "backend" => {
            judge.backend = Some(match value {
                "typesafe" => EvalJudgeBackend::TypeSafe,
                other => bail!("unsupported judge backend `{other}`"),
            });
        }
        "model" => judge.model = Some(value.to_owned()),
        "api_key_env" => judge.api_key_env = Some(value.to_owned()),
        "url" => judge.url = Some(value.to_owned()),
        other => bail!("unsupported judge field `{other}`"),
    }
    Ok(())
}

fn parse_checks(lines: &[&str], mut index: usize) -> anyhow::Result<(Vec<CheckDefinition>, usize)> {
    let mut checks = Vec::new();
    let mut current: Option<CheckDefinition> = None;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            index += 1;
            continue;
        }
        if !line.starts_with("  ") {
            break;
        }
        if let Some(rest) = line.strip_prefix("  - ") {
            if let Some(check) = current.take() {
                checks.push(check);
            }
            let mut check = CheckDefinition {
                id: String::new(),
                check_type: CheckType::Command,
                command: None,
                required: false,
                weight: default_weight(),
                min_score: None,
                standard: None,
                rubric: None,
                event_type: None,
                tool_name: None,
                min_count: DEFAULT_TELEMETRY_MIN_COUNT,
            };
            apply_check_field(&mut check, rest.trim())?;
            current = Some(check);
            index += 1;
            continue;
        }
        let Some(check) = current.as_mut() else {
            bail!("check field `{trimmed}` appeared before a check item");
        };
        let field = line
            .strip_prefix("    ")
            .with_context(|| format!("unsupported check indentation `{trimmed}`"))?;
        apply_check_field(check, field.trim())?;
        index += 1;
    }
    if let Some(check) = current {
        checks.push(check);
    }
    Ok((checks, index))
}

fn apply_check_field(check: &mut CheckDefinition, field: &str) -> anyhow::Result<()> {
    let Some((key, value)) = field.split_once(':') else {
        bail!("expected check field `key: value`, found `{field}`");
    };
    let value = unquote(value.trim());
    match key.trim() {
        "id" => check.id = value.to_owned(),
        "type" => {
            check.check_type = match value {
                "command" => CheckType::Command,
                "judge" => CheckType::Judge,
                "telemetry" => CheckType::Telemetry,
                other => bail!("unsupported check type `{other}`"),
            };
        }
        "command" => check.command = Some(value.to_owned()),
        "required" => {
            check.required = match value {
                "true" => true,
                "false" => false,
                other => bail!("required must be true or false, found `{other}`"),
            };
        }
        "weight" => check.weight = value.parse::<f64>().context("weight must be a number")?,
        "min_score" => {
            check.min_score = Some(value.parse::<f64>().context("min_score must be a number")?)
        }
        "standard" => check.standard = Some(value.to_owned()),
        "rubric" => check.rubric = Some(value.to_owned()),
        "event_type" => check.event_type = Some(value.to_owned()),
        "tool_name" => check.tool_name = Some(value.to_owned()),
        "min_count" => {
            check.min_count = value
                .parse::<u64>()
                .context("min_count must be an integer")?
        }
        other => bail!("unsupported check field `{other}`"),
    }
    Ok(())
}

fn parse_block(lines: &[&str], mut index: usize) -> (String, usize) {
    let mut values = Vec::new();
    while index < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() {
            values.push(String::new());
            index += 1;
            continue;
        }
        let Some(value) = line.strip_prefix("  ") else {
            break;
        };
        values.push(value.to_owned());
        index += 1;
    }
    (values.join("\n"), index)
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn render_terminal(report: &EvalRunReport) -> String {
    let mut output = format!(
        "SVDO Eval\n\nResult\n  {}\nDuration\n  {}ms\nEvals\n  {}\n",
        pass_label(report.passed),
        report.duration_ms,
        report.results.len()
    );
    for result in &report.results {
        output.push_str(&format!(
            "\n{}  score {:.2} / threshold {:.2}  {}\n",
            result.id,
            result.overall_score,
            result.threshold,
            pass_label(result.passed)
        ));
        output.push_str(&format!("  Duration: {}ms\n", result.duration_ms));
        let failed = result
            .checks
            .iter()
            .filter(|check| check.outcome == CheckOutcome::Failed)
            .map(|check| check.id.as_str())
            .collect::<Vec<_>>();
        if !failed.is_empty() {
            output.push_str(&format!("  Failed checks: {}\n", failed.join(", ")));
        }
        for check in &result.checks {
            output.push_str(&format!(
                "  - {} [{}] {}",
                check.id,
                check_type_label(check.check_type),
                outcome_label(check.outcome)
            ));
            if let Some(score) = check.score {
                output.push_str(&format!(" score {:.2}", score));
            }
            if check.required {
                output.push_str(" required");
            }
            if let Some(provider) = check_provider_label(check) {
                output.push_str(&format!(" via {provider}"));
            }
            output.push('\n');
        }
        append_failure_report(&mut output, result);
    }
    output
}

fn append_failure_report(output: &mut String, result: &EvalResult) {
    let failed_checks = result
        .checks
        .iter()
        .filter(|check| check.outcome == CheckOutcome::Failed)
        .collect::<Vec<_>>();

    if failed_checks.is_empty() && result.violations.is_empty() {
        return;
    }

    output.push_str("  Failure Report:\n");
    for check in failed_checks {
        output.push_str(&format!(
            "    {} [{}]",
            check.id,
            check_type_label(check.check_type)
        ));
        if let Some(score) = check.score {
            output.push_str(&format!(" score {:.2}", score));
        }
        if check.required {
            output.push_str(" required");
        }
        if let Some(exit_code) = check.exit_code {
            output.push_str(&format!(" exit {exit_code}"));
        }
        if let Some(provider) = check_provider_label(check) {
            output.push_str(&format!(" via {provider}"));
        }
        output.push('\n');

        for violation in &check.violations {
            output.push_str(&format!("      - {violation}\n"));
        }

        if let Some(output_value) = &check.output {
            append_output_excerpt(output, output_value);
        }
    }

    let overall_violations = result
        .violations
        .iter()
        .filter(|violation| {
            !result.checks.iter().any(|check| {
                check
                    .violations
                    .iter()
                    .any(|check_violation| check_violation == *violation)
            })
        })
        .collect::<Vec<_>>();
    if !overall_violations.is_empty() {
        output.push_str("    Overall:\n");
        for violation in overall_violations {
            output.push_str(&format!("      - {violation}\n"));
        }
    }
}

fn append_output_excerpt(output: &mut String, value: &str) {
    const MAX_OUTPUT_LINES: usize = 40;
    const MAX_OUTPUT_CHARS: usize = 4_000;

    let line_count = value.lines().count();
    let omitted_lines = line_count.saturating_sub(MAX_OUTPUT_LINES);
    let tail = value
        .lines()
        .skip(omitted_lines)
        .collect::<Vec<_>>()
        .join("\n");
    let excerpt = truncate_tail(&tail, MAX_OUTPUT_CHARS);

    if omitted_lines > 0 {
        output.push_str(&format!(
            "      output: showing last {MAX_OUTPUT_LINES} lines ({omitted_lines} earlier lines omitted)\n"
        ));
    } else {
        output.push_str("      output:\n");
    }
    for line in excerpt.lines() {
        output.push_str(&format!("        {line}\n"));
    }
}

fn render_csv(report: &EvalRunReport) -> String {
    let mut output = "eval_id,eval_passed,overall_score,threshold,eval_duration_ms,check_id,check_type,check_outcome,check_harness,required,weight,score,check_duration_ms,exit_code,violations\n".to_owned();
    for result in &report.results {
        for check in &result.checks {
            output.push_str(&format!(
                "{},{},{:.4},{:.4},{},{},{},{},{},{},{:.4},{},{},{},{}\n",
                csv(&result.id),
                result.passed,
                result.overall_score,
                result.threshold,
                result.duration_ms,
                csv(&check.id),
                check_type_label(check.check_type),
                outcome_label(check.outcome),
                csv(check.harness.as_deref().unwrap_or("")),
                check.required,
                check.weight,
                check
                    .score
                    .map_or_else(String::new, |score| format!("{score:.4}")),
                check.duration_ms,
                check
                    .exit_code
                    .map_or_else(String::new, |code| code.to_string()),
                csv(&check.violations.join("; "))
            ));
        }
    }
    output
}

fn check_provider_label(check: &CheckResult) -> Option<String> {
    let harness = check.harness.as_deref()?;
    match check.model.as_deref() {
        Some(model) => Some(format!("{harness}/{model}")),
        None => Some(harness.to_owned()),
    }
}

fn failure_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut value = String::new();
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    if !stdout.trim().is_empty() {
        value.push_str(stdout.trim());
    }
    if !stderr.trim().is_empty() {
        if !value.is_empty() {
            value.push('\n');
        }
        value.push_str(stderr.trim());
    }
    truncate_tail(&value, 4_000)
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn truncate_tail(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_owned();
    }
    let mut truncated = "...".to_owned();
    truncated.push_str(
        &value
            .chars()
            .skip(char_count - max_chars)
            .collect::<String>(),
    );
    truncated
}

fn is_yaml(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("yaml" | "yml")
    )
}

fn csv(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn pass_label(passed: bool) -> &'static str {
    if passed { "PASS" } else { "FAIL" }
}

fn check_type_label(check_type: CheckType) -> &'static str {
    match check_type {
        CheckType::Command => "command",
        CheckType::Judge => "judge",
        CheckType::Telemetry => "telemetry",
    }
}

fn outcome_label(outcome: CheckOutcome) -> &'static str {
    match outcome {
        CheckOutcome::Passed => "passed",
        CheckOutcome::Failed => "failed",
        CheckOutcome::Skipped => "skipped",
    }
}

fn elapsed_ms(duration: Duration) -> u128 {
    duration.as_millis()
}

fn default_threshold() -> f64 {
    1.0
}

fn default_weight() -> f64 {
    1.0
}

impl EvalDefinition {
    fn matches(&self, requested_eval: &str) -> bool {
        if self.id == requested_eval {
            return true;
        }
        let Some(path) = &self.source_path else {
            return false;
        };
        path.file_stem().and_then(OsStr::to_str) == Some(requested_eval)
            || path.file_name().and_then(OsStr::to_str) == Some(requested_eval)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn parses_eval_definition() -> anyhow::Result<()> {
        let definition = parse_definition(
            r#"
id: add-account-endpoint
task: |
  Add GET /accounts/{account_id}.
checks:
  - id: tests
    type: command
    command: pytest
    required: true
  - id: architecture
    type: judge
    standard: api-architecture
    rubric: architecture-alignment
    min_score: 0.75
    weight: 0.4
  - id: requires-apply-patch
    type: telemetry
    event_type: tool.started
    tool_name: apply_patch
    min_count: 1
threshold: 0.85
"#,
        )?;

        assert_eq!(definition.id, "add-account-endpoint");
        assert_eq!(definition.threshold, 0.85);
        assert_eq!(definition.checks.len(), 3);
        assert!(definition.checks[0].required);
        assert_eq!(definition.checks[1].check_type, CheckType::Judge);
        assert_eq!(definition.checks[1].weight, 0.4);
        assert_eq!(definition.checks[1].min_score, Some(0.75));
        assert_eq!(
            definition.checks[1].rubric.as_deref(),
            Some("architecture-alignment")
        );
        assert_eq!(definition.checks[2].check_type, CheckType::Telemetry);
        assert_eq!(
            definition.checks[2].event_type.as_deref(),
            Some("tool.started")
        );
        assert_eq!(
            definition.checks[2].tool_name.as_deref(),
            Some("apply_patch")
        );
        assert_eq!(definition.checks[2].min_count, 1);
        Ok(())
    }

    #[test]
    fn parses_eval_definition_with_typesafe_judge_config() -> anyhow::Result<()> {
        let definition = parse_definition(
            r#"
id: scored-alignment
task: Check alignment.
judge:
  backend: typesafe
  model: jev-latest
  api_key_env: SVDO_TYPESAFE_KEY
  url: https://typesafe.example/systemone
checks:
  - id: architecture
    type: judge
    standard: api-architecture
"#,
        )?;

        let judge = definition.judge.expect("missing judge config");
        assert_eq!(judge.backend, Some(EvalJudgeBackend::TypeSafe));
        assert_eq!(judge.model.as_deref(), Some("jev-latest"));
        assert_eq!(judge.api_key_env.as_deref(), Some("SVDO_TYPESAFE_KEY"));
        assert_eq!(
            judge.url.as_deref(),
            Some("https://typesafe.example/systemone")
        );
        Ok(())
    }

    #[test]
    fn rejects_eval_judge_config_without_backend() {
        let definition = EvalDefinition {
            id: "missing-backend".to_owned(),
            task: "Judge work".to_owned(),
            judge: Some(EvalJudgeConfig {
                backend: None,
                model: Some("jev-latest".to_owned()),
                api_key_env: None,
                url: None,
            }),
            checks: vec![CheckDefinition {
                id: "architecture".to_owned(),
                check_type: CheckType::Judge,
                command: None,
                required: false,
                weight: 1.0,
                min_score: None,
                standard: None,
                rubric: None,
                event_type: None,
                tool_name: None,
                min_count: DEFAULT_TELEMETRY_MIN_COUNT,
            }],
            threshold: 1.0,
            source_path: None,
        };

        let error = validate_definition(&definition, Path::new("eval.yaml"))
            .expect_err("missing judge backend should fail validation");

        assert!(
            error
                .to_string()
                .contains("judge config is missing backend")
        );
    }

    #[test]
    fn aggregates_weighted_scores_and_required_failures() {
        let definition = EvalDefinition {
            id: "sample".to_owned(),
            task: "Do work".to_owned(),
            judge: None,
            checks: Vec::new(),
            threshold: 0.8,
            source_path: None,
        };
        let result = aggregate_result(
            definition,
            vec![
                check_result("tests", true, 1.0, Some(1.0), CheckOutcome::Passed),
                check_result("lint", false, 3.0, Some(0.0), CheckOutcome::Failed),
            ],
            10,
        );

        assert_eq!(result.overall_score, 0.25);
        assert!(!result.required_failure);
        assert!(!result.passed);
    }

    #[test]
    fn required_failure_hard_fails_even_above_threshold() {
        let definition = EvalDefinition {
            id: "sample".to_owned(),
            task: "Do work".to_owned(),
            judge: None,
            checks: Vec::new(),
            threshold: 0.4,
            source_path: None,
        };
        let result = aggregate_result(
            definition,
            vec![
                check_result("required", true, 0.1, Some(0.0), CheckOutcome::Failed),
                check_result("optional", false, 0.9, Some(1.0), CheckOutcome::Passed),
            ],
            10,
        );

        assert!(result.overall_score > result.threshold);
        assert!(result.required_failure);
        assert!(!result.passed);
    }

    #[test]
    fn runs_command_checks() -> anyhow::Result<()> {
        let workspace = unique_temp_path("svdo-meter-eval-command");
        fs::create_dir_all(&workspace)?;
        let definition = EvalDefinition {
            id: "command".to_owned(),
            task: "Run command".to_owned(),
            judge: None,
            checks: vec![CheckDefinition {
                id: "shell".to_owned(),
                check_type: CheckType::Command,
                command: Some("printf ok".to_owned()),
                required: true,
                weight: 1.0,
                min_score: None,
                standard: None,
                rubric: None,
                event_type: None,
                tool_name: None,
                min_count: DEFAULT_TELEMETRY_MIN_COUNT,
            }],
            threshold: 1.0,
            source_path: None,
        };

        let result = run_definition(&workspace, definition, &JudgeConfig::default())?;

        assert!(result.passed);
        assert_eq!(result.checks[0].outcome, CheckOutcome::Passed);
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn skips_judge_checks_without_configured_command() -> anyhow::Result<()> {
        let workspace = unique_temp_path("svdo-meter-eval-judge-skip");
        fs::create_dir_all(&workspace)?;
        let definition = EvalDefinition {
            id: "judge".to_owned(),
            task: "Judge work".to_owned(),
            judge: None,
            checks: vec![CheckDefinition {
                id: "architecture".to_owned(),
                check_type: CheckType::Judge,
                command: None,
                required: false,
                weight: 1.0,
                min_score: None,
                standard: None,
                rubric: None,
                event_type: None,
                tool_name: None,
                min_count: DEFAULT_TELEMETRY_MIN_COUNT,
            }],
            threshold: 0.0,
            source_path: None,
        };

        let result = run_definition(&workspace, definition, &JudgeConfig::default())?;

        assert!(result.passed);
        assert_eq!(result.checks[0].outcome, CheckOutcome::Skipped);
        assert_eq!(result.checks[0].score, None);
        assert_eq!(
            result.checks[0].harness.as_deref(),
            Some("judge-unavailable")
        );
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn selects_typesafe_judge_backend_from_cli() -> anyhow::Result<()> {
        let config = JudgeConfig::from_cli(JudgeCliConfig {
            harness: None,
            model: None,
            command: None,
            args: Vec::new(),
            backend: Some(JudgeBackend::TypeSafe),
            typesafe_model: "jev-latest".to_owned(),
            typesafe_api_key_env: "SVDO_TYPESAFE_KEY".to_owned(),
            typesafe_url: "https://api.typesafe.ai/v1/systemone".to_owned(),
        })?;

        match config.runner {
            Some(JudgeRunner::TypeSafe(judge)) => {
                assert_eq!(judge.model, "jev-latest");
                assert_eq!(judge.api_key_env, "SVDO_TYPESAFE_KEY");
            }
            other => panic!("expected TypeSafe judge runner, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn eval_file_typesafe_judge_config_is_used_when_cli_omits_judge() -> anyhow::Result<()> {
        let definition = EvalDefinition {
            id: "typesafe-default".to_owned(),
            task: "Judge work".to_owned(),
            judge: Some(EvalJudgeConfig {
                backend: Some(EvalJudgeBackend::TypeSafe),
                model: Some("jev-custom".to_owned()),
                api_key_env: Some("SVDO_TYPESAFE_KEY".to_owned()),
                url: Some("https://typesafe.example/systemone".to_owned()),
            }),
            checks: Vec::new(),
            threshold: 1.0,
            source_path: None,
        };

        let config = JudgeConfig::default().for_definition(&definition)?;

        match config.runner {
            Some(JudgeRunner::TypeSafe(judge)) => {
                assert_eq!(judge.model, "jev-custom");
                assert_eq!(judge.api_key_env, "SVDO_TYPESAFE_KEY");
                assert_eq!(judge.endpoint, "https://typesafe.example/systemone");
            }
            other => panic!("expected TypeSafe judge runner, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn cli_judge_config_overrides_eval_file_judge_config() -> anyhow::Result<()> {
        let definition = EvalDefinition {
            id: "typesafe-default".to_owned(),
            task: "Judge work".to_owned(),
            judge: Some(EvalJudgeConfig {
                backend: Some(EvalJudgeBackend::TypeSafe),
                model: Some("jev-from-file".to_owned()),
                api_key_env: Some("SVDO_TYPESAFE_KEY".to_owned()),
                url: None,
            }),
            checks: Vec::new(),
            threshold: 1.0,
            source_path: None,
        };
        let cli_config = JudgeConfig::from_cli(JudgeCliConfig {
            harness: Some(HarnessKind::Codex),
            model: Some("gpt-5".to_owned()),
            command: None,
            args: Vec::new(),
            backend: None,
            typesafe_model: DEFAULT_TYPESAFE_MODEL.to_owned(),
            typesafe_api_key_env: DEFAULT_TYPESAFE_API_KEY_ENV.to_owned(),
            typesafe_url: DEFAULT_TYPESAFE_URL.to_owned(),
        })?;

        let config = cli_config.for_definition(&definition)?;

        match config.runner {
            Some(JudgeRunner::Codex(judge)) => {
                assert_eq!(judge.model.as_ref().map(ModelName::as_str), Some("gpt-5"));
            }
            other => panic!("expected CLI Codex judge runner, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn typesafe_request_uses_structured_score_questions() -> anyhow::Result<()> {
        let workspace = unique_temp_path("svdo-meter-eval-typesafe-request");
        fs::create_dir_all(workspace.join(".svdo").join("standards"))?;
        fs::write(
            workspace
                .join(".svdo")
                .join("standards")
                .join("api-architecture.md"),
            "# API Architecture\n\nUse clear boundaries.",
        )?;
        fs::write(
            workspace.join("calc.py"),
            "def add(left, right):\n    return left + right\n",
        )?;
        let definition = EvalDefinition {
            id: "typesafe".to_owned(),
            task: "Add account endpoint".to_owned(),
            judge: None,
            checks: Vec::new(),
            threshold: 1.0,
            source_path: None,
        };
        let judge = TypeSafeJudge {
            endpoint: "https://api.typesafe.ai/v1/systemone".to_owned(),
            model: "jev-latest".to_owned(),
            api_key_env: "SVDO_TYPESAFE_KEY".to_owned(),
        };
        let prepared = vec![PreparedJudgeCheck {
            check_id: "architecture".to_owned(),
            standard: Some("api-architecture".to_owned()),
            standard_path: Some(
                workspace
                    .join(".svdo")
                    .join("standards")
                    .join("api-architecture.md")
                    .display()
                    .to_string(),
            ),
            standard_contents: Some("# API Architecture\n\nUse clear boundaries.".to_owned()),
            rubric: Some(PreparedRubric {
                id: "architecture-alignment".to_owned(),
                path: "/tmp/architecture-alignment.yaml".to_owned(),
                instructions: "Use the architecture rubric.".to_owned(),
                criteria: vec![
                    "No alignment.".to_owned(),
                    "Partial alignment.".to_owned(),
                    "Full alignment.".to_owned(),
                ],
            }),
            criteria: vec![
                "No alignment.".to_owned(),
                "Partial alignment.".to_owned(),
                "Full alignment.".to_owned(),
            ],
        }];

        let request = build_typesafe_request(&workspace, &definition, &judge, &prepared)?;

        assert_eq!(request.model, "jev-latest");
        let question = request
            .questions
            .get("architecture")
            .expect("missing score question");
        assert_eq!(question.question_type, "score");
        assert_eq!(
            question.criteria,
            vec![
                "No alignment.".to_owned(),
                "Partial alignment.".to_owned(),
                "Full alignment.".to_owned(),
            ]
        );
        assert_eq!(question.instructions, "Use the architecture rubric.");
        assert_eq!(
            request
                .state
                .checks
                .get("architecture")
                .and_then(|check| check.standard_contents.as_deref()),
            Some("# API Architecture\n\nUse clear boundaries.")
        );
        assert_eq!(
            request
                .state
                .checks
                .get("architecture")
                .and_then(|check| check.rubric.as_ref())
                .map(|rubric| rubric.id.as_str()),
            Some("architecture-alignment")
        );
        assert_eq!(
            request
                .state
                .workspace_snapshot
                .source_files
                .get("calc.py")
                .map(String::as_str),
            Some("def add(left, right):\n    return left + right\n")
        );
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn typesafe_git_snapshot_is_scoped_to_workspace() -> anyhow::Result<()> {
        let repo = unique_temp_path("svdo-meter-eval-typesafe-git-scope");
        let workspace = repo.join("examples").join("typesafe-judge");
        fs::create_dir_all(&workspace)?;
        fs::create_dir_all(repo.join("docs"))?;
        fs::write(workspace.join("calc.py"), "print('old')\n")?;
        fs::write(repo.join("docs").join("outside.txt"), "outside\n")?;
        let init = Command::new("git")
            .arg("init")
            .arg("--quiet")
            .current_dir(&repo)
            .output()
            .context("failed to initialize test git repository")?;
        assert!(init.status.success());
        let add = Command::new("git")
            .arg("add")
            .arg("examples/typesafe-judge/calc.py")
            .current_dir(&repo)
            .output()
            .context("failed to stage workspace file")?;
        assert!(add.status.success());
        fs::write(workspace.join("calc.py"), "print('workspace')\n")?;

        let snapshot = collect_typesafe_workspace_snapshot(&workspace)?;

        let status = snapshot.status.unwrap_or_default();
        assert!(status.contains("calc.py"));
        assert!(!status.contains("outside.txt"));
        assert!(snapshot.source_files.contains_key("calc.py"));
        fs::remove_dir_all(repo)?;
        Ok(())
    }

    #[test]
    fn loads_typesafe_rubric_from_rubrics_directory() -> anyhow::Result<()> {
        let workspace = unique_temp_path("svdo-meter-eval-rubric-load");
        fs::create_dir_all(workspace.join(".svdo").join("rubrics"))?;
        fs::write(
            workspace
                .join(".svdo")
                .join("rubrics")
                .join("architecture-alignment.yaml"),
            r#"
instructions: |
  Evaluate architecture alignment against the referenced standard.
criteria:
  - No alignment.
  - Partial alignment.
  - Full alignment.
"#,
        )?;

        let rubric = load_typesafe_rubric(&workspace, "architecture-alignment")?;

        assert_eq!(rubric.id, "architecture-alignment");
        assert!(
            rubric
                .instructions
                .contains("Evaluate architecture alignment")
        );
        assert_eq!(
            rubric.criteria,
            vec![
                "No alignment.".to_owned(),
                "Partial alignment.".to_owned(),
                "Full alignment.".to_owned(),
            ]
        );
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn rejects_typesafe_rubric_with_too_few_criteria() -> anyhow::Result<()> {
        let workspace = unique_temp_path("svdo-meter-eval-rubric-invalid");
        fs::create_dir_all(workspace.join(".svdo").join("rubrics"))?;
        fs::write(
            workspace
                .join(".svdo")
                .join("rubrics")
                .join("too-short.yaml"),
            r#"
instructions: Evaluate alignment.
criteria:
  - Only one level.
"#,
        )?;

        let error = load_typesafe_rubric(&workspace, "too-short")
            .expect_err("rubric with one criterion should fail");

        assert!(error.to_string().contains("between 2 and 10"));
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn normalizes_typesafe_score_answer_with_metadata() -> anyhow::Result<()> {
        let check = CheckDefinition {
            id: "architecture".to_owned(),
            check_type: CheckType::Judge,
            command: None,
            required: true,
            weight: 0.5,
            min_score: None,
            standard: Some("api-architecture".to_owned()),
            rubric: Some("architecture-alignment".to_owned()),
            event_type: None,
            tool_name: None,
            min_count: DEFAULT_TELEMETRY_MIN_COUNT,
        };
        let prepared = PreparedJudgeCheck {
            check_id: "architecture".to_owned(),
            standard: check.standard.clone(),
            standard_path: Some("/tmp/std.md".to_owned()),
            standard_contents: Some("rubric".to_owned()),
            rubric: None,
            criteria: default_typesafe_criteria(),
        };
        let answer = TypeSafeScoreAnswer {
            answer_type: "score".to_owned(),
            score: 3.0,
            confidence: Some(0.72),
            probabilities: BTreeMap::from([("0".to_owned(), 0.0), ("3".to_owned(), 1.0)]),
            legend: BTreeMap::from([("3".to_owned(), Value::String("good".to_owned()))]),
        };
        let usage = TypeSafeUsage {
            input_tokens: Some(100),
            output_tokens: Some(12),
            cache_read_tokens: None,
            total_tokens: None,
        };

        let result = normalize_typesafe_check_result(
            &check,
            &prepared,
            &answer,
            Some("jev-latest"),
            Some(&usage),
            25,
            "jev-latest",
            0.75,
        )?;

        assert_eq!(result.score, Some(0.75));
        assert_eq!(result.outcome, CheckOutcome::Passed);
        assert_eq!(result.harness.as_deref(), Some("typesafe"));
        assert_eq!(result.model.as_deref(), Some("jev-latest"));
        assert_eq!(
            result.token_usage.as_ref().and_then(|usage| usage.total),
            Some(112)
        );
        let metadata = result.typesafe.expect("missing TypeSafe metadata");
        assert_eq!(metadata.provider, "typesafe");
        assert_eq!(metadata.raw_score, 3.0);
        assert_eq!(metadata.confidence, Some(0.72));
        assert_eq!(metadata.probabilities.get("3"), Some(&1.0));
        assert_eq!(
            metadata.legend.get("3"),
            Some(&Value::String("good".to_owned()))
        );
        assert_eq!(metadata.criteria.len(), 5);
        Ok(())
    }

    #[test]
    fn typesafe_partial_score_can_pass_weighted_eval() -> anyhow::Result<()> {
        let check = CheckDefinition {
            id: "implementation-quality".to_owned(),
            check_type: CheckType::Judge,
            command: None,
            required: true,
            weight: 0.4,
            min_score: Some(0.85),
            standard: Some("calculator-quality".to_owned()),
            rubric: Some("calculator-implementation".to_owned()),
            event_type: None,
            tool_name: None,
            min_count: DEFAULT_TELEMETRY_MIN_COUNT,
        };
        let prepared = PreparedJudgeCheck {
            check_id: check.id.clone(),
            standard: check.standard.clone(),
            standard_path: Some("/tmp/calculator-quality.md".to_owned()),
            standard_contents: Some("standard".to_owned()),
            rubric: None,
            criteria: vec![
                "No meaningful calculator implementation is present.".to_owned(),
                "Some operations exist, but major behavior is missing.".to_owned(),
                "All core operations mostly work.".to_owned(),
                "The CLI satisfies the task with minor issues.".to_owned(),
                "The implementation is complete and easy to test.".to_owned(),
            ],
        };
        let answer = TypeSafeScoreAnswer {
            answer_type: "score".to_owned(),
            score: 3.49,
            confidence: None,
            probabilities: BTreeMap::new(),
            legend: BTreeMap::new(),
        };

        let result = normalize_typesafe_check_result(
            &check,
            &prepared,
            &answer,
            Some("jev-1.13.0"),
            None,
            973,
            "jev-latest",
            0.9,
        )?;

        assert_eq!(result.outcome, CheckOutcome::Passed);
        assert_eq!(result.score, Some(0.8725));
        assert!(result.violations.is_empty());
        Ok(())
    }

    #[test]
    fn typesafe_low_nonzero_score_fails_check() -> anyhow::Result<()> {
        let check = CheckDefinition {
            id: "implementation-quality".to_owned(),
            check_type: CheckType::Judge,
            command: None,
            required: true,
            weight: 0.4,
            min_score: Some(0.85),
            standard: Some("calculator-quality".to_owned()),
            rubric: Some("calculator-implementation".to_owned()),
            event_type: None,
            tool_name: None,
            min_count: DEFAULT_TELEMETRY_MIN_COUNT,
        };
        let prepared = PreparedJudgeCheck {
            check_id: check.id.clone(),
            standard: check.standard.clone(),
            standard_path: Some("/tmp/calculator-quality.md".to_owned()),
            standard_contents: Some("standard".to_owned()),
            rubric: None,
            criteria: default_typesafe_criteria(),
        };
        let answer = TypeSafeScoreAnswer {
            answer_type: "score".to_owned(),
            score: 1.0,
            confidence: None,
            probabilities: BTreeMap::new(),
            legend: BTreeMap::new(),
        };

        let result = normalize_typesafe_check_result(
            &check, &prepared, &answer, None, None, 100, "jev", 0.5,
        )?;

        assert_eq!(result.score, Some(0.25));
        assert_eq!(result.outcome, CheckOutcome::Failed);
        assert!(result.violations[0].contains("is below passing threshold"));
        Ok(())
    }

    #[test]
    fn missing_typesafe_api_key_is_actionable() {
        let judge = TypeSafeJudge {
            endpoint: "https://api.typesafe.ai/v1/systemone".to_owned(),
            model: "jev-latest".to_owned(),
            api_key_env: "SVDO_TYPESAFE_KEY_DOES_NOT_EXIST_FOR_TEST".to_owned(),
        };
        let request = TypeSafeRequest {
            state: TypeSafeState {
                eval_id: "missing-key".to_owned(),
                task: "task".to_owned(),
                workspace: "/tmp".to_owned(),
                checks: BTreeMap::new(),
                workspace_snapshot: TypeSafeWorkspaceSnapshot {
                    status: None,
                    diff_stat: None,
                    diff: None,
                    source_files: BTreeMap::new(),
                    truncated: false,
                },
            },
            model: "jev-latest".to_owned(),
            questions: BTreeMap::new(),
        };

        let error = judge
            .run(&request)
            .expect_err("missing API key should fail");

        assert!(error.to_string().contains("requires an API key"));
        assert!(
            error
                .to_string()
                .contains("SVDO_TYPESAFE_KEY_DOES_NOT_EXIST_FOR_TEST")
        );
    }

    #[test]
    fn typesafe_blocking_http_is_safe_inside_tokio_runtime() -> anyhow::Result<()> {
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let judge = TypeSafeJudge {
                endpoint: "http://127.0.0.1:9/systemone".to_owned(),
                model: "jev-latest".to_owned(),
                api_key_env: "PATH".to_owned(),
            };
            let request = TypeSafeRequest {
                state: TypeSafeState {
                    eval_id: "tokio-runtime".to_owned(),
                    task: "task".to_owned(),
                    workspace: "/tmp".to_owned(),
                    checks: BTreeMap::new(),
                    workspace_snapshot: TypeSafeWorkspaceSnapshot {
                        status: None,
                        diff_stat: None,
                        diff: None,
                        source_files: BTreeMap::new(),
                        truncated: false,
                    },
                },
                model: "jev-latest".to_owned(),
                questions: BTreeMap::new(),
            };

            let error = judge
                .run(&request)
                .expect_err("unreachable endpoint should fail without panicking");

            assert!(error.to_string().contains("failed to call TypeSafe"));
        });
        Ok(())
    }

    #[test]
    fn typesafe_http_retries_retryable_statuses() -> anyhow::Result<()> {
        let mut responses = vec![
            (
                reqwest::StatusCode::TOO_MANY_REQUESTS,
                r#"{"error":"rate limited"}"#.to_owned(),
            ),
            (
                reqwest::StatusCode::OK,
                r#"{"model":"jev-latest","answers":{},"usage":{"input_tokens":1,"output_tokens":1}}"#
                    .to_owned(),
            ),
        ]
        .into_iter();

        let response = call_typesafe_endpoint_with_transport(2, Duration::ZERO, || {
            Ok(responses.next().expect("unexpected extra request"))
        })?;

        assert_eq!(response.model.as_deref(), Some("jev-latest"));
        Ok(())
    }

    #[test]
    fn typesafe_http_stops_after_max_attempts() -> anyhow::Result<()> {
        let mut responses = vec![
            (
                reqwest::StatusCode::from_u16(529)?,
                r#"{"error":"overloaded once"}"#.to_owned(),
            ),
            (
                reqwest::StatusCode::from_u16(529)?,
                r#"{"error":"overloaded twice"}"#.to_owned(),
            ),
        ]
        .into_iter();

        let error = call_typesafe_endpoint_with_transport(2, Duration::ZERO, || {
            Ok(responses.next().expect("unexpected extra request"))
        })
        .expect_err("retryable failures should stop at max attempts");

        assert!(error.to_string().contains("HTTP status 529"));
        assert!(error.to_string().contains("overloaded twice"));
        Ok(())
    }

    #[test]
    fn typesafe_provider_failure_fails_judge_check_without_aborting_eval() -> anyhow::Result<()> {
        let workspace = unique_temp_path("svdo-meter-eval-typesafe-provider-failure");
        fs::create_dir_all(&workspace)?;
        let definition = EvalDefinition {
            id: "typesafe-provider-failure".to_owned(),
            task: "Judge work".to_owned(),
            judge: Some(EvalJudgeConfig {
                backend: Some(EvalJudgeBackend::TypeSafe),
                model: Some("jev-latest".to_owned()),
                api_key_env: Some("SVDO_TYPESAFE_KEY_DOES_NOT_EXIST_FOR_TEST".to_owned()),
                url: Some("https://api.typesafe.ai/v1/systemone".to_owned()),
            }),
            checks: vec![
                CheckDefinition {
                    id: "tests".to_owned(),
                    check_type: CheckType::Command,
                    command: Some("printf ok".to_owned()),
                    required: true,
                    weight: 1.0,
                    min_score: None,
                    standard: None,
                    rubric: None,
                    event_type: None,
                    tool_name: None,
                    min_count: DEFAULT_TELEMETRY_MIN_COUNT,
                },
                CheckDefinition {
                    id: "architecture".to_owned(),
                    check_type: CheckType::Judge,
                    command: None,
                    required: false,
                    weight: 1.0,
                    min_score: None,
                    standard: None,
                    rubric: None,
                    event_type: None,
                    tool_name: None,
                    min_count: DEFAULT_TELEMETRY_MIN_COUNT,
                },
            ],
            threshold: 1.0,
            source_path: None,
        };

        let result = run_definition(&workspace, definition, &JudgeConfig::default())?;

        assert_eq!(result.checks[0].id, "tests");
        assert_eq!(result.checks[0].outcome, CheckOutcome::Passed);
        assert_eq!(result.checks[1].id, "architecture");
        assert_eq!(result.checks[1].outcome, CheckOutcome::Failed);
        assert_eq!(result.checks[1].harness.as_deref(), Some("typesafe"));
        assert!(
            result.checks[1].violations[0].contains("TypeSafe judge failed")
                && result.checks[1].violations[0].contains("requires an API key")
        );
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn passes_telemetry_check_for_latest_run_matching_tool_event() -> anyhow::Result<()> {
        let workspace = unique_temp_path("svdo-meter-eval-telemetry-pass");
        write_telemetry(
            &workspace,
            "run-a",
            &[
                telemetry_line("run-a", "2026-08-21T12:00:00Z", "tool.started", "read_file"),
                telemetry_line("run-a", "2026-08-21T12:00:01Z", "run.completed", ""),
            ],
        )?;
        write_telemetry(
            &workspace,
            "run-b",
            &[
                telemetry_line(
                    "run-b",
                    "2026-08-21T12:10:00Z",
                    "tool.started",
                    "apply_patch",
                ),
                telemetry_line(
                    "run-b",
                    "2026-08-21T12:10:01Z",
                    "tool.started",
                    "apply_patch",
                ),
                telemetry_line("run-b", "2026-08-21T12:10:02Z", "run.completed", ""),
            ],
        )?;
        let definition = telemetry_definition("apply_patch", 2);

        let result = run_definition(&workspace, definition, &JudgeConfig::default())?;

        assert!(result.passed);
        assert_eq!(result.checks[0].outcome, CheckOutcome::Passed);
        assert_eq!(result.checks[0].score, Some(1.0));
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn fails_telemetry_check_when_telemetry_is_missing() -> anyhow::Result<()> {
        let workspace = unique_temp_path("svdo-meter-eval-telemetry-missing");
        fs::create_dir_all(&workspace)?;
        let definition = telemetry_definition("apply_patch", 1);

        let result = run_definition(&workspace, definition, &JudgeConfig::default())?;

        assert!(!result.passed);
        assert_eq!(result.checks[0].outcome, CheckOutcome::Failed);
        assert!(
            result.checks[0].violations[0]
                .contains("telemetry was missing: no telemetry files found")
        );
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn fails_telemetry_check_when_count_is_below_minimum() -> anyhow::Result<()> {
        let workspace = unique_temp_path("svdo-meter-eval-telemetry-low-count");
        write_telemetry(
            &workspace,
            "run-a",
            &[
                telemetry_line(
                    "run-a",
                    "2026-08-21T12:00:00Z",
                    "tool.started",
                    "apply_patch",
                ),
                telemetry_line("run-a", "2026-08-21T12:00:01Z", "run.completed", ""),
            ],
        )?;
        let definition = telemetry_definition("apply_patch", 2);

        let result = run_definition(&workspace, definition, &JudgeConfig::default())?;

        assert!(!result.passed);
        assert_eq!(result.checks[0].outcome, CheckOutcome::Failed);
        assert!(
            result.checks[0].violations[0].contains("were below min_count: found 1, required 2")
        );
        fs::remove_dir_all(workspace)?;
        Ok(())
    }

    #[test]
    fn parses_codex_item_text_judge_response() -> anyhow::Result<()> {
        let response = parse_judge_response(
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"{\"score\":0.75,\"passed\":false,\"violations\":[\"needs tighter CLI output\"]}"}}"#,
            "cli-design",
        )?;

        assert_eq!(response.score, 0.75);
        assert_eq!(response.passed, Some(false));
        assert_eq!(response.violations, ["needs tighter CLI output"]);
        Ok(())
    }

    #[test]
    fn codex_judge_arguments_skip_git_repo_check() {
        let model = ModelName::new("gpt-5").unwrap_or_else(|err| panic!("{err}"));

        let args = codex_judge_argv(
            &CodexConfig::default(),
            Some(Path::new("/tmp/eval workspace")),
            Some(&model),
            "Judge this",
        );

        assert_eq!(
            args,
            vec![
                OsString::from("exec"),
                OsString::from("--json"),
                OsString::from("-C"),
                OsString::from("/tmp/eval workspace"),
                OsString::from("--model"),
                OsString::from("gpt-5"),
                OsString::from("--skip-git-repo-check"),
                OsString::from("Judge this"),
            ]
        );
    }

    #[test]
    fn invalid_judge_response_error_includes_check_name() {
        let error = parse_judge_response("not json", "cli-design")
            .expect_err("missing score should fail parsing");

        assert!(
            error
                .to_string()
                .contains("judge check `cli-design` must emit JSON with a numeric `score` field")
        );
    }

    #[test]
    fn renders_json_and_csv() -> anyhow::Result<()> {
        let report = EvalRunReport {
            passed: true,
            duration_ms: 12,
            results: vec![EvalResult {
                id: "sample".to_owned(),
                task: "Do work".to_owned(),
                source: None,
                overall_score: 1.0,
                threshold: 1.0,
                passed: true,
                required_failure: false,
                duration_ms: 12,
                checks: vec![check_result(
                    "tests",
                    true,
                    1.0,
                    Some(1.0),
                    CheckOutcome::Passed,
                )],
                violations: Vec::new(),
                token_usage: None,
                model: None,
                harness: None,
                session_id: None,
            }],
        };

        assert!(render(&report, ReportFormat::Json)?.contains("\"overall_score\""));
        assert!(render(&report, ReportFormat::Csv)?.contains("eval_id,eval_passed"));
        Ok(())
    }

    #[test]
    fn terminal_report_groups_failures_and_tails_noisy_output() -> anyhow::Result<()> {
        let noisy_output = (1..=50)
            .map(|index| format!("passing test line {index:02}"))
            .chain(["actual failure: expected 5 got 6".to_owned()])
            .collect::<Vec<_>>()
            .join("\n");
        let mut command = check_result("tests-pass", true, 0.4, Some(0.0), CheckOutcome::Failed);
        command.exit_code = Some(101);
        command.violations = vec!["command exited with status 101".to_owned()];
        command.output = Some(noisy_output);

        let mut judge = check_result(
            "alignment-review",
            true,
            0.3,
            Some(0.35),
            CheckOutcome::Failed,
        );
        judge.check_type = CheckType::Judge;
        judge.violations = vec![
            "generated standards were manually edited".to_owned(),
            "trailing newline was removed".to_owned(),
        ];

        let report = EvalRunReport {
            passed: false,
            duration_ms: 100,
            results: vec![EvalResult {
                id: "follows-agent-directions".to_owned(),
                task: "Evaluate instructions".to_owned(),
                source: None,
                overall_score: 0.41,
                threshold: 0.9,
                passed: false,
                required_failure: true,
                duration_ms: 100,
                checks: vec![
                    check_result(
                        "instructions-present",
                        true,
                        0.1,
                        Some(1.0),
                        CheckOutcome::Passed,
                    ),
                    command,
                    judge,
                ],
                violations: vec![
                    "command exited with status 101".to_owned(),
                    "generated standards were manually edited".to_owned(),
                    "trailing newline was removed".to_owned(),
                    "overall score 0.41 is below threshold 0.90".to_owned(),
                ],
                token_usage: None,
                model: None,
                harness: None,
                session_id: None,
            }],
        };

        let output = render(&report, ReportFormat::Terminal)?;

        assert!(output.contains("Failed checks: tests-pass, alignment-review"));
        assert!(output.contains("Failure Report:"));
        assert!(output.contains("tests-pass [command] score 0.00 required exit 101"));
        assert!(output.contains("alignment-review [judge] score 0.35 required"));
        assert!(output.contains("output: showing last 40 lines (11 earlier lines omitted)"));
        assert!(output.contains("actual failure: expected 5 got 6"));
        assert!(!output.contains("passing test line 01"));
        assert!(output.contains("Overall:"));
        assert!(output.contains("overall score 0.41 is below threshold 0.90"));
        Ok(())
    }

    #[test]
    fn repo_sample_eval_definitions_are_valid() -> anyhow::Result<()> {
        let samples = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(".svdo")
            .join("evals");
        if samples.exists() {
            let definitions = load_definitions(&samples)?;
            assert!(!definitions.is_empty());
        }
        Ok(())
    }

    fn check_result(
        id: &str,
        required: bool,
        weight: f64,
        score: Option<f64>,
        outcome: CheckOutcome,
    ) -> CheckResult {
        CheckResult {
            id: id.to_owned(),
            check_type: CheckType::Command,
            outcome,
            required,
            weight,
            score,
            duration_ms: 0,
            command: None,
            exit_code: None,
            standard: None,
            standard_path: None,
            violations: Vec::new(),
            output: None,
            token_usage: None,
            model: None,
            harness: None,
            session_id: None,
            typesafe: None,
        }
    }

    fn telemetry_definition(tool_name: &str, min_count: u64) -> EvalDefinition {
        EvalDefinition {
            id: "telemetry".to_owned(),
            task: "Check telemetry".to_owned(),
            judge: None,
            checks: vec![CheckDefinition {
                id: "requires-tool".to_owned(),
                check_type: CheckType::Telemetry,
                command: None,
                required: true,
                weight: 1.0,
                min_score: None,
                standard: None,
                rubric: None,
                event_type: Some("tool.started".to_owned()),
                tool_name: Some(tool_name.to_owned()),
                min_count,
            }],
            threshold: 1.0,
            source_path: None,
        }
    }

    fn write_telemetry(workspace: &Path, run_id: &str, lines: &[String]) -> anyhow::Result<()> {
        let telemetry_dir = workspace.join(".svdo").join("meter");
        fs::create_dir_all(&telemetry_dir)?;
        fs::write(
            telemetry_dir.join(format!("{run_id}.jsonl")),
            format!("{}\n", lines.join("\n")),
        )?;
        Ok(())
    }

    fn telemetry_line(
        run_id: &str,
        occurred_at: &str,
        event_type: &str,
        tool_name: &str,
    ) -> String {
        let payload = match event_type {
            "tool.started" => {
                format!(
                    r#"{{"type":"tool_started","data":{{"tool_id":"tool-1","tool_name":"{tool_name}"}}}}"#
                )
            }
            "run.completed" => r#"{"type":"run_completed","data":{"metrics":{"wall_time_ms":1000,"active_time_ms":0,"command_time_ms":0,"tool_time_ms":0,"turn_count":1,"provider_event_count":1,"commands_executed":0,"failed_commands":0,"files_changed":0,"tool_calls":1,"errors":0,"token_usage":{}},"exit_code":0}}"#.to_owned(),
            other => format!(r#"{{"type":"harness_event","data":{{"source_event":"{other}","retained_raw_payload":false}}}}"#),
        };
        format!(
            r#"{{"schema_version":1,"event_id":"018f6f1b-97f1-7c04-9a96-aaaaaaaaaaaa","event_type":"{event_type}","occurred_at":"{occurred_at}","observed_at":"{occurred_at}","run_id":"{run_id}","ticket_id":"ENG-142","harness":"codex","payload":{payload}}}"#
        )
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("{}-{nanos}-{name}", std::process::id()))
    }
}
