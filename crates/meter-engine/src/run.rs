use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use meter_core::{
    EventContext, EventPayload, HarnessKind, MeterEvent, ModelName, RawEventRetention,
    RunCompleted, RunFailed, RunId, RunMetrics, RunStarted, SessionDiscovered, SessionId, TicketId,
};
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{
    EventBus, EventStore, EventStoreSink, HarnessAdapter, HarnessError, HarnessOptions,
    HarnessRunRequest, StoreError,
};

#[derive(Debug, Clone)]
pub struct RunRequest {
    pub ticket_id: TicketId,
    pub label: Option<String>,
    pub harness: HarnessKind,
    pub workspace: Option<PathBuf>,
    pub session_override: Option<SessionId>,
    pub model: Option<ModelName>,
    pub raw_event_retention: RawEventRetention,
    pub options: HarnessOptions,
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub run_id: RunId,
    pub success: bool,
    pub session_id: Option<SessionId>,
    pub metrics: RunMetrics,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error("no adapter registered for harness {0}")]
    MissingAdapter(HarnessKind),
    #[error("event store error")]
    Store(#[from] StoreError),
    #[error("event sink error")]
    Sink(#[from] crate::EventBusError),
    #[error("event writer task failed")]
    WriterJoin(#[source] tokio::task::JoinError),
}

pub struct RunEngine {
    store: Arc<dyn EventStore>,
    event_bus: EventBus,
    adapters: HashMap<HarnessKind, Arc<dyn HarnessAdapter>>,
}

impl RunEngine {
    pub fn new(store: Arc<dyn EventStore>) -> Self {
        let event_bus = EventBus::default().with_sink(Arc::new(EventStoreSink::new(store.clone())));
        Self {
            store,
            event_bus,
            adapters: HashMap::new(),
        }
    }

    pub fn with_event_sink(mut self, sink: Arc<dyn crate::EventSink>) -> Self {
        self.event_bus = self.event_bus.with_sink(sink);
        self
    }

    pub fn with_adapter(mut self, adapter: Arc<dyn HarnessAdapter>) -> Self {
        self.adapters.insert(adapter.kind(), adapter);
        self
    }

    pub async fn run(&self, request: RunRequest) -> Result<RunOutcome, RunError> {
        let adapter = self
            .adapters
            .get(&request.harness)
            .ok_or(RunError::MissingAdapter(request.harness))?
            .clone();
        let run_id = RunId::new();
        let selected_session = match request.session_override.clone() {
            Some(session_id) => Some(session_id),
            None => {
                self.store
                    .latest_session(
                        &request.ticket_id,
                        request.harness,
                        request.workspace.as_deref(),
                    )
                    .await?
            }
        };
        let base_context = EventContext {
            run_id,
            ticket_id: request.ticket_id.clone(),
            label: request.label.clone(),
            harness: request.harness,
            requested_model: request.model.clone(),
            resolved_model: None,
            session_id: selected_session.clone(),
            workspace: request.workspace.clone(),
        };
        let start_event = MeterEvent::new(
            base_context.clone(),
            EventPayload::RunStarted(RunStarted {
                prompt_recorded: false,
            }),
        );
        self.event_bus.emit(&start_event).await?;
        if request.session_override.is_some() {
            let override_event = MeterEvent::new(
                base_context.clone(),
                EventPayload::SessionDiscovered(SessionDiscovered {
                    source: "user_override".to_owned(),
                }),
            );
            self.event_bus.emit(&override_event).await?;
        }

        let (tx, mut rx) = mpsc::channel::<MeterEvent>(256);
        let event_bus = self.event_bus.clone();
        let writer = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                event_bus.emit(&event).await?;
            }
            Ok::<(), crate::EventBusError>(())
        });

        let started = Instant::now();
        let harness_request = HarnessRunRequest {
            context: base_context.clone(),
            prompt: request.prompt,
            session_id: selected_session.clone(),
            model: request.model,
            raw_event_retention: request.raw_event_retention,
            options: request.options,
        };
        let harness_result = adapter.run(harness_request, tx.clone()).await;
        let elapsed_ms = saturating_elapsed_ms(started);
        let terminal_context = match &harness_result {
            Ok(result) => base_context
                .with_session(
                    result
                        .session_id
                        .clone()
                        .or_else(|| selected_session.clone()),
                )
                .with_resolved_model(result.resolved_model.clone()),
            Err(_) => base_context,
        };
        let (success, session_id, metrics, exit_code) = match harness_result {
            Ok(mut result) => {
                result.metrics.wall_time_ms = elapsed_ms;
                let payload = if result.success {
                    EventPayload::RunCompleted(RunCompleted {
                        metrics: result.metrics.clone(),
                        exit_code: result.exit_code,
                    })
                } else {
                    EventPayload::RunFailed(RunFailed {
                        metrics: result.metrics.clone(),
                        reason: result
                            .failure_reason
                            .clone()
                            .unwrap_or_else(|| "harness reported failure".to_owned()),
                        exit_code: result.exit_code,
                    })
                };
                let terminal = MeterEvent::new(terminal_context, payload);
                tx.send(terminal).await.map_err(|_| {
                    StoreError::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "event writer closed",
                    ))
                })?;
                (
                    result.success,
                    result.session_id,
                    result.metrics,
                    result.exit_code,
                )
            }
            Err(error) => {
                let metrics = RunMetrics {
                    wall_time_ms: elapsed_ms,
                    errors: 1,
                    ..RunMetrics::default()
                };
                let terminal = MeterEvent::new(
                    terminal_context,
                    EventPayload::RunFailed(RunFailed {
                        metrics: metrics.clone(),
                        reason: harness_error_reason(&error),
                        exit_code: None,
                    }),
                );
                tx.send(terminal).await.map_err(|_| {
                    StoreError::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "event writer closed",
                    ))
                })?;
                (false, None, metrics, None)
            }
        };
        drop(tx);
        writer.await.map_err(RunError::WriterJoin)??;

        Ok(RunOutcome {
            run_id,
            success,
            session_id,
            metrics,
            exit_code,
        })
    }
}

