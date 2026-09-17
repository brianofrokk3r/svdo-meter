mod cli;
mod config;
mod eval;
mod wiring;

use anyhow::Context;
use chrono::{Duration as ChronoDuration, Utc};
use clap::Parser;
use cli::{Cli, Commands, EvalCommands, ReportFormat, TelemetryCommands};
use meter_core::{ModelName, SessionId, TicketId};
use meter_engine::RunRequest;
use meter_report::{
    ComparisonDiscoveryQuery, ComparisonQuery, ComparisonReducer, ReportQuery,
    render_comparison_terminal, render_csv, render_inspection, render_json, render_runs,
    render_sessions, render_terminal,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Compare(args) => {
            let workspace = args.workspace.unwrap_or(std::env::current_dir()?);
            let since = args
                .since
                .map(|duration| {
                    ChronoDuration::from_std(duration.as_duration())
                        .map(|duration| Utc::now() - duration)
                })
                .transpose()
                .context("failed to convert --since duration")?;
            let query = ComparisonDiscoveryQuery {
                work: args.work,
                harnesses: args
                    .harnesses
                    .into_iter()
                    .map(|harness| harness.to_string())
                    .collect(),
                models: args.models,
                since,
            };
            let telemetry_path = wiring::default_telemetry_path(&Some(workspace));
            let report =
                wiring::load_comparison_discovery(&telemetry_path, &query).with_context(|| {
                    format!(
                        "failed to read telemetry from `{}`",
                        telemetry_path.display()
                    )
                })?;
            let mut reducer = ComparisonReducer::new(ComparisonQuery {
                work: query.work,
                harnesses: query.harnesses,
                models: query.models,
            });
            reducer.extend(report.runs);
            reducer.add_diagnostics(report.diagnostics);
            println!("{}", render_comparison_terminal(&reducer.finish()));
        }
        Commands::Eval(args) => match args.command {
            EvalCommands::Run(args) => {
                let workspace = args.workspace.unwrap_or(std::env::current_dir()?);
                let judge_config = eval::JudgeConfig::from_cli(
                    args.harness,
                    args.model,
                    args.judge_command,
                    args.judge_args,
                    args.judge_backend,
                    args.typesafe_model,
                    args.typesafe_api_key_env,
                    args.typesafe_url,
                )?;
                let report = eval::run(&workspace, args.eval.as_deref(), &judge_config)
                    .with_context(|| format!("failed to run evals in `{}`", workspace.display()))?;
                let passed = report.passed;
                println!("{}", eval::render(&report, args.format)?);
                if !passed {
                    std::process::exit(1);
                }
            }
        },
        Commands::Run(args) => {
            let prompt = cli::resolve_prompt(&args)?;
            let sink_selection = wiring::RunSinkSelection::from_args(&args);
            let model = args
                .model
                .as_deref()
                .map(|model| ModelName::new(model.to_owned()))
                .transpose()
                .context("invalid --model value")?;
            let harness_config = config::harness_config(&args, model)?;
            let ticket_id = TicketId::new(args.ticket).context("invalid --ticket value")?;
            let session_override = args
                .session
                .clone()
                .map(SessionId::new)
                .transpose()
                .context("invalid --session value")?;
            let engine = wiring::engine(
                &args.workspace,
                &args.output_dir,
                args.harness,
                &harness_config.config,
                sink_selection,
            )?;
            let outcome = engine
                .run(RunRequest {
                    ticket_id,
                    label: args.label,
                    harness: args.harness,
                    workspace: args.workspace,
                    session_override,
                    model: harness_config.model,
                    raw_event_retention: harness_config.raw_event_retention,
                    execution_permission: harness_config.execution_permission,
                    options: harness_config.options,
                    prompt,
                })
                .await?;
            if !outcome.success {
                if let Some(reason) = outcome.failure_reason {
                    eprintln!("{reason}");
                }
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
            let pricing = cli::resolve_pricing(&args)?;
            let query = ReportQuery {
                work: args.work,
                label: args.label,
                since,
                pricing,
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
