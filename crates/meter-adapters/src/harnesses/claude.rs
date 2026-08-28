use std::ffi::OsString;
use std::path::PathBuf;

use async_trait::async_trait;
use meter_core::{
    ClaudeRunOptions, EventContext, EventPayload, HarnessEvent, HarnessKind, MeterEvent,
    RawEventRetention, RunMetrics, SessionDiscovered, SessionId, TokenUsage, ToolCompleted,
    ToolStarted,
};
use meter_engine::{
    EventSender, HarnessAdapter, HarnessCapabilities, HarnessError, HarnessOptions,
    HarnessRunRequest, HarnessRunResult,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const MAX_PROVIDER_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ClaudeAdapter {
    binary: PathBuf,
}

impl ClaudeAdapter {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
        }
    }
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self::new("claude")
    }
}

#[async_trait]
impl HarnessAdapter for ClaudeAdapter {
    fn kind(&self) -> HarnessKind {
        HarnessKind::Claude
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
        let options = claude_options(&request.options)?;
        let argv = claude_argv(
            request.model.as_ref(),
            request.session_id.as_ref(),
            &options,
            &request.prompt,
        )?;
        let mut command = Command::new(&self.binary);
        command.args(argv);
        if let Some(workspace) = &request.context.workspace {
            command.current_dir(workspace);
        }
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let mut child = command.spawn().map_err(HarnessError::Spawn)?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let mut normalizer =
            ClaudeEventNormalizer::new(request.context, request.raw_event_retention);
        if let Some(session_id) = explicit_session_id(&options)? {
            normalizer.session_id = Some(session_id.clone());
            events
                .send(MeterEvent::new(
                    normalizer.context.with_session(Some(session_id)),
                    EventPayload::SessionDiscovered(SessionDiscovered {
                        source: "claude_session_id".to_owned(),
                    }),
                ))
                .await
                .map_err(|_| {
                    HarnessError::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "event writer closed",
                    ))
                })?;
        }

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

pub fn claude_argv(
    model: Option<&meter_core::ModelName>,
    resume_session: Option<&SessionId>,
    options: &ClaudeRunOptions,
    prompt: &str,
) -> Result<Vec<OsString>, HarnessError> {
    validate_options(resume_session, options)?;

    let mut args = vec![OsString::from("-p"), OsString::from(prompt)];

    if options.continue_latest {
        args.push(OsString::from("--continue"));
    }
    if let Some(session) = resume_session {
        args.push(OsString::from("--resume"));
        args.push(OsString::from(session.as_str()));
    }
    if let Some(session) = &options.resume {
        args.push(OsString::from("--resume"));
        args.push(OsString::from(session));
    }
    if let Some(session_id) = &options.session_id {
        args.push(OsString::from("--session-id"));
        args.push(OsString::from(session_id));
    }
    if options.fork_session {
        args.push(OsString::from("--fork-session"));
    }
    args.push(OsString::from("--output-format"));
    args.push(OsString::from("stream-json"));
    args.push(OsString::from("--verbose"));
    if let Some(model) = model {
        args.push(OsString::from("--model"));
        args.push(OsString::from(model.as_str()));
    }
    if let Some(permission_mode) = &options.permission_mode {
        args.push(OsString::from("--permission-mode"));
        args.push(OsString::from(permission_mode));
    }
    push_repeated_values(&mut args, "--allowed-tools", &options.allowed_tools);
    push_repeated_values(&mut args, "--disallowed-tools", &options.disallowed_tools);
    push_repeated_paths(&mut args, "--add-dir", &options.add_dirs);
    push_repeated_values(&mut args, "--mcp-config", &options.mcp_configs);
    if options.strict_mcp_config {
        args.push(OsString::from("--strict-mcp-config"));
    }
    if let Some(settings) = &options.settings {
        args.push(OsString::from("--settings"));
        args.push(OsString::from(settings));
    }
    if let Some(setting_sources) = &options.setting_sources {
        args.push(OsString::from("--setting-sources"));
        args.push(OsString::from(setting_sources));
    }
    if let Some(system_prompt) = &options.system_prompt {
        args.push(OsString::from("--system-prompt"));
        args.push(OsString::from(system_prompt));
    }
    if let Some(system_prompt_file) = &options.system_prompt_file {
        args.push(OsString::from("--system-prompt-file"));
        args.push(system_prompt_file.as_os_str().to_os_string());
    }
    push_repeated_values(
        &mut args,
        "--append-system-prompt",
        &options.append_system_prompts,
    );
    push_repeated_paths(
        &mut args,
        "--append-system-prompt-file",
        &options.append_system_prompt_files,
    );
    if let Some(max_turns) = options.max_turns {
        args.push(OsString::from("--max-turns"));
        args.push(OsString::from(max_turns.to_string()));
    }
    if let Some(max_budget_usd) = &options.max_budget_usd {
        args.push(OsString::from("--max-budget-usd"));
        args.push(OsString::from(max_budget_usd));
    }
    Ok(args)
}

