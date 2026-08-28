use anyhow::{Context, bail};
use meter_core::{
    ClaudeConfig, CodexApprovalMode, CodexConfig, CodexConfigOverride, HarnessConfig, HarnessKind,
    ModelName, RawEventRetention,
};
use meter_engine::HarnessOptions;
use serde_json::Value;

use crate::cli::{RunArgs, claude_options};

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
    validate_claude_option_scope(args)?;

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
        HarnessKind::Claude => HarnessConfig::Claude(ClaudeConfig {
            model: model.clone(),
            raw_event_retention: RawEventRetention::Disabled,
            ..ClaudeConfig::default()
        }),
        HarnessKind::Gemini => HarnessConfig::Gemini(meter_core::GeminiConfig {
            model: model.clone(),
        }),
    };
    let raw_event_retention = match &config {
        HarnessConfig::Codex(config) => config.raw_event_retention.clone(),
        HarnessConfig::Claude(config) => config.raw_event_retention.clone(),
        HarnessConfig::Gemini(_) => RawEventRetention::Disabled,
    };
    let options = match args.harness {
        HarnessKind::Claude => {
            let value = serde_json::to_value(claude_options(args))?;
            match value {
                Value::Object(values) => HarnessOptions::new(values),
                _ => HarnessOptions::empty(),
            }
        }
        HarnessKind::Codex | HarnessKind::Gemini => HarnessOptions::empty(),
    };
    Ok(RunHarnessConfig {
        config,
        model,
        raw_event_retention,
        options,
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

fn validate_claude_option_scope(args: &RunArgs) -> anyhow::Result<()> {
    if args.harness == HarnessKind::Claude || !has_claude_options(args) {
        return Ok(());
    }
    bail!("Claude-specific --claude-* options require --harness claude")
}

fn has_claude_options(args: &RunArgs) -> bool {
    args.claude_continue
        || args.claude_resume.is_some()
        || args.claude_session_id.is_some()
        || args.claude_fork_session
        || args.claude_permission_mode.is_some()
        || !args.claude_allowed_tools.is_empty()
        || !args.claude_disallowed_tools.is_empty()
        || !args.claude_add_dirs.is_empty()
        || !args.claude_mcp_configs.is_empty()
        || args.claude_strict_mcp_config
        || args.claude_settings.is_some()
        || args.claude_setting_sources.is_some()
        || args.claude_system_prompt.is_some()
        || args.claude_system_prompt_file.is_some()
        || !args.claude_append_system_prompts.is_empty()
        || !args.claude_append_system_prompt_files.is_empty()
        || args.claude_max_turns.is_some()
        || args.claude_max_budget_usd.is_some()
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
    fn claude_config_exposes_options_and_neutral_model() {
        let model = ModelName::new("claude-sonnet").unwrap_or_else(|err| panic!("{err}"));
        let mut args = run_args(HarnessKind::Claude);
        args.claude_continue = true;
        args.claude_permission_mode = Some("plan".to_owned());

        let config =
            harness_config(&args, Some(model.clone())).unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(config.model, Some(model.clone()));
        assert_eq!(config.raw_event_retention, RawEventRetention::Disabled);
        assert_eq!(
            config.options.values().get("continue_latest"),
            Some(&serde_json::Value::Bool(true))
        );
        assert_eq!(
            config.options.values().get("permission_mode"),
            Some(&serde_json::Value::String("plan".to_owned()))
        );
        assert_eq!(
            config.config,
            HarnessConfig::Claude(ClaudeConfig {
                model: Some(model),
                raw_event_retention: RawEventRetention::Disabled,
                ..ClaudeConfig::default()
            })
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
    fn rejects_claude_options_for_non_claude_harness() {
        let mut args = run_args(HarnessKind::Codex);
        args.claude_continue = true;

        let error = match harness_config(&args, None) {
            Ok(_) => panic!("expected harness config error"),
            Err(error) => error,
        };

        assert!(
            error
                .to_string()
                .contains("Claude-specific --claude-* options require --harness claude")
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
            claude_continue: false,
            claude_resume: None,
            claude_session_id: None,
            claude_fork_session: false,
            claude_permission_mode: None,
            claude_allowed_tools: Vec::new(),
            claude_disallowed_tools: Vec::new(),
            claude_add_dirs: Vec::new(),
            claude_mcp_configs: Vec::new(),
            claude_strict_mcp_config: false,
            claude_settings: None,
            claude_setting_sources: None,
            claude_system_prompt: None,
            claude_system_prompt_file: None,
            claude_append_system_prompts: Vec::new(),
            claude_append_system_prompt_files: Vec::new(),
            claude_max_turns: None,
            claude_max_budget_usd: None,
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
