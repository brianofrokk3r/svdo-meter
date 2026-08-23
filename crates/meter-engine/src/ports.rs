use std::path::{Path, PathBuf};

use async_trait::async_trait;
use meter_core::{
    EventContext, HarnessKind, MeterEvent, ModelName, RawEventRetention, RunMetrics, SessionId,
    TicketId,
};
use serde_json::{Map, Value};
use thiserror::Error;
use tokio::sync::mpsc;

pub type EventSender = mpsc::Sender<MeterEvent>;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HarnessOptions {
    values: Map<String, Value>,
}

impl HarnessOptions {
    pub fn new(values: Map<String, Value>) -> Self {
        Self { values }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn values(&self) -> &Map<String, Value> {
        &self.values
    }
}

#[derive(Debug, Clone)]
pub struct HarnessRunRequest {
    pub context: EventContext,
    pub prompt: String,
    pub session_id: Option<SessionId>,
    pub model: Option<ModelName>,
    pub raw_event_retention: RawEventRetention,
    pub options: HarnessOptions,
}

#[derive(Debug, Clone, Default)]
pub struct HarnessRunResult {
    pub success: bool,
    pub session_id: Option<SessionId>,
    pub resolved_model: Option<ModelName>,
    pub metrics: RunMetrics,
    pub exit_code: Option<i32>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("failed to start harness process")]
    Spawn(#[source] std::io::Error),
    #[error("failed to read harness output")]
    Io(#[source] std::io::Error),
    #[error("unsupported harness configuration for adapter")]
    UnsupportedConfig,
    #[error("harness process was interrupted")]
    Interrupted,
}

#[async_trait]
pub trait HarnessAdapter: Send + Sync {
    fn kind(&self) -> HarnessKind;
    fn capabilities(&self) -> HarnessCapabilities;

    async fn run(
        &self,
        request: HarnessRunRequest,
        events: EventSender,
    ) -> Result<HarnessRunResult, HarnessError>;
}

#[derive(Debug, Clone, Copy)]
pub struct HarnessCapabilities {
    pub supports_resume: bool,
    pub supports_workspace: bool,
    pub supports_event_stream: bool,
    pub reports_token_usage: bool,
    pub reports_model: bool,
}

#[derive(Debug, Clone, Default)]
pub struct EventQuery {
    pub ticket_id: Option<TicketId>,
    pub harness: Option<HarnessKind>,
    pub workspace: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("event store I/O failure")]
    Io(#[from] std::io::Error),
    #[error("event store serialization failure")]
    Json(#[from] serde_json::Error),
}

#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, event: &MeterEvent) -> Result<(), StoreError>;
    async fn stream(&self, query: EventQuery) -> Result<Vec<MeterEvent>, StoreError>;

    async fn latest_session(
        &self,
        ticket_id: &TicketId,
        harness: HarnessKind,
        workspace: Option<&Path>,
    ) -> Result<Option<SessionId>, StoreError> {
        let events = self
            .stream(EventQuery {
                ticket_id: Some(ticket_id.clone()),
                harness: Some(harness),
                workspace: workspace.map(Path::to_path_buf),
            })
            .await?;
        Ok(crate::SessionProjection::from_events(&events)
            .lookup(ticket_id, harness, workspace)
            .cloned())
    }
}
