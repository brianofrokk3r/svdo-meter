use std::ffi::OsString;
use std::path::Path;

use async_trait::async_trait;
use meter_core::{
    EventContext, EventPayload, ExecutionPermissionMode, HarnessEvent, HarnessKind, MeterEvent,
    OpenCodeConfig, RawEventRetention, RunMetrics, SessionDiscovered, SessionId, TokenUsage,
    ToolCompleted,
};
use meter_engine::{
    EventSender, HarnessAdapter, HarnessCapabilities, HarnessError, HarnessRunRequest,
    HarnessRunResult,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const MAX_PROVIDER_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct OpenCodeAdapter {
    config: OpenCodeConfig,
}

impl OpenCodeAdapter {
    pub fn new(config: OpenCodeConfig) -> Self {
        Self { config }
    }
}

impl Default for OpenCodeAdapter {
    fn default() -> Self {
        Self::new(OpenCodeConfig::default())
    }
}

#[async_trait]
impl HarnessAdapter for OpenCodeAdapter {
    fn kind(&self) -> HarnessKind {
        HarnessKind::OpenCode
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
        reject_unsupported_options(&request)?;

        let mut command = Command::new(&self.config.binary);
        command.args(opencode_argv(
            request.context.workspace.as_deref(),
            request.model.as_ref(),
            request.session_id.as_ref(),
            request.execution_permission,
            self.config.agent.as_deref(),
            &request.prompt,
        ));
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let mut child = command.spawn().map_err(HarnessError::Spawn)?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let mut normalizer =
            OpenCodeEventNormalizer::new(request.context, request.raw_event_retention);

        let stderr_task = stderr.map(|stderr| {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Some(line) = lines.next_line().await? {
                    eprintln!("{line}");
                }
                Ok::<(), std::io::Error>(())
            })
        });

        if let Some(stdout) = stdout {
            let mut lines = BufReader::new(stdout).lines();
            while let Some(line) = lines.next_line().await.map_err(HarnessError::Io)? {
                if line.len() > MAX_PROVIDER_LINE_BYTES {
                    normalizer.metrics.errors = normalizer.metrics.errors.saturating_add(1);
                    continue;
                }
                let outcome = normalizer.normalize_line(&line);
                if !outcome.parsed_json {
                    println!("{line}");
                }
                for event in outcome.events {
                    events.send(event).await.map_err(|_| {
                        HarnessError::Io(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "event writer closed",
                        ))
                    })?;
                }
            }
        }

        if let Some(task) = stderr_task {
            task.await
                .map_err(|_| HarnessError::Interrupted)?
                .map_err(HarnessError::Io)?;
        }
        let status = child.wait().await.map_err(HarnessError::Io)?;

        Ok(HarnessRunResult {
            success: status.success(),
            session_id: normalizer.session_id,
            resolved_model: normalizer.resolved_model,
            metrics: normalizer.metrics,
            exit_code: status.code(),
            failure_reason: if status.success() {
                None
            } else {
                Some("OpenCode process exited unsuccessfully".to_owned())
            },
        })
    }
}

pub fn opencode_argv(
    workspace: Option<&Path>,
    model: Option<&meter_core::ModelName>,
    session_id: Option<&SessionId>,
    execution_permission: ExecutionPermissionMode,
    agent: Option<&str>,
    prompt: &str,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("run"),
        OsString::from("--format"),
        OsString::from("json"),
    ];
    if let Some(workspace) = workspace {
        args.push(OsString::from("--dir"));
        args.push(workspace.as_os_str().to_os_string());
    }
    if let Some(model) = model {
        args.push(OsString::from("--model"));
        args.push(OsString::from(model.as_str()));
    }
    if let Some(session_id) = session_id {
        args.push(OsString::from("--session"));
        args.push(OsString::from(session_id.as_str()));
    }
    if execution_permission == ExecutionPermissionMode::DangerousBypass {
        args.push(OsString::from("--auto"));
    }
    if let Some(agent) = agent {
        args.push(OsString::from("--agent"));
        args.push(OsString::from(agent));
    }
    args.push(OsString::from(prompt));
    args
}

