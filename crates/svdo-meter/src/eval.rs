use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde::Serialize;

use crate::cli::ReportFormat;

#[derive(Debug, Clone)]
pub struct EvalDefinition {
    pub id: String,
    pub task: String,
    pub checks: Vec<CheckDefinition>,
    pub threshold: f64,
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CheckDefinition {
    pub id: String,
    pub check_type: CheckType,
    pub command: Option<String>,
    pub required: bool,
    pub weight: f64,
    pub standard: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckType {
    Command,
    Judge,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckOutcome {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
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

pub fn run(workspace: &Path, requested_eval: Option<&str>) -> anyhow::Result<EvalRunReport> {
    let started = Instant::now();
    let definitions = load_requested_definitions(workspace, requested_eval)?;
    let mut results = Vec::with_capacity(definitions.len());
    for definition in definitions {
        results.push(run_definition(workspace, definition)?);
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

pub fn run_definition(workspace: &Path, definition: EvalDefinition) -> anyhow::Result<EvalResult> {
    let started = Instant::now();
    let mut checks = Vec::with_capacity(definition.checks.len());
    for check in &definition.checks {
        checks.push(run_check(workspace, check)?);
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

fn run_check(workspace: &Path, check: &CheckDefinition) -> anyhow::Result<CheckResult> {
    match check.check_type {
        CheckType::Command => run_command_check(workspace, check),
        CheckType::Judge => run_judge_check(workspace, check),
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
    })
}

fn run_judge_check(workspace: &Path, check: &CheckDefinition) -> anyhow::Result<CheckResult> {
    let started = Instant::now();
    let standard_path = check
        .standard
        .as_deref()
        .map(|standard| resolve_standard(workspace, standard))
        .transpose()?;
    Ok(CheckResult {
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
    })
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
        if check.check_type == CheckType::Command
            && check.command.as_deref().unwrap_or("").trim().is_empty()
        {
            bail!(
                "eval definition `{}` command check `{}` is missing command",
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
        checks,
        threshold,
        source_path: None,
    })
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
                standard: None,
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
        "standard" => check.standard = Some(value.to_owned()),
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
            output.push('\n');
            for violation in &check.violations {
                output.push_str(&format!("    reason: {violation}\n"));
            }
            if let Some(output_value) = &check.output {
                output.push_str(&format!("    output: {output_value}\n"));
            }
        }
        if !result.violations.is_empty() {
            output.push_str("  Violations:\n");
            for violation in &result.violations {
                output.push_str(&format!("    - {violation}\n"));
            }
        }
    }
    output
}

fn render_csv(report: &EvalRunReport) -> String {
    let mut output = "eval_id,eval_passed,overall_score,threshold,eval_duration_ms,check_id,check_type,check_outcome,required,weight,score,check_duration_ms,exit_code,violations\n".to_owned();
    for result in &report.results {
        for check in &result.checks {
            output.push_str(&format!(
                "{},{},{:.4},{:.4},{},{},{},{},{},{:.4},{},{},{},{}\n",
                csv(&result.id),
                result.passed,
                result.overall_score,
                result.threshold,
                result.duration_ms,
                csv(&check.id),
                check_type_label(check.check_type),
                outcome_label(check.outcome),
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
    truncate(&value, 4_000)
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
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
    weight: 0.4
threshold: 0.85
"#,
        )?;

        assert_eq!(definition.id, "add-account-endpoint");
        assert_eq!(definition.threshold, 0.85);
        assert_eq!(definition.checks.len(), 2);
        assert!(definition.checks[0].required);
        assert_eq!(definition.checks[1].check_type, CheckType::Judge);
        assert_eq!(definition.checks[1].weight, 0.4);
        Ok(())
    }

    #[test]
    fn aggregates_weighted_scores_and_required_failures() {
        let definition = EvalDefinition {
            id: "sample".to_owned(),
            task: "Do work".to_owned(),
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
            checks: vec![CheckDefinition {
                id: "shell".to_owned(),
                check_type: CheckType::Command,
                command: Some("printf ok".to_owned()),
                required: true,
                weight: 1.0,
                standard: None,
            }],
            threshold: 1.0,
            source_path: None,
        };

        let result = run_definition(&workspace, definition)?;

        assert!(result.passed);
        assert_eq!(result.checks[0].outcome, CheckOutcome::Passed);
        fs::remove_dir_all(workspace)?;
        Ok(())
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
    fn repo_sample_eval_definitions_are_valid() -> anyhow::Result<()> {
        let samples = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(".svdo")
            .join("evals");
        if samples.exists() {
            let definitions = load_definitions(&samples)?;
            assert_eq!(definitions.len(), 5);
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
        }
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("{}-{nanos}-{name}", std::process::id()))
    }
}