fn claude_options(options: &HarnessOptions) -> Result<ClaudeRunOptions, HarnessError> {
    serde_json::from_value(Value::Object(options.values().clone())).map_err(|error| {
        HarnessError::UnsupportedConfig(format!("invalid Claude options: {error}"))
    })
}

fn validate_options(
    resume_session: Option<&SessionId>,
    options: &ClaudeRunOptions,
) -> Result<(), HarnessError> {
    let resume_count = usize::from(resume_session.is_some())
        + usize::from(options.resume.is_some())
        + usize::from(options.continue_latest);
    if resume_count > 1 {
        return Err(HarnessError::UnsupportedConfig(
            "choose only one of --session, --claude-resume, or --claude-continue".to_owned(),
        ));
    }
    if options.session_id.is_some() && resume_count > 0 {
        return Err(HarnessError::UnsupportedConfig(
            "--claude-session-id cannot be combined with resume or continue".to_owned(),
        ));
    }
    let _ = explicit_session_id(options)?;
    if options.fork_session && resume_count == 0 {
        return Err(HarnessError::UnsupportedConfig(
            "--claude-fork-session requires --session, --claude-resume, or --claude-continue"
                .to_owned(),
        ));
    }
    if options.system_prompt.is_some() && options.system_prompt_file.is_some() {
        return Err(HarnessError::UnsupportedConfig(
            "--claude-system-prompt cannot be combined with --claude-system-prompt-file".to_owned(),
        ));
    }
    if options.strict_mcp_config && options.mcp_configs.is_empty() {
        return Err(HarnessError::UnsupportedConfig(
            "--claude-strict-mcp-config requires at least one --claude-mcp-config".to_owned(),
        ));
    }
    if options.max_turns == Some(0) {
        return Err(HarnessError::UnsupportedConfig(
            "--claude-max-turns must be greater than zero".to_owned(),
        ));
    }
    Ok(())
}

fn explicit_session_id(options: &ClaudeRunOptions) -> Result<Option<SessionId>, HarnessError> {
    options
        .session_id
        .as_ref()
        .map(|session_id| {
            uuid::Uuid::parse_str(session_id).map_err(|error| {
                HarnessError::UnsupportedConfig(format!(
                    "invalid --claude-session-id value: {error}"
                ))
            })?;
            SessionId::new(session_id.clone()).map_err(|error| {
                HarnessError::UnsupportedConfig(format!(
                    "invalid --claude-session-id value: {error}"
                ))
            })
        })
        .transpose()
}

fn push_repeated_values(args: &mut Vec<OsString>, flag: &str, values: &[String]) {
    for value in values {
        args.push(OsString::from(flag));
        args.push(OsString::from(value));
    }
}

fn push_repeated_paths(args: &mut Vec<OsString>, flag: &str, values: &[PathBuf]) {
    for value in values {
        args.push(OsString::from(flag));
        args.push(value.as_os_str().to_os_string());
    }
}

#[derive(Debug, Default)]
pub struct ClaudeNormalizeOutcome {
    pub parsed_json: bool,
    pub events: Vec<MeterEvent>,
}

#[derive(Debug, Clone)]
pub struct ClaudeEventNormalizer {
    context: EventContext,
    retention: RawEventRetention,
    pub metrics: RunMetrics,
    pub session_id: Option<SessionId>,
    pub resolved_model: Option<meter_core::ModelName>,
    pub failure_reason: Option<String>,
}

impl ClaudeEventNormalizer {
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

    pub fn normalize_line(&mut self, line: &str) -> ClaudeNormalizeOutcome {
        let parsed = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(_) => {
                self.metrics.errors = self.metrics.errors.saturating_add(1);
                return ClaudeNormalizeOutcome::default();
            }
        };
        self.metrics.provider_event_count = self.metrics.provider_event_count.saturating_add(1);
        let source_event = event_name(&parsed);
        let mut events = Vec::new();

