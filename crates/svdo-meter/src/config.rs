use meter_core::{CodexConfig, HarnessConfig, HarnessKind, ModelName, RawEventRetention};

pub fn harness_config(harness: HarnessKind, model: Option<ModelName>) -> HarnessConfig {
    match harness {
        HarnessKind::Codex => HarnessConfig::Codex(CodexConfig {
            model,
            raw_event_retention: RawEventRetention::Disabled,
            ..CodexConfig::default()
        }),
        HarnessKind::Claude => HarnessConfig::Claude(meter_core::ClaudeConfig { model }),
        HarnessKind::Gemini => HarnessConfig::Gemini(meter_core::GeminiConfig { model }),
    }
}
