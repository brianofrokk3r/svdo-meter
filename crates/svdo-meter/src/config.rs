use anyhow::{Context, bail};
use meter_core::{
    CodexApprovalMode, CodexConfig, CodexConfigOverride, HarnessConfig, HarnessKind, ModelName,
    RawEventRetention,
};
use meter_engine::HarnessOptions;

use crate::cli::RunArgs;

#[derive(Debug, Clone)]
pub struct RunHarnessConfig {
    pub config: HarnessConfig,
    pub model: Option<ModelName>,
    pub raw_event_retention: RawEventRetention,
    pub options: HarnessOptions,
}

pub fn harness_config(
    args: &RunArgs,
    model: Option<ModelName>,
) -> anyhow::Result<RunHarnessConfig> {
    validate_codex_option_scope(args)?;
    let config = match args.harness {
        HarnessKind::Codex => {
            let profile = args
                .codex_profile
                .as_ref()
                .map(|profile| validate_non_empty("codex profile", profile))
                .transpose()?;
            let config_overrides = args
                .codex_config
                .iter()
                .map(|value| parse_codex_config_override(value))
                .collect::<anyhow::Result<Vec<_>>>()?;
            HarnessConfig::Codex(CodexConfig {
                model: model.clone(),
                raw_event_retention: RawEventRetention::Disabled,
                profile,
                sandbox: args.codex_sandbox,
                approval_mode: if args.codex_approve_for_me {
                    CodexApprovalMode::ApproveForMe
                } else {
                    CodexApprovalMode::Manual
                },
                yolo: args.codex_yolo,
                config_overrides,
                ..CodexConfig::default()
            })
        }
        HarnessKind::Claude => HarnessConfig::Claude(meter_core::ClaudeConfig {
            model: model.clone(),
        }),
        HarnessKind::Gemini => HarnessConfig::Gemini(meter_core::GeminiConfig {
            model: model.clone(),
        }),
    };
    let raw_event_retention = match &config {
        HarnessConfig::Codex(config) => config.raw_event_retention.clone(),
        HarnessConfig::Claude(_) | HarnessConfig::Gemini(_) => RawEventRetention::Disabled,
    };
    Ok(RunHarnessConfig {
        config,
        model,
        raw_event_retention,
        options: HarnessOptions::empty(),
    })
}

fn validate_codex_option_scope(args: &RunArgs) -> anyhow::Result<()> {
    if args.harness == HarnessKind::Codex || !has_codex_options(args) {
        return Ok(());
    }
    bail!("Codex-specific --codex-* options require --harness codex")
}

fn has_codex_options(args: &RunArgs) -> bool {
    args.codex_profile.is_some()
        || args.codex_sandbox.is_some()
        || args.codex_approve_for_me
        || args.codex_yolo
        || !args.codex_config.is_empty()
}

fn validate_non_empty(name: &str, value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{name} cannot be empty");
    }
    Ok(trimmed.to_owned())
}

fn parse_codex_config_override(value: &str) -> anyhow::Result<CodexConfigOverride> {
    let (key, override_value) = value
        .split_once('=')
        .with_context(|| format!("invalid --codex-config `{value}`; expected key=value"))?;
    let key = key.trim();
    let override_value = override_value.trim();
    if key.is_empty() {
        bail!("invalid --codex-config `{value}`; key cannot be empty");
    }
    if override_value.is_empty() {
        bail!("invalid --codex-config `{value}`; value cannot be empty");
    }
    Ok(CodexConfigOverride::new(key, override_value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_config_exposes_neutral_run_fields() {
        let model = ModelName::new("gpt-5").unwrap_or_else(|err| panic!("{err}"));

        let args = run_args(HarnessKind::Codex);

        let config =
            harness_config(&args, Some(model.clone())).unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(config.model, Some(model.clone()));
        assert_eq!(config.raw_event_retention, RawEventRetention::Disabled);
        assert!(config.options.values().is_empty());
        assert_eq!(
            config.config,
            HarnessConfig::Codex(CodexConfig {
                model: Some(model),
                raw_event_retention: RawEventRetention::Disabled,
                ..CodexConfig::default()
            })
        );
    }

    #[test]
    fn unsupported_provider_configs_still_expose_neutral_model() {
        let model = ModelName::new("claude-sonnet").unwrap_or_else(|err| panic!("{err}"));
        let args = run_args(HarnessKind::Claude);

        let config =
            harness_config(&args, Some(model.clone())).unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(config.model, Some(model.clone()));
        assert_eq!(config.raw_event_retention, RawEventRetention::Disabled);
        assert_eq!(
            config.config,
            HarnessConfig::Claude(meter_core::ClaudeConfig { model: Some(model) })
        );
    }

    #[test]
    fn codex_config_maps_typed_codex_fields() {
        let mut args = run_args(HarnessKind::Codex);
        args.codex_profile = Some("default".to_owned());
        args.codex_sandbox = Some(meter_core::CodexSandboxMode::WorkspaceWrite);
        args.codex_approve_for_me = true;
        args.codex_yolo = true;
        args.codex_config = vec![
            "model_reasoning_effort=high".to_owned(),
            "features.experimental=true".to_owned(),
        ];

        let config = harness_config(&args, None).unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(
            config.config,
            HarnessConfig::Codex(CodexConfig {
                raw_event_retention: RawEventRetention::Disabled,
                profile: Some("default".to_owned()),
                sandbox: Some(meter_core::CodexSandboxMode::WorkspaceWrite),
                approval_mode: CodexApprovalMode::ApproveForMe,
                yolo: true,
                config_overrides: vec![
                    CodexConfigOverride::new("model_reasoning_effort", "high"),
                    CodexConfigOverride::new("features.experimental", "true"),
                ],
                ..CodexConfig::default()
            })
        );
    }

    #[test]
    fn rejects_codex_options_for_non_codex_harness() {
        let mut args = run_args(HarnessKind::Claude);
        args.codex_yolo = true;

        let error = match harness_config(&args, None) {
            Ok(_) => panic!("expected harness config error"),
            Err(error) => error,
        };

        assert!(
            error
                .to_string()
                .contains("Codex-specific --codex-* options require --harness codex")
        );
    }

    #[test]
    fn rejects_invalid_codex_config_overrides() {
        let mut args = run_args(HarnessKind::Codex);
        args.codex_config = vec!["model_reasoning_effort".to_owned()];

        let error = match harness_config(&args, None) {
            Ok(_) => panic!("expected harness config error"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("expected key=value"));
    }

    fn run_args(harness: HarnessKind) -> RunArgs {
        RunArgs {
            ticket: "ENG-142".to_owned(),
            label: None,
            harness,
            workspace: None,
            session: None,
            model: None,
            prompt_file: None,
            prompt: Some("Do work".to_owned()),
            sinks: Vec::new(),
            emit: None,
            codex_profile: None,
            codex_sandbox: None,
            codex_approve_for_me: false,
            codex_yolo: false,
            codex_config: Vec::new(),
        }
    }
}