        if let Some(session_id) = discover_session_id(&parsed)
            && self.session_id.as_ref() != Some(&session_id)
        {
            self.session_id = Some(session_id.clone());
            events.push(MeterEvent::new(
                self.context.with_session(Some(session_id)),
                EventPayload::SessionDiscovered(SessionDiscovered {
                    source: "claude".to_owned(),
                }),
            ));
        }
        if let Some(model) = string_field_any_path(&parsed, &[&["model"], &["message", "model"]])
            && let Ok(model) = meter_core::ModelName::new(model)
        {
            self.resolved_model = Some(model);
        }

        let context = self
            .context
            .with_session(self.session_id.clone())
            .with_resolved_model(self.resolved_model.clone());
        if let Some(usage) = token_usage(&parsed) {
            self.metrics.token_usage.add_assign(&usage);
            events.push(MeterEvent::new(
                context.clone(),
                EventPayload::UsageReported(usage),
            ));
        }

        match source_event.as_deref() {
            Some("assistant") => {
                for tool in assistant_tool_uses(&parsed) {
                    self.metrics.tool_calls = self.metrics.tool_calls.saturating_add(1);
                    events.push(MeterEvent::new(
                        context.clone(),
                        EventPayload::ToolStarted(tool),
                    ));
                }
            }
            Some("user") => {
                for tool in user_tool_results(&parsed) {
                    if !tool.success {
                        self.metrics.errors = self.metrics.errors.saturating_add(1);
                    }
                    events.push(MeterEvent::new(
                        context.clone(),
                        EventPayload::ToolCompleted(tool),
                    ));
                }
            }
            Some("result") => {
                let turns = u64_field_any(&parsed, &["num_turns"]).unwrap_or(1);
                self.metrics.turn_count = self.metrics.turn_count.saturating_add(turns);
                if let Some(active_ms) = u64_field_any(&parsed, &["duration_api_ms", "duration_ms"])
                {
                    self.metrics.active_time_ms =
                        self.metrics.active_time_ms.saturating_add(active_ms);
                }
                let is_error = bool_field(&parsed, "is_error").unwrap_or(false)
                    || string_field_any(&parsed, &["subtype"]).is_some_and(|subtype| {
                        subtype == "error" || subtype == "failure" || subtype == "failed"
                    });
                if is_error {
                    self.metrics.errors = self.metrics.errors.saturating_add(1);
                    self.failure_reason =
                        string_field_any(&parsed, &["error", "message", "result"])
                            .or_else(|| Some("Claude Code reported failure".to_owned()));
                }
            }
            Some("error") => {
                self.metrics.errors = self.metrics.errors.saturating_add(1);
                self.failure_reason = string_field_any(&parsed, &["error", "message"])
                    .or_else(|| Some("Claude Code reported failure".to_owned()));
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

        ClaudeNormalizeOutcome {
            parsed_json: true,
            events,
        }
    }
}

fn event_name(value: &Value) -> Option<String> {
    string_field_any(value, &["type", "event_type", "event"])
}

fn discover_session_id(value: &Value) -> Option<SessionId> {
    string_field_any_path(
        value,
        &[
            &["session_id"],
            &["conversation_id"],
            &["message", "session_id"],
            &["message", "conversation_id"],
        ],
    )
    .and_then(|value| SessionId::new(value).ok())
}

fn token_usage(value: &Value) -> Option<TokenUsage> {
    let usage = value
        .get("usage")
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("usage"))
        })
        .unwrap_or(value);
    let token_usage = TokenUsage {
        input_tokens: u64_field_any(usage, &["input_tokens", "prompt_tokens"]),
        cached_input_tokens: u64_field_any(
            usage,
            &[
                "cache_read_input_tokens",
                "cached_input_tokens",
                "cache_read_tokens",
            ],
        ),
        cache_write_tokens: u64_field_any(
            usage,
            &["cache_creation_input_tokens", "cache_write_tokens"],
        ),
        output_tokens: u64_field_any(usage, &["output_tokens", "completion_tokens"]),
        reasoning_tokens: u64_field_any(usage, &["reasoning_tokens", "reasoning_output_tokens"]),
    };
    if token_usage == TokenUsage::default() {
        None
    } else {
        Some(token_usage)
    }
}

