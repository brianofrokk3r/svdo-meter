use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use meter_core::HarnessKind;
use meter_report::PricingConfig;

#[derive(Debug, Parser)]
#[command(name = "svdo-meter")]
#[command(about = "Thin telemetry harness for agentic coding CLI sessions")]
#[command(long_about = "Run an agent CLI with ticket telemetry and local event storage.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(about = "Run measured agent CLI work")]
    #[command(
        after_help = "Examples:\n  svdo-meter run --ticket ENG-142 --harness codex PROMPT\n  svdo-meter run --ticket ENG-142 --harness codex --prompt-file prompt.txt"
    )]
    Run(RunArgs),
    #[command(about = "Generate a local SVDO Trace report from JSONL telemetry")]
    #[command(
        after_help = "Examples:\n  svdo-meter report ENG-142\n  svdo-meter report --last 7d\n  svdo-meter report --label plan\n  svdo-meter report ENG-142 --format json\n  svdo-meter report --last 7d --format csv\n  svdo-meter report --pricing-file pricing.json\n\nReports read .svdo/meter.jsonl under --workspace or the current directory. Without WORK, output is grouped by work identifier; records without one are shown as Unknown. Token output preserves input, output, cache, and total fields, with missing components shown distinctly from zero. Pricing rates are specified as cost per 1,000,000 tokens."
    )]
    Report(ReportArgs),
    #[command(about = "Inspect local SVDO Meter telemetry")]
    #[command(
        after_help = "Examples:\n  svdo-meter telemetry sessions\n  svdo-meter telemetry runs\n  svdo-meter telemetry inspect 018f6f1b-97f1-7c04-9a96-111111111111\n  svdo-meter telemetry inspect sess-abc123\n\nTelemetry inspection reads .svdo/meter.jsonl under --workspace or the current directory. Malformed JSONL lines are reported as diagnostics while valid records remain inspectable."
    )]
    Telemetry(TelemetryArgs),
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// External ticket/work identifier used as the telemetry join key.
    #[arg(long)]
    pub ticket: String,

    /// Optional human-readable label recorded on run events.
    #[arg(long)]
    pub label: Option<String>,

    /// Agent CLI harness to execute. v0.1 supports `codex`.
    #[arg(long)]
    pub harness: HarnessKind,

    /// Workspace directory passed to the harness and used for local telemetry.
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// Provider session/thread ID to resume instead of auto-discovery.
    #[arg(long)]
    pub session: Option<String>,

    /// Harness-specific model selection. For Codex this is passed to Codex.
    #[arg(long)]
    pub model: Option<String>,

    /// Read the agent prompt from a UTF-8 text file.
    #[arg(long, value_name = "PATH", conflicts_with = "prompt")]
    pub prompt_file: Option<PathBuf>,

    /// Prompt or work instruction forwarded to the selected harness.
    #[arg(
        value_name = "PROMPT",
        required_unless_present = "prompt_file",
        conflicts_with = "prompt_file"
    )]
    pub prompt: Option<String>,
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Optional work identifier. When omitted, results are grouped by work identifier.
    #[arg(value_name = "WORK")]
    pub work: Option<String>,

    /// Workspace containing `.svdo/meter.jsonl`. Defaults to the current directory.
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// Include only telemetry observed within a recent duration such as `7d`, `12h`, or `30m`.
    #[arg(long, value_name = "DURATION")]
    pub last: Option<ReportDuration>,

    /// Include only telemetry records with this label.
    #[arg(long)]
    pub label: Option<String>,

    /// Output format for the report.
    #[arg(long, default_value_t = ReportFormat::Terminal)]
    pub format: ReportFormat,

    /// Read a JSON model pricing map from a UTF-8 file. Rates are per 1,000,000 tokens.
    #[arg(long, value_name = "PATH")]
    pub pricing_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct TelemetryArgs {
    /// Workspace containing `.svdo/meter.jsonl`. Defaults to the current directory.
    #[arg(long, global = true)]
    pub workspace: Option<PathBuf>,

    #[command(subcommand)]
    pub command: TelemetryCommands,
}

