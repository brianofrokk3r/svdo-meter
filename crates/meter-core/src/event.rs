use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    EventId, ExecutionPermissionMode, HarnessKind, ModelName, RunId, RunMetrics, SessionId,
    TicketId, TokenUsage,
};

pub const SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeterEvent {
    pub schema_version: u16,
    pub event_id: EventId,
    pub event_type: EventType,
    pub occurred_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub run_id: RunId,
    pub ticket_id: TicketId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub harness: HarnessKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_model: Option<ModelName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_model: Option<ModelName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<PathBuf>,
    pub payload: EventPayload,
}

impl MeterEvent {
    pub fn new(context: EventContext, payload: EventPayload) -> Self {
        let now = Utc::now();
        Self {
            schema_version: SCHEMA_VERSION,
            event_id: EventId::new(),
            event_type: payload.event_type(),
            occurred_at: now,
            observed_at: now,
            run_id: context.run_id,
            ticket_id: context.ticket_id,
            label: context.label,
            harness: context.harness,
            requested_model: context.requested_model,
            resolved_model: context.resolved_model,
            session_id: context.session_id,
            workspace: context.workspace,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventContext {
    pub run_id: RunId,
    pub ticket_id: TicketId,
    pub label: Option<String>,
    pub harness: HarnessKind,
    pub requested_model: Option<ModelName>,
    pub resolved_model: Option<ModelName>,
    pub session_id: Option<SessionId>,
    pub workspace: Option<PathBuf>,
}

impl EventContext {
    pub fn with_session(&self, session_id: Option<SessionId>) -> Self {
        let mut next = self.clone();
        next.session_id = session_id;
        next
    }

    pub fn with_resolved_model(&self, resolved_model: Option<ModelName>) -> Self {
        let mut next = self.clone();
        next.resolved_model = resolved_model;
        next
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventType {
    #[serde(rename = "run.started")]
    RunStarted,
    #[serde(rename = "session.discovered")]
    SessionDiscovered,
    #[serde(rename = "harness.event")]
    HarnessEvent,
    #[serde(rename = "usage.reported")]
    UsageReported,
    #[serde(rename = "command.started")]
    CommandStarted,
    #[serde(rename = "command.completed")]
    CommandCompleted,
    #[serde(rename = "files.changed")]
    FilesChanged,
    #[serde(rename = "tool.started")]
    ToolStarted,
    #[serde(rename = "tool.completed")]
    ToolCompleted,
    #[serde(rename = "run.completed")]
    RunCompleted,
    #[serde(rename = "run.failed")]
    RunFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum EventPayload {
    RunStarted(RunStarted),
    SessionDiscovered(SessionDiscovered),
    HarnessEvent(HarnessEvent),
    UsageReported(TokenUsage),
    CommandStarted(CommandStarted),
    CommandCompleted(CommandCompleted),
    FilesChanged(FilesChanged),
    ToolStarted(ToolStarted),
    ToolCompleted(ToolCompleted),
    RunCompleted(RunCompleted),
    RunFailed(RunFailed),
}

impl EventPayload {
    pub fn event_type(&self) -> EventType {
        match self {
            Self::RunStarted(_) => EventType::RunStarted,
            Self::SessionDiscovered(_) => EventType::SessionDiscovered,
            Self::HarnessEvent(_) => EventType::HarnessEvent,
            Self::UsageReported(_) => EventType::UsageReported,
            Self::CommandStarted(_) => EventType::CommandStarted,
            Self::CommandCompleted(_) => EventType::CommandCompleted,
            Self::FilesChanged(_) => EventType::FilesChanged,
            Self::ToolStarted(_) => EventType::ToolStarted,
            Self::ToolCompleted(_) => EventType::ToolCompleted,
            Self::RunCompleted(_) => EventType::RunCompleted,
            Self::RunFailed(_) => EventType::RunFailed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStarted {
    pub prompt_recorded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_permission: Option<ExecutionPermissionMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDiscovered {
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessEvent {
    pub source_event: String,
    pub retained_raw_payload: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandStarted {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandCompleted {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesChanged {
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolStarted {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCompleted {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCompleted {
    pub metrics: RunMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFailed {
    pub metrics: RunMetrics,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}
