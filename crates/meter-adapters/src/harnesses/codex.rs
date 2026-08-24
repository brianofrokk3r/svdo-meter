use std::ffi::OsString;
use std::path::Path;

use async_trait::async_trait;
use meter_core::{
    CodexApprovalMode, CodexConfig, CommandCompleted, CommandStarted, EventContext, EventPayload,
    FilesChanged, HarnessEvent, HarnessKind, MeterEvent, RawEventRetention, RunMetrics,
    SessionDiscovered, SessionId, TokenUsage, ToolCompleted, ToolStarted,
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
pub struct CodexAdapter {
    config: CodexConfig,
}

impl CodexAdapter {
    pub fn new(config: CodexConfig) -> Self {
        Self { config }
    }
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new(CodexConfig::default())
    }
}

#[async_trait]
impl HarnessAdapter for CodexAdapter {
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
        let mut command = Command::new(&self.config.binary);
        command.args(codex_argv(
            &self.config,
            request.context.workspace.as_deref(),
            request.model.as_ref(),
            request.session_id.as_ref(),
            &request.prompt,
        ));
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let mut child = command.spawn().map_err(HarnessError::Spawn)?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let mut normalizer =
            CodexEventNormalizer::new(request.context, request.raw_event_retention);

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
        let success = status.success() && normalizer.failure_reason.is_none();
        Ok(HarnessRunResult {
            success,
            session_id: normalizer.session_id,
            resolved_model: normalizer.resolved_model,
            metrics: normalizer.metrics,
            exit_code: status.code(),
            failure_reason: normalizer.failure_reason,
        })
    }
}

pub fn codex_argv(
    config: &CodexConfig,
    workspace: Option<&Path>,
    model: Option<&meter_core::ModelName>,
    session_id: Option<&SessionId>,
    prompt: &str,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("exec"), OsString::from("--json")];
    if let Some(workspace) = workspace {
        args.push(OsString::from("-C"));
        args.push(workspace.as_os_str().to_os_string());
    }
    if let Some(model) = model {
        args.push(OsString::from("--model"));
        args.push(OsString::from(model.as_str()));
    }
    if let Some(profile) = &config.profile {
        args.push(OsString::from("--profile"));
        args.push(OsString::from(profile));
    }
    if let Some(sandbox) = config.sandbox {
        args.push(OsString::from("--sandbox"));
        args.push(OsString::from(sandbox.as_str()));
    }
    if config.approval_mode == CodexApprovalMode::ApproveForMe {
        args.push(OsString::from("--approve-for-me"));
    }
    if config.yolo {
        args.push(OsString::from("--dangerously-bypass-approvals-and-sandbox"));
    }
    for override_value in &config.config_overrides {
        args.push(OsString::from("--config"));
        args.push(OsString::from(override_value.as_key_value()));
    }
    if let Some(session_id) = session_id {
        args.push(OsString::from("resume"));
        args.push(OsString::from(session_id.as_str()));
    }
    args.push(OsString::from(prompt));
    args
}

#[derive(Debug, Default)]
pub struct NormalizeOutcome {
    pub parsed_json: bool,
    pub events: Vec<MeterEvent>,
}

#[derive(Debug, Clone)]
pub struct CodexEventNormalizer {
    context: EventContext,
    retention: RawEventRetention,
    pub metrics: RunMetrics,
    pub session_id: Option<SessionId>,
    pub resolved_model: Option<meter_core::ModelName>,
    pub failure_reason: Option<String>,
}

impl CodexEventNormalizer {
    pub fn new(context: EventContext, retention: RawEventRetention) -> Self {
        let session_id = context.session_id.clone();
        Self {
            context,
            retention,
            metrics: RunMetrics::default(),
            session_id,
            resolved_model: None,
            failure_reason: None,
        }
    }

    pub fn normalize_line(&mut self, line: &str) -> NormalizeOutcome {
        let parsed = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(_) => {
                self.metrics.errors = self.metrics.errors.saturating_add(1);
                return NormalizeOutcome::default();
            }
        };
        self.metrics.provider_event_count = self.metrics.provider_event_count.saturating_add(1);
        let source_event = event_name(&parsed);
        let mut events = Vec::new();
        let context = self.context.with_session(self.session_id.clone());

