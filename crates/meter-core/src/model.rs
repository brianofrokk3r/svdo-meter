use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ModelName, RunId, SessionId, TicketId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkRef {
    pub ticket_id: TicketId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunContext {
    pub run_id: RunId,
    pub work: WorkRef,
    pub harness: HarnessKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_model: Option<ModelName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessKind {
    Codex,
    Claude,
    Gemini,
}

impl HarnessKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
        }
    }
}

impl Display for HarnessKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("unsupported harness `{0}`")]
pub struct HarnessParseError(String);

impl FromStr for HarnessKind {
    type Err = HarnessParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "codex" => Ok(Self::Codex),
            "claude" => Ok(Self::Claude),
            "gemini" => Ok(Self::Gemini),
            other => Err(HarnessParseError(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawEventRetention {
    Disabled,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HarnessConfig {
    Codex(CodexConfig),
    Claude(ClaudeConfig),
    Gemini(GeminiConfig),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexConfig {
    pub binary: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelName>,
    pub raw_event_retention: RawEventRetention,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("codex"),
            model: None,
            raw_event_retention: RawEventRetention::Disabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeConfig {
    pub binary: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelName>,
    pub raw_event_retention: RawEventRetention,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("claude"),
            model: None,
            raw_event_retention: RawEventRetention::Disabled,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeRunOptions {
    pub continue_latest: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub fork_session: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub add_dirs: Vec<PathBuf>,
    pub mcp_configs: Vec<String>,
    pub strict_mcp_config: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setting_sources: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_file: Option<PathBuf>,
    pub append_system_prompts: Vec<String>,
    pub append_system_prompt_files: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_budget_usd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelName>,
}
