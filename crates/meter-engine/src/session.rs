use std::collections::HashMap;
use std::path::{Path, PathBuf};

use meter_core::{EventPayload, HarnessKind, MeterEvent, SessionId, TicketId};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SessionKey {
    ticket_id: TicketId,
    harness: HarnessKind,
    workspace: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct SessionProjection {
    latest: HashMap<SessionKey, SessionId>,
}

impl SessionProjection {
    pub fn from_events(events: &[MeterEvent]) -> Self {
        let mut projection = Self::default();
        for event in events {
            if matches!(event.payload, EventPayload::SessionDiscovered(_))
                && let Some(session_id) = &event.session_id
            {
                projection.latest.insert(
                    SessionKey {
                        ticket_id: event.ticket_id.clone(),
                        harness: event.harness,
                        workspace: event.workspace.clone(),
                    },
                    session_id.clone(),
                );
            }
        }
        projection
    }

    pub fn lookup(
        &self,
        ticket_id: &TicketId,
        harness: HarnessKind,
        workspace: Option<&Path>,
    ) -> Option<&SessionId> {
        self.latest.get(&SessionKey {
            ticket_id: ticket_id.clone(),
            harness,
            workspace: workspace.map(Path::to_path_buf),
        })
    }
}

#[cfg(test)]
mod tests {
    use meter_core::{EventContext, EventPayload, MeterEvent, RunId, SessionDiscovered, WorkRef};

    use super::*;

    #[test]
    fn rebuilds_latest_session_from_events() {
        let ticket_id = TicketId::new("ENG-1").unwrap_or_else(|err| panic!("{err}"));
        let session_id = SessionId::new("thread-1").unwrap_or_else(|err| panic!("{err}"));
        let context = EventContext {
            run_id: RunId::new(),
            ticket_id: ticket_id.clone(),
            label: Some("label".to_owned()),
            harness: HarnessKind::Codex,
            requested_model: None,
            resolved_model: None,
            session_id: Some(session_id.clone()),
            workspace: Some(PathBuf::from(".")),
        };
        let _work = WorkRef {
            ticket_id: ticket_id.clone(),
            label: None,
        };
        let event = MeterEvent::new(
            context,
            EventPayload::SessionDiscovered(SessionDiscovered {
                source: "codex".to_owned(),
            }),
        );

        let projection = SessionProjection::from_events(&[event]);

        assert_eq!(
            projection.lookup(&ticket_id, HarnessKind::Codex, Some(Path::new("."))),
            Some(&session_id)
        );
    }
}