        if let Some(session_id) = discover_session_id(&parsed)
            && self.session_id.as_ref() != Some(&session_id)
        {
            self.session_id = Some(session_id.clone());
            events.push(MeterEvent::new(
                self.context.with_session(Some(session_id)),
                EventPayload::SessionDiscovered(SessionDiscovered {
                    source: "codex".to_owned(),
                }),
            ));
        }
        if let Some(model) = string_field_any(&parsed, &["model", "resolved_model"])
            && let Ok(model) = meter_core::ModelName::new(model)
        {
            self.resolved_model = Some(model);
        }
        if let Some(usage) = token_usage(&parsed) {
            self.metrics.token_usage.add_assign(&usage);
            events.push(MeterEvent::new(
                context.clone(),
                EventPayload::UsageReported(usage),
            ));
        }

        match source_event.as_deref() {
            Some("command.started") | Some("exec.started") => {
                self.metrics.commands_executed = self.metrics.commands_executed.saturating_add(1);
                events.push(MeterEvent::new(
                    context.clone(),
                    EventPayload::CommandStarted(CommandStarted {
                        command_id: string_field_any(&parsed, &["command_id", "id"]),
                        command_kind: string_field_any(&parsed, &["command_kind", "kind"]),
                    }),
                ));
            }
            Some("command.completed") | Some("exec.completed") => {
                let success = bool_field(&parsed, "success").unwrap_or_else(|| {
                    int_field_any(&parsed, &["exit_code", "exit_status"]).unwrap_or(1) == 0
                });
                if !success {
                    self.metrics.failed_commands = self.metrics.failed_commands.saturating_add(1);
                }
                let duration_ms = u64_field_any(&parsed, &["duration_ms", "elapsed_ms"]);
                if let Some(duration_ms) = duration_ms {
                    self.metrics.command_time_ms =
                        self.metrics.command_time_ms.saturating_add(duration_ms);
                }
                events.push(MeterEvent::new(
                    context.clone(),
                    EventPayload::CommandCompleted(CommandCompleted {
                        command_id: string_field_any(&parsed, &["command_id", "id"]),
                        success,
                        exit_code: int_field_any(&parsed, &["exit_code", "exit_status"]),
                        duration_ms,
                    }),
                ));
            }
            Some("files.changed") | Some("file.changed") => {
                let count = u64_field_any(&parsed, &["count", "files_changed"])
                    .or_else(|| {
                        parsed
                            .get("files")
                            .and_then(Value::as_array)
                            .map(|files| files.len() as u64)
                    })
                    .unwrap_or(1);
                self.metrics.files_changed = self.metrics.files_changed.saturating_add(count);
                events.push(MeterEvent::new(
                    context.clone(),
                    EventPayload::FilesChanged(FilesChanged { count }),
                ));
            }
            Some("tool.started") | Some("tool.call.started") => {
                self.metrics.tool_calls = self.metrics.tool_calls.saturating_add(1);
                events.push(MeterEvent::new(
                    context.clone(),
                    EventPayload::ToolStarted(ToolStarted {
                        tool_id: string_field_any(&parsed, &["tool_id", "id"]),
                        tool_name: string_field_any(&parsed, &["tool_name", "name"]),
                    }),
                ));
            }
            Some("tool.completed") | Some("tool.call.completed") => {
                let success = bool_field(&parsed, "success").unwrap_or(true);
                if !success {
                    self.metrics.errors = self.metrics.errors.saturating_add(1);
                }
                let duration_ms = u64_field_any(&parsed, &["duration_ms", "elapsed_ms"]);
                if let Some(duration_ms) = duration_ms {
                    self.metrics.tool_time_ms =
                        self.metrics.tool_time_ms.saturating_add(duration_ms);
                }
                events.push(MeterEvent::new(
                    context.clone(),
                    EventPayload::ToolCompleted(ToolCompleted {
                        tool_id: string_field_any(&parsed, &["tool_id", "id"]),
                        tool_name: string_field_any(&parsed, &["tool_name", "name"]),
                        success,
                        duration_ms,
                    }),
                ));
            }
            Some("turn.completed") => {
                self.metrics.turn_count = self.metrics.turn_count.saturating_add(1);
                if let Some(active_ms) = u64_field_any(&parsed, &["active_time_ms", "duration_ms"])
                {
                    self.metrics.active_time_ms =
                        self.metrics.active_time_ms.saturating_add(active_ms);
                }
            }
            Some("run.failed") | Some("error") => {
                self.metrics.errors = self.metrics.errors.saturating_add(1);
                self.failure_reason = string_field_any(&parsed, &["message", "reason", "error"])
                    .or_else(|| Some("provider reported failure".to_owned()));
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

        NormalizeOutcome {
            parsed_json: true,
            events,
        }
    }
}

fn event_name(value: &Value) -> Option<String> {
    string_field_any(value, &["type", "event_type", "event"])
}

fn discover_session_id(value: &Value) -> Option<SessionId> {
    string_field_any(value, &["session_id", "thread_id", "conversation_id"])
        .and_then(|value| SessionId::new(value).ok())
}

fn token_usage(value: &Value) -> Option<TokenUsage> {
    let usage = value.get("usage").unwrap_or(value);
    let token_usage = TokenUsage {
        input_tokens: u64_field_any(usage, &["input_tokens", "prompt_tokens"]),
        cached_input_tokens: u64_field_any(usage, &["cached_input_tokens", "cache_read_tokens"]),
        cache_write_tokens: u64_field_any(usage, &["cache_write_tokens"]),
        output_tokens: u64_field_any(usage, &["output_tokens", "completion_tokens"]),
        reasoning_tokens: u64_field_any(usage, &["reasoning_tokens", "reasoning_output_tokens"]),
    };
    if token_usage == TokenUsage::default() {
        None
    } else {
        Some(token_usage)
    }
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

fn int_field_any(value: &Value, keys: &[&str]) -> Option<i32> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
    })
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
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
            harness: HarnessKind::Codex,
            requested_model: None,
            resolved_model: None,
            session_id: None,
            workspace: Some(PathBuf::from(".")),
        }
    }

    #[test]
    fn parses_session_usage_and_command_metrics() {
        let mut normalizer = CodexEventNormalizer::new(context(), RawEventRetention::Disabled);

        let session = normalizer.normalize_line(r#"{"type":"session.created","session_id":"abc"}"#);
        let usage_line = concat!(
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"#,
            r#""cached_input_tokens":4,"output_tokens":3,"reasoning_tokens":2}}"#
        );
        let command_line = concat!(
            r#"{"type":"command.completed","command_id":"cmd1","success":false,"#,
            r#""exit_code":1,"duration_ms":12}"#
        );
        let usage = normalizer.normalize_line(usage_line);
        let command = normalizer.normalize_line(command_line);

        assert_eq!(session.events[0].event_type, EventType::SessionDiscovered);
        assert_eq!(usage.events[0].event_type, EventType::UsageReported);
        assert_eq!(command.events[0].event_type, EventType::CommandCompleted);
        assert_eq!(normalizer.metrics.failed_commands, 1);
        assert_eq!(normalizer.metrics.command_time_ms, 12);
        assert_eq!(normalizer.metrics.token_usage.input_tokens, Some(10));
    }

    #[test]
    fn tolerates_unknown_malformed_and_missing_usage() {
        let mut normalizer = CodexEventNormalizer::new(context(), RawEventRetention::Disabled);

        let unknown = normalizer.normalize_line(r#"{"type":"new.future.event","foo":"bar"}"#);
        let malformed = normalizer.normalize_line("{not json");
        let no_usage = normalizer.normalize_line(r#"{"type":"turn.completed"}"#);

        assert!(unknown.parsed_json);
        assert!(unknown.events.is_empty());
        assert!(!malformed.parsed_json);
        assert!(no_usage.parsed_json);
        assert_eq!(normalizer.metrics.provider_event_count, 2);
        assert_eq!(normalizer.metrics.errors, 1);
    }

    #[test]
    fn raw_payload_requires_explicit_retention() {
        let mut disabled = CodexEventNormalizer::new(context(), RawEventRetention::Disabled);
        let mut enabled = CodexEventNormalizer::new(context(), RawEventRetention::Full);

        let disabled_events = disabled.normalize_line(r#"{"type":"unknown","secret":"nope"}"#);
        let enabled_events = enabled.normalize_line(r#"{"type":"unknown","secret":"kept"}"#);

        assert!(disabled_events.events.is_empty());
        assert_eq!(enabled_events.events[0].event_type, EventType::HarnessEvent);
    }

    #[test]
    fn builds_explicit_codex_arguments_for_resume_model_and_workspace() {
        let model = meter_core::ModelName::new("gpt-5").unwrap_or_else(|err| panic!("{err}"));
        let session = SessionId::new("session-1").unwrap_or_else(|err| panic!("{err}"));

        let args = codex_argv(
            &CodexConfig::default(),
            Some(Path::new("/tmp/work space")),
            Some(&model),
            Some(&session),
            "Fix tests",
        );

        assert_eq!(
            args,
            vec![
                OsString::from("exec"),
                OsString::from("--json"),
                OsString::from("-C"),
                OsString::from("/tmp/work space"),
                OsString::from("--model"),
                OsString::from("gpt-5"),
                OsString::from("resume"),
                OsString::from("session-1"),
                OsString::from("Fix tests"),
            ]
        );
    }

    #[test]
    fn builds_codex_arguments_for_profile_sandbox_approval_and_yolo() {
        let config = CodexConfig {
            profile: Some("default".to_owned()),
            sandbox: Some(meter_core::CodexSandboxMode::WorkspaceWrite),
            approval_mode: CodexApprovalMode::ApproveForMe,
            yolo: true,
            ..CodexConfig::default()
        };

        let args = codex_argv(&config, None, None, None, "Prototype");

        assert_eq!(
            args,
            vec![
                OsString::from("exec"),
                OsString::from("--json"),
                OsString::from("--profile"),
                OsString::from("default"),
                OsString::from("--sandbox"),
                OsString::from("workspace-write"),
                OsString::from("--approve-for-me"),
                OsString::from("--dangerously-bypass-approvals-and-sandbox"),
                OsString::from("Prototype"),
            ]
        );
    }

    #[test]
    fn builds_repeated_codex_config_overrides_before_final_prompt() {
        let config = CodexConfig {
            config_overrides: vec![
                meter_core::CodexConfigOverride::new("model_reasoning_effort", "high"),
                meter_core::CodexConfigOverride::new("features.foo", "true"),
            ],
            ..CodexConfig::default()
        };

        let args = codex_argv(&config, Some(Path::new(".")), None, None, "Do work");

        assert_eq!(args.last(), Some(&OsString::from("Do work")));
        assert_eq!(
            args,
            vec![
                OsString::from("exec"),
                OsString::from("--json"),
                OsString::from("-C"),
                OsString::from("."),
                OsString::from("--config"),
                OsString::from("model_reasoning_effort=high"),
                OsString::from("--config"),
                OsString::from("features.foo=true"),
                OsString::from("Do work"),
            ]
        );
    }

    #[test]
    fn fixture_success_covers_discovery_usage_files_tool_and_completion() {
        let fixture = include_str!("../../../../tests/fixtures/codex/successful_run.jsonl");
        let mut normalizer = CodexEventNormalizer::new(context(), RawEventRetention::Disabled);
        let mut event_types = Vec::new();

        for line in fixture.lines() {
            let outcome = normalizer.normalize_line(line);
            event_types.extend(outcome.events.into_iter().map(|event| event.event_type));
        }

        assert!(event_types.contains(&EventType::SessionDiscovered));
        assert!(event_types.contains(&EventType::UsageReported));
        assert!(event_types.contains(&EventType::CommandStarted));
        assert!(event_types.contains(&EventType::CommandCompleted));
        assert!(event_types.contains(&EventType::FilesChanged));
        assert!(event_types.contains(&EventType::ToolStarted));
        assert!(event_types.contains(&EventType::ToolCompleted));
        assert_eq!(normalizer.metrics.commands_executed, 1);
        assert_eq!(normalizer.metrics.files_changed, 2);
        assert_eq!(normalizer.metrics.tool_calls, 1);
        assert_eq!(normalizer.metrics.turn_count, 1);
        assert_eq!(normalizer.metrics.token_usage.output_tokens, Some(80));
    }

    #[test]
    fn fixture_failure_and_interrupted_paths_record_errors() {
        let failure = include_str!("../../../../tests/fixtures/codex/failed_run.jsonl");
        let interrupted = include_str!("../../../../tests/fixtures/codex/interrupted_run.jsonl");
        let mut failed = CodexEventNormalizer::new(context(), RawEventRetention::Disabled);
        let mut partial = CodexEventNormalizer::new(context(), RawEventRetention::Disabled);

        for line in failure.lines() {
            let _ = failed.normalize_line(line);
        }
        for line in interrupted.lines() {
            let _ = partial.normalize_line(line);
        }

        assert_eq!(failed.metrics.failed_commands, 1);
        assert!(failed.failure_reason.is_some());
        assert_eq!(partial.metrics.errors, 1);
        assert_eq!(partial.metrics.provider_event_count, 1);
    }
}
