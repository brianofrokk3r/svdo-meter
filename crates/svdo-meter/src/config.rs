use meter_core::{
    ClaudeConfig, CodexConfig, HarnessConfig, HarnessKind, ModelName, RawEventRetention,
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
    let harness = args.harness;
    let config = match harness {
        HarnessKind::Codex => HarnessConfig::Codex(CodexConfig {
            model: model.clone(),
            raw_event_retention: RawEventRetention::Disabled,
            ..CodexConfig::default()
        }),
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
    let options = match harness {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_config_exposes_neutral_run_fields() -> anyhow::Result<()> {
        let model = ModelName::new("gpt-5").unwrap_or_else(|err| panic!("{err}"));
        let mut args = run_args();
        args.harness = HarnessKind::Codex;

        let config = harness_config(&args, Some(model.clone()))?;

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
        Ok(())
    }

    #[test]
    fn claude_config_exposes_options_and_neutral_model() -> anyhow::Result<()> {
        let model = ModelName::new("claude-sonnet").unwrap_or_else(|err| panic!("{err}"));
        let mut args = run_args();
        args.harness = HarnessKind::Claude;
        args.claude_continue = true;
        args.claude_permission_mode = Some("plan".to_owned());

        let config = harness_config(&args, Some(model.clone()))?;

        assert_eq!(config.model, Some(model.clone()));
        assert_eq!(config.raw_event_retention, RawEventRetention::Disabled);
        assert_eq!(
            config.options.values().get("continue_latest"),
            Some(&serde_json::Value::Bool(true))
        );
        assert_eq!(
            config.config,
            HarnessConfig::Claude(ClaudeConfig {
                model: Some(model),
                raw_event_retention: RawEventRetention::Disabled,
                ..ClaudeConfig::default()
            })
        );
        Ok(())
    }

    fn run_args() -> RunArgs {
        RunArgs {
            ticket: "ENG-142".to_owned(),
            label: None,
            harness: HarnessKind::Codex,
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
        }
    }
}
