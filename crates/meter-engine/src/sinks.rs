use std::sync::Arc;

use async_trait::async_trait;
use meter_core::MeterEvent;
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::{EventSink, EventStore, SinkError};

#[derive(Clone, Default)]
pub struct EventBus {
    sinks: Vec<Arc<dyn EventSink>>,
}

impl EventBus {
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self { sinks }
    }

    pub fn with_sink(mut self, sink: Arc<dyn EventSink>) -> Self {
        self.sinks.push(sink);
        self
    }

    pub async fn emit(&self, event: &MeterEvent) -> Result<(), EventBusError> {
        let mut first_error = None;
        for sink in &self.sinks {
            if let Err(error) = sink.emit(event).await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        match first_error {
            Some(error) => Err(EventBusError::Sink(error)),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Error)]
pub enum EventBusError {
    #[error("event sink failed")]
    Sink(#[source] SinkError),
}

pub struct EventStoreSink {
    store: Arc<dyn EventStore>,
}

impl EventStoreSink {
    pub fn new(store: Arc<dyn EventStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl EventSink for EventStoreSink {
    async fn emit(&self, event: &MeterEvent) -> Result<(), SinkError> {
        self.store.append(event).await.map_err(SinkError::Store)
    }
}

pub struct NdjsonWriteSink<W> {
    writer: Mutex<W>,
}

impl<W> NdjsonWriteSink<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Mutex::new(writer),
        }
    }
}

#[async_trait]
impl<W> EventSink for NdjsonWriteSink<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    async fn emit(&self, event: &MeterEvent) -> Result<(), SinkError> {
        let line = ndjson_line(event)?;
        let mut writer = self.writer.lock().await;
        writer.write_all(line.as_bytes()).await?;
        writer.flush().await?;
        Ok(())
    }
}

pub fn ndjson_line(event: &MeterEvent) -> Result<String, serde_json::Error> {
    let mut line = serde_json::to_string(event)?;
    line.push('\n');
    Ok(line)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex as StdMutex};

    use async_trait::async_trait;
    use meter_core::{
        EventContext, EventPayload, HarnessKind, MeterEvent, RunId, RunStarted, TicketId,
    };

    use super::*;

    #[derive(Default)]
    struct RecordingSink {
        events: StdMutex<Vec<MeterEvent>>,
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
            Err(SinkError::Message("fixture failure".to_owned()))
        }
    }

    #[tokio::test]
    async fn event_bus_fans_out_events_to_all_sinks() {
        let first = Arc::new(RecordingSink::default());
        let second = Arc::new(RecordingSink::default());
        let bus = EventBus::new(vec![first.clone(), second.clone()]);
        let event = fixture_event();

        bus.emit(&event).await.unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(
            first
                .events
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .as_slice(),
            std::slice::from_ref(&event)
        );
        assert_eq!(
            second
                .events
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .as_slice(),
            &[event]
        );
    }

    #[tokio::test]
    async fn event_bus_reports_sink_errors_after_attempting_later_sinks() {
        let recorder = Arc::new(RecordingSink::default());
        let bus = EventBus::new(vec![Arc::new(FailingSink), recorder.clone()]);
        let event = fixture_event();

        let error = bus
            .emit(&event)
            .await
            .err()
            .unwrap_or_else(|| panic!("expected sink failure"));

        assert!(format!("{error:#}").contains("event sink failed"));
        assert_eq!(
            recorder
                .events
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .as_slice(),
            &[event]
        );
    }

    #[test]
    fn ndjson_line_serializes_one_event_per_line() {
        let event = fixture_event();

        let line = ndjson_line(&event).unwrap_or_else(|err| panic!("{err}"));

        assert!(line.ends_with('\n'));
        assert_eq!(line.lines().count(), 1);
        let decoded = serde_json::from_str::<MeterEvent>(line.trim_end())
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(decoded, event);
    }

    fn fixture_event() -> MeterEvent {
        let ticket_id = TicketId::new("ENG-1").unwrap_or_else(|err| panic!("{err}"));
        MeterEvent::new(
            EventContext {
                run_id: RunId::new(),
                ticket_id,
                label: None,
                harness: HarnessKind::Codex,
                requested_model: None,
                resolved_model: None,
                session_id: None,
                workspace: None,
            },
            EventPayload::RunStarted(RunStarted {
                prompt_recorded: false,
            }),
        )
    }
}