#[derive(Debug, Subcommand)]
pub enum TelemetryCommands {
    /// List discovered local telemetry sessions.
    Sessions,
    /// List telemetry runs and work associations.
    Runs,
    /// Inspect ordered telemetry events for a run or session identifier.
    Inspect(TelemetryInspectArgs),
}

#[derive(Debug, Args)]
pub struct TelemetryInspectArgs {
    /// Run identifier or provider session identifier to inspect.
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Terminal,
    Json,
    Csv,
}

impl std::fmt::Display for ReportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Terminal => "terminal",
            Self::Json => "json",
            Self::Csv => "csv",
        };
        f.write_str(value)
    }
}

impl FromStr for ReportFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "terminal" => Ok(Self::Terminal),
            "json" => Ok(Self::Json),
            "csv" => Ok(Self::Csv),
            other => Err(format!(
                "unsupported report format `{other}`; expected terminal, json, or csv"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportDuration(Duration);

impl ReportDuration {
    pub fn as_duration(self) -> Duration {
        self.0
    }
}

impl FromStr for ReportDuration {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some(unit) = value.chars().last() else {
            return Err("duration cannot be empty".to_owned());
        };
        let number = &value[..value.len().saturating_sub(unit.len_utf8())];
        let amount = number
            .parse::<u64>()
            .map_err(|_| "duration must start with a whole number".to_owned())?;
        if amount == 0 {
            return Err("duration must be greater than zero".to_owned());
        }
        let seconds = match unit {
            'd' => amount.saturating_mul(24 * 60 * 60),
            'h' => amount.saturating_mul(60 * 60),
            'm' => amount.saturating_mul(60),
            's' => amount,
            _ => {
                return Err(
                    "duration must end with one of: d for days, h for hours, m for minutes, s for seconds"
                        .to_owned(),
                );
            }
        };
        Ok(Self(Duration::from_secs(seconds)))
    }
}

pub fn resolve_prompt(args: &RunArgs) -> anyhow::Result<String> {
    if let Some(prompt) = &args.prompt {
        return Ok(prompt.clone());
    }
    if let Some(path) = &args.prompt_file {
        return std::fs::read_to_string(path)
            .with_context(|| format!("failed to read prompt file `{}`", path.display()));
    }
    unreachable!("clap requires either an inline prompt or --prompt-file")
}

pub fn resolve_pricing(args: &ReportArgs) -> anyhow::Result<Option<PricingConfig>> {
    if let Some(path) = &args.pricing_file {
        let value = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read pricing file `{}`", path.display()))?;
        return serde_json::from_str(&value).with_context(|| {
            format!(
                "failed to parse pricing file `{}` as a model pricing map keyed by model identifier",
                path.display()
            )
        }).map(Some);
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Context;
    use clap::{CommandFactory, Parser, error::ErrorKind};
    use meter_core::HarnessKind;

    use super::{Cli, Commands, ReportFormat, TelemetryCommands, resolve_pricing, resolve_prompt};

    #[test]
    fn parses_run_with_inline_prompt_and_all_optional_flags() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from([
            "svdo-meter",
            "run",
            "--ticket",
            "ENG-142",
            "--label",
            "Implement auth",
            "--harness",
            "codex",
            "--workspace",
            "/tmp/workspace",
            "--session",
            "sess-123",
            "--model",
            "gpt-5",
            "Do work",
        ])?;
        let args = match cli.command {
            Commands::Run(args) => args,
            Commands::Report(_) => panic!("expected run command"),
            Commands::Telemetry(_) => panic!("expected run command"),
        };

        assert_eq!(args.ticket, "ENG-142");
        assert_eq!(args.label.as_deref(), Some("Implement auth"));
        assert_eq!(args.harness, HarnessKind::Codex);
        assert_eq!(
            args.workspace.as_deref(),
            Some(std::path::Path::new("/tmp/workspace"))
        );
        assert_eq!(args.session.as_deref(), Some("sess-123"));
        assert_eq!(args.model.as_deref(), Some("gpt-5"));
        assert_eq!(resolve_prompt(&args)?, "Do work");
        Ok(())
    }

    #[test]
    fn resolves_prompt_file_contents() -> anyhow::Result<()> {
        let path = unique_temp_path("svdo-meter-prompt-success.txt");
        fs::write(&path, "Review the diff\nThen run tests\n")?;
        let cli = Cli::try_parse_from([
            "svdo-meter",
            "run",
            "--ticket",
            "ENG-142",
            "--harness",
            "codex",
            "--prompt-file",
            path.to_str()
                .context("temporary prompt path must be UTF-8")?,
        ])?;
        let args = match cli.command {
            Commands::Run(args) => args,
            Commands::Report(_) => panic!("expected run command"),
            Commands::Telemetry(_) => panic!("expected run command"),
        };

        assert_eq!(resolve_prompt(&args)?, "Review the diff\nThen run tests\n");
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn missing_prompt_file_reports_path() -> anyhow::Result<()> {
        let path = unique_temp_path("svdo-meter-missing-prompt.txt");
        let cli = Cli::try_parse_from([
            "svdo-meter",
            "run",
            "--ticket",
            "ENG-142",
            "--harness",
            "codex",
            "--prompt-file",
            path.to_str()
                .context("temporary prompt path must be UTF-8")?,
        ])?;
        let args = match cli.command {
            Commands::Run(args) => args,
            Commands::Report(_) => panic!("expected run command"),
            Commands::Telemetry(_) => panic!("expected run command"),
        };

        let error = resolve_prompt(&args)
            .err()
            .context("expected read failure")?;
        let message = format!("{error:#}");
        assert!(message.contains("failed to read prompt file"));
        assert!(message.contains(&path.display().to_string()));
        Ok(())
    }

    #[test]
    fn rejects_inline_prompt_with_prompt_file() -> anyhow::Result<()> {
        let path = unique_temp_path("svdo-meter-conflict-prompt.txt");
        let result = Cli::try_parse_from([
            "svdo-meter",
            "run",
            "--ticket",
            "ENG-142",
            "--harness",
            "codex",
            "--prompt-file",
            path.to_str()
                .context("temporary prompt path must be UTF-8")?,
            "Do work",
        ]);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn rejects_missing_prompt_source() {
        let result = Cli::try_parse_from([
            "svdo-meter",
            "run",
            "--ticket",
            "ENG-142",
            "--harness",
            "codex",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_missing_run_ticket() {
        let result = Cli::try_parse_from(["svdo-meter", "run", "--harness", "codex", "Do work"]);

        assert_eq!(parse_error_kind(result), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn rejects_missing_run_harness() {
        let result = Cli::try_parse_from(["svdo-meter", "run", "--ticket", "ENG-142", "Do work"]);

        assert_eq!(parse_error_kind(result), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn rejects_invalid_harness() {
        let result = Cli::try_parse_from([
            "svdo-meter",
            "run",
            "--ticket",
            "ENG-142",
            "--harness",
            "unknown",
            "Do work",
        ]);

        assert_eq!(parse_error_kind(result), ErrorKind::ValueValidation);
    }

    #[test]
    fn parses_report_default_terminal_format_and_workspace() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from([
            "svdo-meter",
            "report",
            "ENG-142",
            "--workspace",
            "/tmp/workspace",
        ])?;
        let args = match cli.command {
            Commands::Report(args) => args,
            Commands::Run(_) => panic!("expected report command"),
            Commands::Telemetry(_) => panic!("expected report command"),
        };

        assert_eq!(args.work.as_deref(), Some("ENG-142"));
        assert_eq!(
            args.workspace.as_deref(),
            Some(std::path::Path::new("/tmp/workspace"))
        );
        assert_eq!(args.format, ReportFormat::Terminal);
        Ok(())
    }

    #[test]
    fn parses_report_work_and_json_format() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["svdo-meter", "report", "ENG-142", "--format", "json"])?;
        let args = match cli.command {
            Commands::Report(args) => args,
            Commands::Run(_) => panic!("expected report command"),
            Commands::Telemetry(_) => panic!("expected report command"),
        };

        assert_eq!(args.work.as_deref(), Some("ENG-142"));
        assert_eq!(args.format, ReportFormat::Json);
        Ok(())
    }

    #[test]
    fn parses_report_csv_format() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["svdo-meter", "report", "--format", "csv"])?;
        let args = match cli.command {
            Commands::Report(args) => args,
            Commands::Run(_) => panic!("expected report command"),
            Commands::Telemetry(_) => panic!("expected report command"),
        };

        assert_eq!(args.work, None);
        assert_eq!(args.format, ReportFormat::Csv);
        Ok(())
    }

    #[test]
    fn parses_report_last_duration_and_label() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["svdo-meter", "report", "--last", "7d", "--label", "plan"])?;
        let args = match cli.command {
            Commands::Report(args) => args,
            Commands::Run(_) => panic!("expected report command"),
            Commands::Telemetry(_) => panic!("expected report command"),
        };

        assert_eq!(args.work, None);
        assert_eq!(args.label.as_deref(), Some("plan"));
        assert_eq!(
            args.last.map(|duration| duration.as_duration().as_secs()),
            Some(604_800)
        );
        Ok(())
    }

    #[test]
    fn resolves_report_pricing_file_contents() -> anyhow::Result<()> {
        let path = unique_temp_path("svdo-meter-pricing.json");
        fs::write(
            &path,
            r#"{"gpt-5-mini":{"input_per_million":0.25,"cached_input_per_million":0.025,"output_per_million":2.0}}"#,
        )?;
        let cli = Cli::try_parse_from([
            "svdo-meter",
            "report",
            "--pricing-file",
            path.to_str()
                .context("temporary pricing path must be UTF-8")?,
        ])?;
        let args = match cli.command {
            Commands::Report(args) => args,
            Commands::Run(_) => panic!("expected report command"),
            Commands::Telemetry(_) => panic!("expected report command"),
        };

        let pricing = resolve_pricing(&args)?.context("expected pricing")?;
        let model = pricing
            .get("gpt-5-mini")
            .context("expected gpt-5-mini pricing")?;
        assert_eq!(model.input_per_million, Some(0.25));
        assert_eq!(model.cached_input_per_million, Some(0.025));
        assert_eq!(model.output_per_million, Some(2.0));
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn report_help_documents_pricing_unit() {
        let mut command = Cli::command();
        let report = command
            .find_subcommand_mut("report")
            .expect("report subcommand exists");
        let help = report.render_help().to_string();

        assert!(help.contains("--pricing-file"));
        assert!(help.contains("per 1,000,000 tokens"));
    }

    #[test]
    fn rejects_inline_pricing_json_option() {
        let result = Cli::try_parse_from(["svdo-meter", "report", "--pricing-json", "{}"]);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_report_format() {
        let result = Cli::try_parse_from(["svdo-meter", "report", "--format", "yaml"]);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_report_durations() {
        for value in ["", "0d", "seven-days", "7w"] {
            let result = Cli::try_parse_from(["svdo-meter", "report", "--last", value]);

            assert!(result.is_err(), "expected `{value}` to be rejected");
        }
    }

    #[test]
    fn parses_telemetry_sessions_with_workspace() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from([
            "svdo-meter",
            "telemetry",
            "sessions",
            "--workspace",
            "/tmp/workspace",
        ])?;
        match cli.command {
            Commands::Telemetry(args) => match args.command {
                TelemetryCommands::Sessions => assert_eq!(
                    args.workspace.as_deref(),
                    Some(std::path::Path::new("/tmp/workspace"))
                ),
                _ => panic!("expected sessions command"),
            },
            _ => panic!("expected telemetry command"),
        }
        Ok(())
    }

    #[test]
    fn parses_telemetry_runs() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["svdo-meter", "telemetry", "runs"])?;
        match cli.command {
            Commands::Telemetry(args) => match args.command {
                TelemetryCommands::Runs => assert_eq!(args.workspace, None),
                _ => panic!("expected runs command"),
            },
            _ => panic!("expected telemetry command"),
        }
        Ok(())
    }

    #[test]
    fn parses_telemetry_inspect_id() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["svdo-meter", "telemetry", "inspect", "sess-1"])?;
        match cli.command {
            Commands::Telemetry(args) => match args.command {
                TelemetryCommands::Inspect(args) => assert_eq!(args.id, "sess-1"),
                _ => panic!("expected inspect command"),
            },
            _ => panic!("expected telemetry command"),
        }
        Ok(())
    }

    #[test]
    fn rejects_missing_top_level_command() {
        let result = Cli::try_parse_from(["svdo-meter"]);

        assert_eq!(
            parse_error_kind(result),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn rejects_unknown_top_level_command() {
        let result = Cli::try_parse_from(["svdo-meter", "export"]);

        assert_eq!(parse_error_kind(result), ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn rejects_missing_telemetry_subcommand() {
        let result = Cli::try_parse_from(["svdo-meter", "telemetry"]);

        assert_eq!(
            parse_error_kind(result),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn rejects_unknown_telemetry_subcommand() {
        let result = Cli::try_parse_from(["svdo-meter", "telemetry", "unknown"]);

        assert_eq!(parse_error_kind(result), ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn rejects_missing_telemetry_inspect_id() {
        let result = Cli::try_parse_from(["svdo-meter", "telemetry", "inspect"]);

        assert_eq!(parse_error_kind(result), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn help_documents_current_command_wiring() {
        let help = Cli::command().render_help().to_string();

        assert!(help.contains("run"));
        assert!(help.contains("report"));
        assert!(help.contains("telemetry"));
        assert!(help.contains("Run measured agent CLI work"));
        assert!(help.contains("Generate a local SVDO Trace report"));
        assert!(help.contains("Inspect local SVDO Meter telemetry"));
    }

    #[test]
    fn command_help_documents_nested_paths() {
        assert_help_contains(["svdo-meter", "run", "--help"], "svdo-meter run --ticket");
        assert_help_contains(
            ["svdo-meter", "report", "--help"],
            "svdo-meter report --last 7d",
        );
        assert_help_contains(["svdo-meter", "telemetry", "--help"], "sessions");
        assert_help_contains(["svdo-meter", "telemetry", "--help"], "runs");
        assert_help_contains(["svdo-meter", "telemetry", "--help"], "inspect");
        assert_help_contains(
            ["svdo-meter", "telemetry", "inspect", "--help"],
            "Run identifier or provider session identifier",
        );
    }

    fn parse_error_kind<T>(result: Result<T, clap::Error>) -> ErrorKind {
        match result {
            Ok(_) => panic!("expected parse error"),
            Err(error) => error.kind(),
        }
    }

    fn assert_help_contains<const N: usize>(args: [&str; N], expected: &str) {
        let result = Cli::try_parse_from(args);
        let error = match result {
            Ok(_) => panic!("expected help output"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), ErrorKind::DisplayHelp);
        assert!(
            error.to_string().contains(expected),
            "help output did not contain `{expected}`:\n{error}"
        );
    }

    fn unique_temp_path(file_name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("{nanos}-{file_name}"))
    }
}