fn assistant_tool_uses(value: &Value) -> Vec<ToolStarted> {
    message_content(value)
        .into_iter()
        .filter(|block| string_field_any(block, &["type"]).as_deref() == Some("tool_use"))
        .map(|block| ToolStarted {
            tool_id: string_field_any(block, &["id"]),
            tool_name: string_field_any(block, &["name"]),
        })
        .collect()
}

fn user_tool_results(value: &Value) -> Vec<ToolCompleted> {
    message_content(value)
        .into_iter()
        .filter(|block| string_field_any(block, &["type"]).as_deref() == Some("tool_result"))
        .map(|block| ToolCompleted {
            tool_id: string_field_any(block, &["tool_use_id", "id"]),
            tool_name: None,
            success: !bool_field(block, "is_error").unwrap_or(false),
            duration_ms: None,
        })
        .collect()
}

fn message_content(value: &Value) -> Vec<&Value> {
    value
        .get("message")
        .and_then(|message| message.get("content"))
        .or_else(|| value.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect()
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

fn string_field_any_path(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut cursor = value;
        for key in *path {
            cursor = cursor.get(*key)?;
        }
        cursor
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
    })
}

fn u64_field_any(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

#[cfg(test)]
mod tests {
    use meter_core::{EventType, RunId, TicketId};

    use super::*;

    fn context() -> EventContext {
        EventContext {
            run_id: RunId::new(),
            ticket_id: TicketId::new("ENG-CLAUDE").unwrap_or_else(|err| panic!("{err}")),
            label: Some("Claude harness".to_owned()),
            harness: HarnessKind::Claude,
            requested_model: None,
            resolved_model: None,
            session_id: None,
            workspace: Some(PathBuf::from(".")),
        }
    }

    #[test]
    fn builds_new_print_stream_command_with_practical_options() {
        let model = meter_core::ModelName::new("sonnet").unwrap_or_else(|err| panic!("{err}"));
        let options = ClaudeRunOptions {
            permission_mode: Some("plan".to_owned()),
            allowed_tools: vec!["Read".to_owned(), "Bash(git diff *)".to_owned()],
            disallowed_tools: vec!["Bash(rm *)".to_owned()],
            add_dirs: vec![PathBuf::from("../lib")],
            mcp_configs: vec!["./mcp.json".to_owned()],
            strict_mcp_config: true,
            settings: Some("./settings.json".to_owned()),
            setting_sources: Some("user,project".to_owned()),
            append_system_prompts: vec!["Follow repo standards".to_owned()],
            max_turns: Some(3),
            max_budget_usd: Some("5.00".to_owned()),
            ..ClaudeRunOptions::default()
        };

        let args = claude_argv(Some(&model), None, &options, "Fix tests")
            .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(
            args,
            vec![
                OsString::from("-p"),
                OsString::from("Fix tests"),
                OsString::from("--output-format"),
                OsString::from("stream-json"),
                OsString::from("--verbose"),
                OsString::from("--model"),
                OsString::from("sonnet"),
                OsString::from("--permission-mode"),
                OsString::from("plan"),
                OsString::from("--allowed-tools"),
                OsString::from("Read"),
                OsString::from("--allowed-tools"),
                OsString::from("Bash(git diff *)"),
                OsString::from("--disallowed-tools"),
                OsString::from("Bash(rm *)"),
                OsString::from("--add-dir"),
                OsString::from("../lib"),
                OsString::from("--mcp-config"),
                OsString::from("./mcp.json"),
                OsString::from("--strict-mcp-config"),
                OsString::from("--settings"),
                OsString::from("./settings.json"),
                OsString::from("--setting-sources"),
                OsString::from("user,project"),
                OsString::from("--append-system-prompt"),
                OsString::from("Follow repo standards"),
                OsString::from("--max-turns"),
                OsString::from("3"),
                OsString::from("--max-budget-usd"),
                OsString::from("5.00"),
            ]
        );
    }

    #[test]
    fn builds_resume_continue_and_session_commands() {
        let resume = SessionId::new("sess-123").unwrap_or_else(|err| panic!("{err}"));
        let resume_args = claude_argv(
            None,
            Some(&resume),
            &ClaudeRunOptions {
                fork_session: true,
                ..ClaudeRunOptions::default()
            },
            "Continue work",
        )
        .unwrap_or_else(|err| panic!("{err}"));
        let continue_args = claude_argv(
            None,
            None,
            &ClaudeRunOptions {
                continue_latest: true,
                ..ClaudeRunOptions::default()
            },
            "Run tests",
        )
        .unwrap_or_else(|err| panic!("{err}"));
        let session_id_args = claude_argv(
            None,
            None,
            &ClaudeRunOptions {
                session_id: Some("550e8400-e29b-41d4-a716-446655440000".to_owned()),
                ..ClaudeRunOptions::default()
            },
            "Start named session",
        )
        .unwrap_or_else(|err| panic!("{err}"));

        assert!(resume_args.contains(&OsString::from("--resume")));
        assert!(resume_args.contains(&OsString::from("sess-123")));
        assert!(resume_args.contains(&OsString::from("--fork-session")));
        assert!(continue_args.contains(&OsString::from("--continue")));
        assert!(session_id_args.contains(&OsString::from("--session-id")));
    }

    #[test]
    fn explicit_claude_session_id_is_available_for_telemetry_association() {
        let options = ClaudeRunOptions {
            session_id: Some("550e8400-e29b-41d4-a716-446655440000".to_owned()),
            ..ClaudeRunOptions::default()
        };
        let session_id = explicit_session_id(&options)
            .unwrap_or_else(|err| panic!("{err}"))
            .unwrap_or_else(|| panic!("expected explicit session id"));

        assert_eq!(session_id.as_str(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn rejects_invalid_combinations_before_launch() {
        let resume = SessionId::new("sess-123").unwrap_or_else(|err| panic!("{err}"));
        let result = claude_argv(
            None,
            Some(&resume),
            &ClaudeRunOptions {
                continue_latest: true,
                ..ClaudeRunOptions::default()
            },
            "Do work",
        );
        assert!(matches!(result, Err(HarnessError::UnsupportedConfig(_))));

        let result = claude_argv(
            None,
            None,
            &ClaudeRunOptions {
                system_prompt: Some("A".to_owned()),
                system_prompt_file: Some(PathBuf::from("prompt.txt")),
                ..ClaudeRunOptions::default()
            },
            "Do work",
        );
        assert!(matches!(result, Err(HarnessError::UnsupportedConfig(_))));

        let result = claude_argv(
            None,
            None,
            &ClaudeRunOptions {
                session_id: Some("not-a-uuid".to_owned()),
                ..ClaudeRunOptions::default()
            },
            "Do work",
        );
        assert!(matches!(result, Err(HarnessError::UnsupportedConfig(_))));
    }

    #[test]
    fn fixture_success_covers_session_usage_tool_completion_and_result() {
        let fixture = include_str!("../../../../tests/fixtures/claude/successful_run.jsonl");
        let mut normalizer = ClaudeEventNormalizer::new(context(), RawEventRetention::Disabled);
        let mut event_types = Vec::new();

        for line in fixture.lines() {
            let outcome = normalizer.normalize_line(line);
            event_types.extend(outcome.events.into_iter().map(|event| event.event_type));
        }

        assert!(event_types.contains(&EventType::SessionDiscovered));
        assert!(event_types.contains(&EventType::UsageReported));
        assert!(event_types.contains(&EventType::ToolStarted));
        assert!(event_types.contains(&EventType::ToolCompleted));
        assert_eq!(
            normalizer.session_id.as_ref().map(SessionId::as_str),
            Some("sess-claude-1")
        );
        assert_eq!(
            normalizer
                .resolved_model
                .as_ref()
                .map(meter_core::ModelName::as_str),
            Some("claude-sonnet-5")
        );
        assert_eq!(normalizer.metrics.tool_calls, 1);
        assert_eq!(normalizer.metrics.turn_count, 2);
        assert_eq!(normalizer.metrics.token_usage.input_tokens, Some(100));
        assert_eq!(normalizer.metrics.token_usage.output_tokens, Some(25));
    }

    #[test]
    fn fixture_failure_and_unknown_paths_are_durable_when_requested() {
        let failure = include_str!("../../../../tests/fixtures/claude/failed_run.jsonl");
        let unknown = include_str!("../../../../tests/fixtures/claude/unknown_event.jsonl");
        let mut failed = ClaudeEventNormalizer::new(context(), RawEventRetention::Disabled);
        let mut retained = ClaudeEventNormalizer::new(context(), RawEventRetention::Full);

        for line in failure.lines() {
            let _ = failed.normalize_line(line);
        }
        let unknown_events = retained.normalize_line(unknown.trim()).events;

        assert!(failed.failure_reason.is_some());
        assert_eq!(failed.metrics.errors, 1);
        assert_eq!(unknown_events[0].event_type, EventType::HarnessEvent);
    }
}
