use std::path::{Path, PathBuf};

use async_trait::async_trait;
use meter_core::{EventPayload, HarnessKind, MeterEvent, SessionId, TicketId};
use meter_engine::{EventQuery, EventStore, StoreError};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const MAX_JSONL_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct JsonlEventStore {
    path: PathBuf,
}

impl JsonlEventStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_under(workspace: &Path) -> Self {
        Self::new(workspace.join(".svdo").join("meter.jsonl"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[async_trait]
impl EventStore for JsonlEventStore {
    async fn append(&self, event: &MeterEvent) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = open_append(&self.path).await?;
        let mut line = serde_json::to_vec(event)?;
        line.push(b'\n');
        file.write_all(&line).await?;
        file.flush().await?;
        Ok(())
    }

    async fn stream(&self, query: EventQuery) -> Result<Vec<MeterEvent>, StoreError> {
        let file = match tokio::fs::File::open(&self.path).await {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(StoreError::Io(err)),
        };
        let mut lines = BufReader::new(file).lines();
        let mut events = Vec::new();
        while let Some(line) = lines.next_line().await? {
            if line.len() > MAX_JSONL_LINE_BYTES {
                continue;
            }
            match serde_json::from_str::<MeterEvent>(&line) {
                Ok(event) if matches_query(&event, &query) => events.push(event),
                Ok(_) => {}
                Err(_) => {
                    // A crashed append can leave a final truncated line.
                    continue;
                }
            }
        }
        Ok(events)
    }
}

async fn open_append(path: &Path) -> Result<tokio::fs::File, std::io::Error> {
    let mut options = tokio::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    options.open(path).await
}

fn matches_query(event: &MeterEvent, query: &EventQuery) -> bool {
    if let Some(ticket_id) = &query.ticket_id
        && &event.ticket_id != ticket_id
    {
        return false;
    }
    if let Some(harness) = query.harness
        && event.harness != harness
    {
        return false;
    }
    if let Some(workspace) = &query.workspace
        && event.workspace.as_ref() != Some(workspace)
    {
        return false;
    }
    true
}

#[derive(Debug, Default)]
pub struct RebuiltSessionRegistry {
    entries: Vec<(TicketId, HarnessKind, Option<PathBuf>, SessionId)>,
}

impl RebuiltSessionRegistry {
    pub fn from_events(events: &[MeterEvent]) -> Self {
        let mut entries = Vec::new();
        for event in events {
            if matches!(event.payload, EventPayload::SessionDiscovered(_))
                && let Some(session_id) = &event.session_id
            {
                entries.push((
                    event.ticket_id.clone(),
                    event.harness,
                    event.workspace.clone(),
                    session_id.clone(),
                ));
            }
        }
        Self { entries }
    }

    pub fn lookup(
        &self,
        ticket_id: &TicketId,
        harness: HarnessKind,
        workspace: Option<&Path>,
    ) -> Option<&SessionId> {
        let workspace = workspace.map(Path::to_path_buf);
        self.entries.iter().rev().find_map(|entry| {
            let (entry_ticket, entry_harness, entry_workspace, session_id) = entry;
            if entry_ticket == ticket_id
                && *entry_harness == harness
                && *entry_workspace == workspace
            {
                Some(session_id)
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use meter_core::{
        EventContext, EventPayload, MeterEvent, RunId, RunStarted, SessionDiscovered,
    };
    use meter_engine::EventStore;

    use super::*;

    #[tokio::test]
    async fn appends_and_replays_jsonl_events() {
        let dir = tempfile::tempdir().unwrap_or_else(|err| panic!("{err}"));
        let store = JsonlEventStore::new(dir.path().join(".svdo").join("meter.jsonl"));
        let ticket_id = TicketId::new("ENG-1").unwrap_or_else(|err| panic!("{err}"));
        let context = EventContext {
            run_id: RunId::new(),
            ticket_id: ticket_id.clone(),
            label: None,
            harness: HarnessKind::Codex,
            requested_model: None,
            resolved_model: None,
            session_id: None,
            workspace: Some(dir.path().to_path_buf()),
        };
        let event = MeterEvent::new(
            context,
            EventPayload::RunStarted(RunStarted {
                prompt_recorded: false,
            }),
        );

        store
            .append(&event)
            .await
            .unwrap_or_else(|err| panic!("{err}"));
        let replayed = store
            .stream(EventQuery::default())
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(replayed, vec![event]);
    }

    #[tokio::test]
    async fn registry_is_rebuildable_from_session_events() {
        let dir = tempfile::tempdir().unwrap_or_else(|err| panic!("{err}"));
        let ticket_id = TicketId::new("ENG-2").unwrap_or_else(|err| panic!("{err}"));
        let session_id = SessionId::new("session-2").unwrap_or_else(|err| panic!("{err}"));
        let context = EventContext {
            run_id: RunId::new(),
            ticket_id: ticket_id.clone(),
            label: None,
            harness: HarnessKind::Codex,
            requested_model: None,
            resolved_model: None,
            session_id: Some(session_id.clone()),
            workspace: Some(dir.path().to_path_buf()),
        };
        let events = vec![MeterEvent::new(
            context,
            EventPayload::SessionDiscovered(SessionDiscovered {
                source: "codex".to_owned(),
            }),
        )];

        let registry = RebuiltSessionRegistry::from_events(&events);

        assert_eq!(
            registry.lookup(&ticket_id, HarnessKind::Codex, Some(dir.path())),
            Some(&session_id)
        );
    }
}