fn reject_unsupported_options(request: &HarnessRunRequest) -> Result<(), HarnessError> {
    if !request.options.values().is_empty() {
        return Err(HarnessError::UnsupportedConfig(
            "OpenCode does not accept harness-specific options yet".to_owned(),
        ));
    }
    if matches!(request.raw_event_retention, RawEventRetention::Full) {
        return Err(HarnessError::UnsupportedConfig(
            "OpenCode raw event retention is not implemented yet".to_owned(),
        ));
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct OpenCodeNormalizeOutcome {
    pub parsed_json: bool,
    pub events: Vec<MeterEvent>,
}

#[derive(Debug, Clone)]
pub struct OpenCodeEventNormalizer {
    context: EventContext,
    retention: RawEventRetention,
    pub metrics: RunMetrics,
    pub session_id: Option<SessionId>,
    pub resolved_model: Option<meter_core::ModelName>,
}

impl OpenCodeEventNormalizer {
    pub fn new(context: EventContext, retention: RawEventRetention) -> Self {
        let session_id = context.session_id.clone();
        Self {
            context,
            retention,
            metrics: RunMetrics::default(),
            session_id,
            resolved_model: None,
        }
    }

    pub fn normalize_line(&mut self, line: &str) -> OpenCodeNormalizeOutcome {
        let parsed = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(_) => {
                self.metrics.errors = self.metrics.errors.saturating_add(1);
                return OpenCodeNormalizeOutcome::default();
            }
        };
        self.metrics.provider_event_count = self.metrics.provider_event_count.saturating_add(1);
        let source_event = event_name(&parsed);
        let mut events = Vec::new();
        let discovered_session = if let Some(session_id) = discover_session_id(&parsed)
            && self.session_id.as_ref() != Some(&session_id)
        {
            self.session_id = Some(session_id);
            true
        } else {
            false
        };

        if let Some(model) = discover_model(&parsed)
            && let Ok(model) = meter_core::ModelName::new(model)
        {
            self.resolved_model = Some(model);
        }

        let context = self
            .context
            .with_session(self.session_id.clone())
            .with_resolved_model(self.resolved_model.clone());
        if discovered_session {
            events.push(MeterEvent::new(
                context.clone(),
                EventPayload::SessionDiscovered(SessionDiscovered {
                    source: "opencode".to_owned(),
                }),
            ));
        }

        if let Some(usage) = token_usage(&parsed) {
            self.metrics.token_usage.add_assign(&usage);
            events.push(MeterEvent::new(
                context.clone(),
                EventPayload::UsageReported(usage),
            ));
        }

        match source_event.as_deref() {
            Some("step_finish") => {
                self.metrics.turn_count = self.metrics.turn_count.saturating_add(1);
                if let Some(active_ms) = duration_ms(&parsed) {
                    self.metrics.active_time_ms =
                        self.metrics.active_time_ms.saturating_add(active_ms);
                }
            }
            Some("tool_use") => {
                let part = parsed.get("part").unwrap_or(&parsed);
                self.metrics.tool_calls = self.metrics.tool_calls.saturating_add(1);
                if let Some(duration_ms) = duration_ms(&parsed) {
                    self.metrics.tool_time_ms =
                        self.metrics.tool_time_ms.saturating_add(duration_ms);
                }
                let success = !matches!(
                    string_field_any(part, &["status", "state", "error"]).as_deref(),
                    Some("error" | "failed")
                );
                if !success {
                    self.metrics.errors = self.metrics.errors.saturating_add(1);
                }
                events.push(MeterEvent::new(
                    context.clone(),
                    EventPayload::ToolCompleted(ToolCompleted {
                        tool_id: string_field_any(part, &["id", "tool_id", "toolID"]),
                        tool_name: string_field_any(part, &["tool", "tool_name", "name"]),
                        success,
                        duration_ms: duration_ms(&parsed),
                    }),
                ));
            }
            Some("error") => {
                self.metrics.errors = self.metrics.errors.saturating_add(1);
            }
            _ => {}
        }

        if matches!(self.retention, RawEventRetention::Full) {
            events.push(MeterEvent::new(
                context,
                EventPayload::HarnessEvent(HarnessEvent {
                    source_event: source_event.unwrap_or_else(|| "unknown".to_owned()),
                    retained_raw_payload: true,
                    raw_payload: Some(parsed),
                }),
            ));
        }

        OpenCodeNormalizeOutcome {
            parsed_json: true,
            events,
        }
    }
}

fn event_name(value: &Value) -> Option<String> {
    string_field_any(value, &["type", "event_type", "event"])
}

fn discover_session_id(value: &Value) -> Option<SessionId> {
    string_field_any(
        value,
        &["sessionID", "session_id", "thread_id", "conversation_id"],
    )
    .or_else(|| {
        value
            .pointer("/session/id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    })
    .or_else(|| {
        value
            .get("part")
            .and_then(|part| string_field_any(part, &["sessionID", "session_id"]))
    })
    .and_then(|value| SessionId::new(value).ok())
}

fn discover_model(value: &Value) -> Option<String> {
    string_field_any(value, &["model", "resolved_model"])
        .or_else(|| {
            value
                .pointer("/message/model")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            value
                .pointer("/part/model")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

fn token_usage(value: &Value) -> Option<TokenUsage> {
    let usage = value
        .get("usage")
        .or_else(|| value.pointer("/part/tokens"))
        .or_else(|| value.pointer("/part/usage"))
        .unwrap_or(value);
    let cache = usage.get("cache").unwrap_or(usage);
    let token_usage = TokenUsage {
        input_tokens: u64_field_any(usage, &["input_tokens", "prompt_tokens", "input"]),
        cached_input_tokens: u64_field_any(
            usage,
            &["cached_input_tokens", "cache_read_tokens", "cache_read"],
        )
        .or_else(|| u64_field_any(cache, &["read"])),
        cache_write_tokens: u64_field_any(usage, &["cache_write_tokens", "cache_write"])
            .or_else(|| u64_field_any(cache, &["write"])),
        output_tokens: u64_field_any(usage, &["output_tokens", "completion_tokens", "output"]),
        reasoning_tokens: u64_field_any(usage, &["reasoning_tokens", "reasoning"]),
    };
    if token_usage == TokenUsage::default() {
        None
    } else {
        Some(token_usage)
    }
}

fn duration_ms(value: &Value) -> Option<u64> {
    u64_field_any(value, &["duration_ms", "elapsed_ms"]).or_else(|| {
        let time = value.pointer("/part/time")?;
        let start = time.get("start")?.as_u64()?;
        let end = time.get("end")?.as_u64()?;
        end.checked_sub(start)
    })
}

fn string_field_any(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
    })
}

fn u64_field_any(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

#[cfg(test)]
mod tests {
    use meter_core::{EventType, RunId, TicketId};
    use std::path::PathBuf;

    use super::*;

    fn context() -> EventContext {
        EventContext {
            run_id: RunId::new(),
            ticket_id: TicketId::new("ENG-142").unwrap_or_else(|err| panic!("{err}")),
            label: Some("Password reset".to_owned()),
            harness: HarnessKind::OpenCode,
            requested_model: None,
            resolved_model: None,
            session_id: None,
            workspace: Some(PathBuf::from(".")),
        }
    }

    #[test]
    fn builds_base_opencode_run_arguments() {
        let args = opencode_argv(
            None,
            None,
            None,
            ExecutionPermissionMode::Standard,
            None,
            "Fix tests",
        );

        assert_eq!(
            args,
            vec![
                OsString::from("run"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from("Fix tests"),
            ]
        );
    }

    #[test]
    fn builds_opencode_arguments_for_workspace_model_and_auto() {
        let model = meter_core::ModelName::new("github-copilot/gpt-5")
            .unwrap_or_else(|err| panic!("{err}"));
        let session_id = SessionId::new("ses_existing").unwrap_or_else(|err| panic!("{err}"));

        let args = opencode_argv(
            Some(Path::new("/tmp/work space")),
            Some(&model),
            Some(&session_id),
            ExecutionPermissionMode::DangerousBypass,
            Some("build"),
            "Implement feature",
        );

        assert_eq!(
            args,
            vec![
                OsString::from("run"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from("--dir"),
                OsString::from("/tmp/work space"),
                OsString::from("--model"),
                OsString::from("github-copilot/gpt-5"),
                OsString::from("--session"),
                OsString::from("ses_existing"),
                OsString::from("--auto"),
                OsString::from("--agent"),
                OsString::from("build"),
                OsString::from("Implement feature"),
            ]
        );
    }

    #[test]
    fn parses_session_usage_and_tool_metrics() {
        let mut normalizer = OpenCodeEventNormalizer::new(context(), RawEventRetention::Disabled);

        let started = normalizer.normalize_line(
            r#"{"type":"step_start","sessionID":"ses_123","model":"github-copilot/gpt-5"}"#,
        );
        let tool = normalizer.normalize_line(
            r#"{"type":"tool_use","sessionID":"ses_123","part":{"id":"prt_1","tool":"bash","status":"completed","time":{"start":10,"end":25}}}"#,
        );
        let finished = normalizer.normalize_line(
            r#"{"type":"step_finish","sessionID":"ses_123","part":{"type":"step-finish","tokens":{"input":10,"output":3,"reasoning":2,"cache":{"read":4,"write":1}},"time":{"start":100,"end":140}}}"#,
        );

        assert_eq!(started.events[0].event_type, EventType::SessionDiscovered);
        assert_eq!(tool.events[0].event_type, EventType::ToolCompleted);
        assert_eq!(finished.events[0].event_type, EventType::UsageReported);
        assert_eq!(normalizer.metrics.provider_event_count, 3);
        assert_eq!(normalizer.metrics.tool_calls, 1);
        assert_eq!(normalizer.metrics.tool_time_ms, 15);
        assert_eq!(normalizer.metrics.turn_count, 1);
        assert_eq!(normalizer.metrics.active_time_ms, 40);
        assert_eq!(normalizer.metrics.token_usage.input_tokens, Some(10));
        assert_eq!(
            normalizer
                .resolved_model
                .as_ref()
                .map(|model| model.as_str()),
            Some("github-copilot/gpt-5")
        );
    }
}
