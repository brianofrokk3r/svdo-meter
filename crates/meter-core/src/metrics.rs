use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
}

impl TokenUsage {
    pub fn add_assign(&mut self, other: &Self) {
        add_opt(&mut self.input_tokens, other.input_tokens);
        add_opt(&mut self.cached_input_tokens, other.cached_input_tokens);
        add_opt(&mut self.cache_write_tokens, other.cache_write_tokens);
        add_opt(&mut self.output_tokens, other.output_tokens);
        add_opt(&mut self.reasoning_tokens, other.reasoning_tokens);
    }
}

fn add_opt(target: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        *target = Some(target.unwrap_or(0).saturating_add(value));
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunMetrics {
    pub wall_time_ms: u64,
    pub active_time_ms: u64,
    pub command_time_ms: u64,
    pub tool_time_ms: u64,
    pub turn_count: u64,
    pub provider_event_count: u64,
    pub commands_executed: u64,
    pub failed_commands: u64,
    pub files_changed: u64,
    pub tool_calls: u64,
    pub errors: u64,
    pub token_usage: TokenUsage,
}

impl RunMetrics {
    pub fn merge_from_event_payload(&mut self, payload: &crate::EventPayload) {
        match payload {
            crate::EventPayload::UsageReported(usage) => self.token_usage.add_assign(usage),
            crate::EventPayload::CommandStarted(_) => {
                self.commands_executed = self.commands_executed.saturating_add(1);
            }
            crate::EventPayload::CommandCompleted(command) => {
                if !command.success {
                    self.failed_commands = self.failed_commands.saturating_add(1);
                }
                if let Some(duration_ms) = command.duration_ms {
                    self.command_time_ms = self.command_time_ms.saturating_add(duration_ms);
                }
            }
            crate::EventPayload::FilesChanged(files) => {
                self.files_changed = self.files_changed.saturating_add(files.count);
            }
            crate::EventPayload::ToolStarted(_) => {
                self.tool_calls = self.tool_calls.saturating_add(1);
            }
            crate::EventPayload::ToolCompleted(tool) => {
                if !tool.success {
                    self.errors = self.errors.saturating_add(1);
                }
                if let Some(duration_ms) = tool.duration_ms {
                    self.tool_time_ms = self.tool_time_ms.saturating_add(duration_ms);
                }
            }
            _ => {}
        }
    }
}
