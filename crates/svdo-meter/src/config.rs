use meter_core::{CodexConfig, HarnessConfig, HarnessKind, ModelName, RawEventRetention};
use meter_engine::HarnessOptions;

#[derive(Debug, Clone)]
pub struct RunHarnessConfig {
    pub config: HarnessConfig,
    pub model: Option<ModelName>,
    pub raw_event_retention: RawEventRetention,
    pub options: HarnessOptions,
}

pub fn harness_config(harness: HarnessKind, model: Option<ModelName>) -> RunHarnessConfig {
    let config = match harness {
        HarnessKind::Codex => HarnessConfig::Codex(CodexConfig {
            model: model.clone(),
            raw_event_retention: RawEventRetention::Disabled,
            ..CodexConfig::default()
        }),
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
    RunHarnessConfig {
        config,
        model,
        raw_event_retention,
        options: HarnessOptions::empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_config_exposes_neutral_run_fields() {
        let model = ModelName::new("gpt-5").unwrap_or_else(|err| panic!("{err}"));

        let config = harness_config(HarnessKind::Codex, Some(model.clone()));

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

        let config = harness_config(HarnessKind::Claude, Some(model.clone()));

        assert_eq!(config.model, Some(model.clone()));
        assert_eq!(config.raw_event_retention, RawEventRetention::Disabled);
        assert_eq!(
            config.config,
            HarnessConfig::Claude(meter_core::ClaudeConfig { model: Some(model) })
        );
    }
}
