mod cli;
mod config;
mod wiring;

use anyhow::Context;
use chrono::{Duration as ChronoDuration, Utc};
use clap::Parser;
use cli::{Cli, Commands, ReportFormat, TelemetryCommands};
use meter_core::{ModelName, SessionId, TicketId};
use meter_engine::RunRequest;
use meter_report::{
    ReportQuery, render_csv, render_inspection, render_json, render_runs, render_sessions,
    render_terminal,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Run(args) => {
            let prompt = cli::resolve_prompt(&args)?;
            let ticket_id = TicketId::new(args.ticket).context("invalid --ticket value")?;
            let session_override = args
                .session
                .map(SessionId::new)
                .transpose()
                .context("invalid --session value")?;
            let model = args
                .model
                .map(ModelName::new)
                .transpose()
                .context("invalid --model value")?;
            let harness_config = config::harness_config(args.harness, model);
            let engine = wiring::engine(&args.workspace, args.harness, &harness_config.config);
            let outcome = engine
                .run(RunRequest {
                    ticket_id,
                    label: args.label,
                    harness: args.harness,
                    workspace: args.workspace,
                    session_override,
                    model: harness_config.model,
                    raw_event_retention: harness_config.raw_event_retention,
                    options: harness_config.options,
                    prompt,
                })
                .await?;
            if !outcome.success {
                std::process::exit(outcome.exit_code.unwrap_or(1));
            }
        }
        Commands::Report(args) => {
            let since = args
                .last
                .map(|duration| {
                    ChronoDuration::from_std(duration.as_duration())
                        .map(|duration| Utc::now() - duration)
                })
                .transpose()
                .context("failed to convert --last duration")?;
            let query = ReportQuery {
                work: args.work,
                label: args.label,
                since,
            };
            let telemetry_path = wiring::default_telemetry_path(&args.workspace);
            let report = wiring::load_report(&telemetry_path, &query).with_context(|| {
                format!(
                    "failed to read telemetry from `{}`",
                    telemetry_path.display()
                )
            })?;
            let output = match args.format {
                ReportFormat::Terminal => render_terminal(&report),
                ReportFormat::Json => {
                    render_json(&report).context("failed to render JSON report")?
                }
                ReportFormat::Csv => render_csv(&report),
            };
            println!("{output}");
        }
        Commands::Telemetry(args) => {
            let telemetry_path = wiring::default_telemetry_path(&args.workspace);
            let inspection =
                wiring::load_telemetry_inspection(&telemetry_path).with_context(|| {
                    format!(
                        "failed to read telemetry from `{}`",
                        telemetry_path.display()
                    )
                })?;
            let output = match args.command {
                TelemetryCommands::Sessions => render_sessions(&inspection),
                TelemetryCommands::Runs => render_runs(&inspection),
                TelemetryCommands::Inspect(args) => render_inspection(&inspection, &args.id),
            };
            println!("{output}");
        }
    }
    Ok(())
}