fn saturating_elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn harness_error_reason(error: &HarnessError) -> String {
    match error {
        HarnessError::Spawn(source) => format!("failed to start harness process: {source}"),
        HarnessError::Io(source) => format!("failed to read harness output: {source}"),
        HarnessError::UnsupportedConfig(message) => {
            format!("unsupported harness configuration for adapter: {message}")
        }
        HarnessError::Interrupted => "harness process was interrupted".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use meter_core::{
        EventContext, EventPayload, EventType, HarnessKind, MeterEvent, ModelName,
        RawEventRetention, SessionDiscovered, SessionId,
    };

    use super::*;
    use crate::{
        EventQuery, EventSender, EventSink, EventStore, HarnessCapabilities, HarnessRunResult,
        SinkError,
    };

    #[derive(Debug, Default)]
    struct MemoryStore {
        events: Mutex<Vec<MeterEvent>>,
    }

    #[async_trait]
    impl EventStore for MemoryStore {
        async fn append(&self, event: &MeterEvent) -> Result<(), StoreError> {
            self.events
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .push(event.clone());
            Ok(())
        }

        async fn stream(&self, query: EventQuery) -> Result<Vec<MeterEvent>, StoreError> {
            Ok(self
                .events
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .iter()
                .filter(|event| {
                    query
                        .ticket_id
                        .as_ref()
                        .is_none_or(|ticket_id| &event.ticket_id == ticket_id)
                        && query.harness.is_none_or(|harness| event.harness == harness)
                        && query
                            .workspace
                            .as_ref()
                            .is_none_or(|workspace| event.workspace.as_ref() == Some(workspace))
                })
                .cloned()
                .collect())
        }
    }

    #[derive(Debug)]
    struct FakeAdapter {
        observed_sessions: Arc<Mutex<Vec<Option<SessionId>>>>,
        observed_models: Arc<Mutex<Vec<Option<ModelName>>>>,
        observed_retention: Arc<Mutex<Vec<RawEventRetention>>>,
        result: HarnessRunResult,
    }

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<MeterEvent>>,
    }

    #[async_trait]
    impl EventSink for RecordingSink {
        async fn emit(&self, event: &MeterEvent) -> Result<(), SinkError> {
            self.events
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .push(event.clone());
            Ok(())
        }
    }

    struct FailingSink;

    #[async_trait]
    impl EventSink for FailingSink {
        async fn emit(&self, _event: &MeterEvent) -> Result<(), SinkError> {
            Err(SinkError::Message("fixture sink failed".to_owned()))
        }
    }

    #[async_trait]
    impl HarnessAdapter for FakeAdapter {
        fn kind(&self) -> HarnessKind {
            HarnessKind::Codex
        }

        fn capabilities(&self) -> HarnessCapabilities {
            HarnessCapabilities {
                supports_resume: true,
                supports_workspace: true,
                supports_event_stream: true,
                reports_token_usage: true,
                reports_model: true,
            }
        }

        async fn run(
            &self,
            request: HarnessRunRequest,
            events: EventSender,
        ) -> Result<HarnessRunResult, HarnessError> {
            self.observed_sessions
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .push(request.session_id.clone());
            self.observed_models
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .push(request.model.clone());
            self.observed_retention
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .push(request.raw_event_retention.clone());
            if let Some(session_id) = &self.result.session_id {
                events
                    .send(MeterEvent::new(
                        request.context.with_session(Some(session_id.clone())),
                        EventPayload::SessionDiscovered(SessionDiscovered {
                            source: "fake".to_owned(),
                        }),
                    ))
                    .await
                    .unwrap_or_else(|err| panic!("{err}"));
            }
            Ok(self.result.clone())
        }
    }

    fn run_request(ticket: &str, session_override: Option<SessionId>) -> RunRequest {
        RunRequest {
            ticket_id: TicketId::new(ticket).unwrap_or_else(|err| panic!("{err}")),
            label: Some("Label".to_owned()),
            harness: HarnessKind::Codex,
            workspace: Some(PathBuf::from(".")),
            session_override,
            model: None,
            raw_event_retention: RawEventRetention::Disabled,
            options: HarnessOptions::empty(),
            prompt: "Do work".to_owned(),
        }
    }

    #[tokio::test]
    async fn first_run_discovers_session_and_completes() {
        let store = Arc::new(MemoryStore::default());
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_models = Arc::new(Mutex::new(Vec::new()));
        let observed_retention = Arc::new(Mutex::new(Vec::new()));
        let session_id = SessionId::new("session-new").unwrap_or_else(|err| panic!("{err}"));
        let adapter = Arc::new(FakeAdapter {
            observed_sessions: Arc::clone(&observed),
            observed_models,
            observed_retention,
            result: HarnessRunResult {
                success: true,
                session_id: Some(session_id.clone()),
                ..HarnessRunResult::default()
            },
        });
        let engine = RunEngine::new(store.clone()).with_adapter(adapter);

        let outcome = engine
            .run(run_request("ENG-1", None))
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        assert!(outcome.success);
        assert_eq!(outcome.session_id, Some(session_id));
        assert_eq!(
            observed
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .as_slice(),
            &[None]
        );
        let event_types: Vec<EventType> = store
            .stream(EventQuery::default())
            .await
            .unwrap_or_else(|err| panic!("{err}"))
            .into_iter()
            .map(|event| event.event_type)
            .collect();
        assert_eq!(
            event_types,
            vec![
                EventType::RunStarted,
                EventType::SessionDiscovered,
                EventType::RunCompleted,
            ]
        );
    }

    #[tokio::test]
    async fn extra_sink_does_not_replace_default_store_sink() {
        let store = Arc::new(MemoryStore::default());
        let live_sink = Arc::new(RecordingSink::default());
        let adapter = Arc::new(FakeAdapter {
            observed_sessions: Arc::new(Mutex::new(Vec::new())),
            observed_models: Arc::new(Mutex::new(Vec::new())),
            observed_retention: Arc::new(Mutex::new(Vec::new())),
            result: HarnessRunResult {
                success: true,
                ..HarnessRunResult::default()
            },
        });
        let engine = RunEngine::new(store.clone())
            .with_event_sink(live_sink.clone())
            .with_adapter(adapter);

        let _ = engine
            .run(run_request("ENG-SINKS", None))
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        let stored = store
            .stream(EventQuery::default())
            .await
            .unwrap_or_else(|err| panic!("{err}"));
        let live = live_sink
            .events
            .lock()
            .unwrap_or_else(|err| panic!("{err}"))
            .clone();
        assert_eq!(stored, live);
        assert!(
            stored
                .iter()
                .any(|event| event.event_type == EventType::RunStarted)
        );
        assert!(
            stored
                .iter()
                .any(|event| event.event_type == EventType::RunCompleted)
        );
    }

    #[tokio::test]
    async fn sink_failure_fails_run_command() {
        let store = Arc::new(MemoryStore::default());
        let adapter = Arc::new(FakeAdapter {
            observed_sessions: Arc::new(Mutex::new(Vec::new())),
            observed_models: Arc::new(Mutex::new(Vec::new())),
            observed_retention: Arc::new(Mutex::new(Vec::new())),
            result: HarnessRunResult {
                success: true,
                ..HarnessRunResult::default()
            },
        });
        let engine = RunEngine::new(store.clone())
            .with_event_sink(Arc::new(FailingSink))
            .with_adapter(adapter);

        let error = engine
            .run(run_request("ENG-SINK-FAIL", None))
            .await
            .err()
            .unwrap_or_else(|| panic!("expected sink error"));

        assert!(matches!(error, RunError::Sink(_)));
        let stored = store
            .stream(EventQuery::default())
            .await
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].event_type, EventType::RunStarted);
    }

    #[tokio::test]
    async fn later_run_resumes_known_session() {
        let store = Arc::new(MemoryStore::default());
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_models = Arc::new(Mutex::new(Vec::new()));
        let observed_retention = Arc::new(Mutex::new(Vec::new()));
        let session_id = SessionId::new("session-existing").unwrap_or_else(|err| panic!("{err}"));
        let seed_context = EventContext {
            run_id: RunId::new(),
            ticket_id: TicketId::new("ENG-2").unwrap_or_else(|err| panic!("{err}")),
            label: None,
            harness: HarnessKind::Codex,
            requested_model: None,
            resolved_model: None,
            session_id: Some(session_id.clone()),
            workspace: Some(Path::new(".").to_path_buf()),
        };
        store
            .append(&MeterEvent::new(
                seed_context,
                EventPayload::SessionDiscovered(SessionDiscovered {
                    source: "seed".to_owned(),
                }),
            ))
            .await
            .unwrap_or_else(|err| panic!("{err}"));
        let adapter = Arc::new(FakeAdapter {
            observed_sessions: Arc::clone(&observed),
            observed_models,
            observed_retention,
            result: HarnessRunResult {
                success: true,
                session_id: Some(session_id.clone()),
                ..HarnessRunResult::default()
            },
        });
        let engine = RunEngine::new(store).with_adapter(adapter);

        let _ = engine
            .run(run_request("ENG-2", None))
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(
            observed
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .as_slice(),
            &[Some(session_id)]
        );
    }

    #[tokio::test]
    async fn explicit_session_override_wins() {
        let store = Arc::new(MemoryStore::default());
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_models = Arc::new(Mutex::new(Vec::new()));
        let observed_retention = Arc::new(Mutex::new(Vec::new()));
        let override_session = SessionId::new("override").unwrap_or_else(|err| panic!("{err}"));
        let adapter = Arc::new(FakeAdapter {
            observed_sessions: Arc::clone(&observed),
            observed_models,
            observed_retention,
            result: HarnessRunResult {
                success: true,
                session_id: Some(override_session.clone()),
                ..HarnessRunResult::default()
            },
        });
        let engine = RunEngine::new(store).with_adapter(adapter);

        let _ = engine
            .run(run_request("ENG-3", Some(override_session.clone())))
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(
            observed
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .as_slice(),
            &[Some(override_session)]
        );
    }

    #[tokio::test]
    async fn forwards_neutral_model_and_retention_to_adapter() {
        let store = Arc::new(MemoryStore::default());
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_models = Arc::new(Mutex::new(Vec::new()));
        let observed_retention = Arc::new(Mutex::new(Vec::new()));
        let adapter = Arc::new(FakeAdapter {
            observed_sessions: observed,
            observed_models: Arc::clone(&observed_models),
            observed_retention: Arc::clone(&observed_retention),
            result: HarnessRunResult {
                success: true,
                ..HarnessRunResult::default()
            },
        });
        let engine = RunEngine::new(store.clone()).with_adapter(adapter);
        let model = ModelName::new("gpt-5").unwrap_or_else(|err| panic!("{err}"));
        let mut request = run_request("ENG-NEUTRAL", None);
        request.model = Some(model.clone());
        request.raw_event_retention = RawEventRetention::Full;

        let _ = engine
            .run(request)
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(
            observed_models
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .as_slice(),
            &[Some(model.clone())]
        );
        assert_eq!(
            observed_retention
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .as_slice(),
            &[RawEventRetention::Full]
        );
        let start = store
            .stream(EventQuery::default())
            .await
            .unwrap_or_else(|err| panic!("{err}"))
            .into_iter()
            .find(|event| event.event_type == EventType::RunStarted)
            .unwrap_or_else(|| panic!("missing run start event"));
        assert_eq!(start.requested_model, Some(model));
    }

    #[tokio::test]
    async fn failed_adapter_result_emits_run_failed() {
        let store = Arc::new(MemoryStore::default());
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_models = Arc::new(Mutex::new(Vec::new()));
        let observed_retention = Arc::new(Mutex::new(Vec::new()));
        let adapter = Arc::new(FakeAdapter {
            observed_sessions: observed,
            observed_models,
            observed_retention,
            result: HarnessRunResult {
                success: false,
                exit_code: Some(2),
                failure_reason: Some("provider failed".to_owned()),
                ..HarnessRunResult::default()
            },
        });
        let engine = RunEngine::new(store.clone()).with_adapter(adapter);

        let outcome = engine
            .run(run_request("ENG-4", None))
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        assert!(!outcome.success);
        let events = store
            .stream(EventQuery::default())
            .await
            .unwrap_or_else(|err| panic!("{err}"));
        assert!(
            events
                .iter()
                .any(|event| event.event_type == EventType::RunFailed)
        );
    }

    #[tokio::test]
    async fn multiple_runs_for_one_ticket_have_distinct_run_ids() {
        let store = Arc::new(MemoryStore::default());
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_models = Arc::new(Mutex::new(Vec::new()));
        let observed_retention = Arc::new(Mutex::new(Vec::new()));
        let adapter = Arc::new(FakeAdapter {
            observed_sessions: observed,
            observed_models,
            observed_retention,
            result: HarnessRunResult {
                success: true,
                ..HarnessRunResult::default()
            },
        });
        let engine = RunEngine::new(store).with_adapter(adapter);

        let first = engine
            .run(run_request("ENG-5", None))
            .await
            .unwrap_or_else(|err| panic!("{err}"));
        let second = engine
            .run(run_request("ENG-5", None))
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        assert_ne!(first.run_id, second.run_id);
    }
}
