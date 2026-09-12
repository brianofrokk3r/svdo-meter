//! Fixture-based adapter conformance tests.
//!
//! Add new provider or version coverage by placing captured JSONL under
//! `tests/fixtures/<provider>/`, then adding a `ConformanceCase` below with:
//! provider, version label, fixture path, expected canonical event summaries,
//! final metrics, and final session/model state. These tests never launch a
//! provider CLI; they exercise only normalization into the canonical
//! `meter_core::MeterEvent` schema.

use meter_adapters::{ClaudeEventNormalizer, CodexEventNormalizer, OpenCodeEventNormalizer};
use meter_core::{
    EventContext, HarnessKind, MeterEvent, ModelName, RawEventRetention, RunId, RunMetrics,
    SessionId, TicketId, TokenUsage,
};
use serde_json::{Value, json};
use std::path::PathBuf;

#[test]
fn codex_fixtures_conform_to_canonical_events() {
    for case in codex_cases() {
        assert_case(case);
    }
}

#[test]
fn claude_fixtures_conform_to_canonical_events() {
    for case in claude_cases() {
        assert_case(case);
    }
}

#[test]
fn opencode_fixtures_conform_to_canonical_events() {
    for case in opencode_cases() {
        assert_case(case);
    }
}

#[derive(Debug)]
struct ConformanceCase {
    provider: &'static str,
    version: &'static str,
    fixture_path: &'static str,
    fixture: &'static str,
    harness: HarnessKind,
    expected_events: Vec<ExpectedEvent>,
    expected_metrics: RunMetrics,
    expected_session_id: Option<&'static str>,
    expected_resolved_model: Option<&'static str>,
    expected_failure_reason: Option<&'static str>,
}

#[derive(Debug, PartialEq)]
struct ExpectedEvent {
    event_type: &'static str,
    session_id: Option<&'static str>,
    resolved_model: Option<&'static str>,
    payload: Value,
}

#[derive(Debug, PartialEq)]
struct ActualEvent {
    event_type: String,
    session_id: Option<String>,
    resolved_model: Option<String>,
    payload: Value,
}

#[derive(Debug)]
struct NormalizedFixture {
    events: Vec<MeterEvent>,
    metrics: RunMetrics,
    session_id: Option<SessionId>,
    resolved_model: Option<ModelName>,
    failure_reason: Option<String>,
}

fn assert_case(case: ConformanceCase) {
    let actual = normalize_fixture(&case);
    let actual_events: Vec<_> = actual.events.iter().map(summarize_event).collect();
    let expected_events: Vec<_> = case
        .expected_events
        .iter()
        .map(ExpectedEvent::actual)
        .collect();
    let id = case_id(&case);

    assert_eq!(
        actual_events.len(),
        expected_events.len(),
        "{id}: canonical event count mismatch; actual={actual_events:#?}"
    );
    for (index, (actual, expected)) in actual_events.iter().zip(expected_events.iter()).enumerate()
    {
        assert_eq!(
            actual.event_type, expected.event_type,
            "{id}: event[{index}].event_type mismatch"
        );
        assert_eq!(
            actual.session_id, expected.session_id,
            "{id}: event[{index}].session_id mismatch"
        );
        assert_eq!(
            actual.resolved_model, expected.resolved_model,
            "{id}: event[{index}].resolved_model mismatch"
        );
        assert_eq!(
            actual.payload, expected.payload,
            "{id}: event[{index}].payload mismatch"
        );
    }
    assert_eq!(
        actual.metrics, case.expected_metrics,
        "{id}: final metrics mismatch"
    );
    assert_eq!(
        actual.session_id.as_ref().map(SessionId::as_str),
        case.expected_session_id,
        "{id}: final session_id mismatch"
    );
    assert_eq!(
        actual.resolved_model.as_ref().map(ModelName::as_str),
        case.expected_resolved_model,
        "{id}: final resolved_model mismatch"
    );
    assert_eq!(
        actual.failure_reason.as_deref(),
        case.expected_failure_reason,
        "{id}: failure_reason mismatch"
    );
}

fn normalize_fixture(case: &ConformanceCase) -> NormalizedFixture {
    match case.harness {
        HarnessKind::Codex => {
            let mut normalizer =
                CodexEventNormalizer::new(context(case.harness), RawEventRetention::Disabled);
            let mut events = Vec::new();
            for line in case.fixture.lines() {
                events.extend(normalizer.normalize_line(line).events);
            }
            NormalizedFixture {
                events,
                metrics: normalizer.metrics,
                session_id: normalizer.session_id,
                resolved_model: normalizer.resolved_model,
                failure_reason: normalizer.failure_reason,
            }
        }
        HarnessKind::Claude => {
            let mut normalizer =
                ClaudeEventNormalizer::new(context(case.harness), RawEventRetention::Disabled);
            let mut events = Vec::new();
            for line in case.fixture.lines() {
                events.extend(normalizer.normalize_line(line).events);
            }
            NormalizedFixture {
                events,
                metrics: normalizer.metrics,
                session_id: normalizer.session_id,
                resolved_model: normalizer.resolved_model,
                failure_reason: normalizer.failure_reason,
            }
        }
        HarnessKind::OpenCode => {
            let mut normalizer =
                OpenCodeEventNormalizer::new(context(case.harness), RawEventRetention::Disabled);
            let mut events = Vec::new();
            for line in case.fixture.lines() {
                events.extend(normalizer.normalize_line(line).events);
            }
            NormalizedFixture {
                events,
                metrics: normalizer.metrics,
                session_id: normalizer.session_id,
                resolved_model: normalizer.resolved_model,
                failure_reason: None,
            }
        }
        unsupported => panic!(
            "{}: no conformance normalizer configured for {unsupported:?}",
            case_id(case)
        ),
    }
}

fn summarize_event(event: &MeterEvent) -> ActualEvent {
    ActualEvent {
        event_type: serde_json::to_value(event.event_type)
            .unwrap_or_else(|error| panic!("event_type must serialize: {error}"))
            .as_str()
            .unwrap_or_else(|| panic!("event_type must serialize as string"))
            .to_owned(),
        session_id: event.session_id.as_ref().map(|id| id.as_str().to_owned()),
        resolved_model: event
            .resolved_model
            .as_ref()
            .map(|model| model.as_str().to_owned()),
        payload: serde_json::to_value(&event.payload)
            .unwrap_or_else(|error| panic!("payload must serialize: {error}")),
    }
}

impl ExpectedEvent {
    fn actual(&self) -> ActualEvent {
        ActualEvent {
            event_type: self.event_type.to_owned(),
            session_id: self.session_id.map(ToOwned::to_owned),
            resolved_model: self.resolved_model.map(ToOwned::to_owned),
            payload: self.payload.clone(),
        }
    }
}

fn context(harness: HarnessKind) -> EventContext {
    EventContext {
        run_id: RunId::new(),
        ticket_id: TicketId::new("ENG-CONFORMANCE").unwrap_or_else(|err| panic!("{err}")),
        label: Some("Adapter conformance".to_owned()),
        harness,
        requested_model: None,
        resolved_model: None,
        session_id: None,
        workspace: Some(PathBuf::from(".")),
    }
}

fn case_id(case: &ConformanceCase) -> String {
    format!("{} {} {}", case.provider, case.version, case.fixture_path)
}

fn event(
    event_type: &'static str,
    session_id: Option<&'static str>,
    resolved_model: Option<&'static str>,
    payload: Value,
) -> ExpectedEvent {
    ExpectedEvent {
        event_type,
        session_id,
        resolved_model,
        payload,
    }
}

fn payload(name: &'static str, data: Value) -> Value {
    json!({ "type": name, "data": data })
}

fn codex_cases() -> Vec<ConformanceCase> {
    vec![
        ConformanceCase {
            provider: "codex",
            version: "gpt-5/captured-current",
            fixture_path: "tests/fixtures/codex/successful_run.jsonl",
            fixture: include_str!("../../../tests/fixtures/codex/successful_run.jsonl"),
            harness: HarnessKind::Codex,
            expected_events: vec![
                event(
                    "session.discovered",
                    Some("019c8a42-f72-success"),
                    Some("gpt-5"),
                    payload("session_discovered", json!({ "source": "codex" })),
                ),
                event(
                    "command.started",
                    Some("019c8a42-f72-success"),
                    Some("gpt-5"),
                    payload(
                        "command_started",
                        json!({ "command_id": "cmd-1", "command_kind": "shell" }),
                    ),
                ),
                event(
                    "command.completed",
                    Some("019c8a42-f72-success"),
                    Some("gpt-5"),
                    payload(
                        "command_completed",
                        json!({
                            "command_id": "cmd-1",
                            "success": true,
                            "exit_code": 0,
                            "duration_ms": 1200
                        }),
                    ),
                ),
                event(
                    "files.changed",
                    Some("019c8a42-f72-success"),
                    Some("gpt-5"),
                    payload("files_changed", json!({ "count": 2 })),
                ),
                event(
                    "tool.started",
                    Some("019c8a42-f72-success"),
                    Some("gpt-5"),
                    payload(
                        "tool_started",
                        json!({ "tool_id": "tool-1", "tool_name": "apply_patch" }),
                    ),
                ),
                event(
                    "tool.completed",
                    Some("019c8a42-f72-success"),
                    Some("gpt-5"),
                    payload(
                        "tool_completed",
                        json!({
                            "tool_id": "tool-1",
                            "tool_name": "apply_patch",
                            "success": true,
                            "duration_ms": 300
                        }),
                    ),
                ),
                event(
                    "usage.reported",
                    Some("019c8a42-f72-success"),
                    Some("gpt-5"),
                    payload(
                        "usage_reported",
                        json!({
                            "input_tokens": 100,
                            "cached_input_tokens": 25,
                            "cache_write_tokens": 5,
                            "output_tokens": 80,
                            "reasoning_tokens": 30
                        }),
                    ),
                ),
            ],
            expected_metrics: RunMetrics {
                provider_event_count: 7,
                commands_executed: 1,
                command_time_ms: 1200,
                files_changed: 2,
                tool_calls: 1,
                tool_time_ms: 300,
                turn_count: 1,
                active_time_ms: 2400,
                token_usage: TokenUsage {
                    input_tokens: Some(100),
                    cached_input_tokens: Some(25),
                    cache_write_tokens: Some(5),
                    output_tokens: Some(80),
                    reasoning_tokens: Some(30),
                },
                ..RunMetrics::default()
            },
            expected_session_id: Some("019c8a42-f72-success"),
            expected_resolved_model: Some("gpt-5"),
            expected_failure_reason: None,
        },
        ConformanceCase {
            provider: "codex",
            version: "captured-current",
            fixture_path: "tests/fixtures/codex/failed_run.jsonl",
            fixture: include_str!("../../../tests/fixtures/codex/failed_run.jsonl"),
            harness: HarnessKind::Codex,
            expected_events: vec![
                event(
                    "session.discovered",
                    Some("019c8a42-f72-failed"),
                    None,
                    payload("session_discovered", json!({ "source": "codex" })),
                ),
                event(
                    "command.started",
                    Some("019c8a42-f72-failed"),
                    None,
                    payload(
                        "command_started",
                        json!({ "command_id": "cmd-fail", "command_kind": "shell" }),
                    ),
                ),
                event(
                    "command.completed",
                    Some("019c8a42-f72-failed"),
                    None,
                    payload(
                        "command_completed",
                        json!({
                            "command_id": "cmd-fail",
                            "success": false,
                            "exit_code": 101,
                            "duration_ms": 900
                        }),
                    ),
                ),
            ],
            expected_metrics: RunMetrics {
                provider_event_count: 4,
                commands_executed: 1,
                failed_commands: 1,
                command_time_ms: 900,
                errors: 1,
                ..RunMetrics::default()
            },
            expected_session_id: Some("019c8a42-f72-failed"),
            expected_resolved_model: None,
            expected_failure_reason: Some("tests failed"),
        },
        ConformanceCase {
            provider: "codex",
            version: "captured-current",
            fixture_path: "tests/fixtures/codex/resumed_session.jsonl",
            fixture: include_str!("../../../tests/fixtures/codex/resumed_session.jsonl"),
            harness: HarnessKind::Codex,
            expected_events: vec![
                event(
                    "session.discovered",
                    Some("019c8a42-f72-existing"),
                    None,
                    payload("session_discovered", json!({ "source": "codex" })),
                ),
                event(
                    "usage.reported",
                    Some("019c8a42-f72-existing"),
                    None,
                    payload(
                        "usage_reported",
                        json!({ "input_tokens": 8, "output_tokens": 2 }),
                    ),
                ),
            ],
            expected_metrics: RunMetrics {
                provider_event_count: 2,
                turn_count: 1,
                token_usage: TokenUsage {
                    input_tokens: Some(8),
                    output_tokens: Some(2),
                    ..TokenUsage::default()
                },
                ..RunMetrics::default()
            },
            expected_session_id: Some("019c8a42-f72-existing"),
            expected_resolved_model: None,
            expected_failure_reason: None,
        },
        ConformanceCase {
            provider: "codex",
            version: "captured-current",
            fixture_path: "tests/fixtures/codex/interrupted_run.jsonl",
            fixture: include_str!("../../../tests/fixtures/codex/interrupted_run.jsonl"),
            harness: HarnessKind::Codex,
            expected_events: vec![event(
                "session.discovered",
                Some("019c8a42-f72-partial"),
                None,
                payload("session_discovered", json!({ "source": "codex" })),
            )],
            expected_metrics: RunMetrics {
                provider_event_count: 1,
                errors: 1,
                ..RunMetrics::default()
            },
            expected_session_id: Some("019c8a42-f72-partial"),
            expected_resolved_model: None,
            expected_failure_reason: None,
        },
        ConformanceCase {
            provider: "codex",
            version: "captured-current",
            fixture_path: "tests/fixtures/codex/malformed_event.jsonl",
            fixture: include_str!("../../../tests/fixtures/codex/malformed_event.jsonl"),
            harness: HarnessKind::Codex,
            expected_events: vec![event(
                "session.discovered",
                Some("ok"),
                None,
                payload("session_discovered", json!({ "source": "codex" })),
            )],
            expected_metrics: RunMetrics {
                provider_event_count: 1,
                errors: 1,
                ..RunMetrics::default()
            },
            expected_session_id: Some("ok"),
            expected_resolved_model: None,
            expected_failure_reason: None,
        },
        ConformanceCase {
            provider: "codex",
            version: "captured-current",
            fixture_path: "tests/fixtures/codex/missing_token_usage.jsonl",
            fixture: include_str!("../../../tests/fixtures/codex/missing_token_usage.jsonl"),
            harness: HarnessKind::Codex,
            expected_events: Vec::new(),
            expected_metrics: RunMetrics {
                provider_event_count: 1,
                turn_count: 1,
                active_time_ms: 100,
                ..RunMetrics::default()
            },
            expected_session_id: None,
            expected_resolved_model: None,
            expected_failure_reason: None,
        },
        ConformanceCase {
            provider: "codex",
            version: "captured-current",
            fixture_path: "tests/fixtures/codex/unknown_event.jsonl",
            fixture: include_str!("../../../tests/fixtures/codex/unknown_event.jsonl"),
            harness: HarnessKind::Codex,
            expected_events: Vec::new(),
            expected_metrics: RunMetrics {
                provider_event_count: 1,
                ..RunMetrics::default()
            },
            expected_session_id: None,
            expected_resolved_model: None,
            expected_failure_reason: None,
        },
    ]
}

fn claude_cases() -> Vec<ConformanceCase> {
    vec![
        ConformanceCase {
            provider: "claude",
            version: "claude-sonnet-5/captured-current",
            fixture_path: "tests/fixtures/claude/successful_run.jsonl",
            fixture: include_str!("../../../tests/fixtures/claude/successful_run.jsonl"),
            harness: HarnessKind::Claude,
            expected_events: vec![
                event(
                    "session.discovered",
                    Some("sess-claude-1"),
                    Some("claude-sonnet-5"),
                    payload("session_discovered", json!({ "source": "claude" })),
                ),
                event(
                    "usage.reported",
                    Some("sess-claude-1"),
                    Some("claude-sonnet-5"),
                    payload(
                        "usage_reported",
                        json!({
                            "input_tokens": 100,
                            "cached_input_tokens": 20,
                            "cache_write_tokens": 5,
                            "output_tokens": 10
                        }),
                    ),
                ),
                event(
                    "tool.started",
                    Some("sess-claude-1"),
                    Some("claude-sonnet-5"),
                    payload(
                        "tool_started",
                        json!({ "tool_id": "toolu-1", "tool_name": "Bash" }),
                    ),
                ),
                event(
                    "tool.completed",
                    Some("sess-claude-1"),
                    Some("claude-sonnet-5"),
                    payload(
                        "tool_completed",
                        json!({ "tool_id": "toolu-1", "success": true }),
                    ),
                ),
                event(
                    "usage.reported",
                    Some("sess-claude-1"),
                    Some("claude-sonnet-5"),
                    payload("usage_reported", json!({ "output_tokens": 15 })),
                ),
            ],
            expected_metrics: RunMetrics {
                provider_event_count: 4,
                tool_calls: 1,
                turn_count: 2,
                active_time_ms: 1800,
                token_usage: TokenUsage {
                    input_tokens: Some(100),
                    cached_input_tokens: Some(20),
                    cache_write_tokens: Some(5),
                    output_tokens: Some(25),
                    reasoning_tokens: None,
                },
                ..RunMetrics::default()
            },
            expected_session_id: Some("sess-claude-1"),
            expected_resolved_model: Some("claude-sonnet-5"),
            expected_failure_reason: None,
        },
        ConformanceCase {
            provider: "claude",
            version: "claude-sonnet-5/captured-current",
            fixture_path: "tests/fixtures/claude/failed_run.jsonl",
            fixture: include_str!("../../../tests/fixtures/claude/failed_run.jsonl"),
            harness: HarnessKind::Claude,
            expected_events: vec![event(
                "session.discovered",
                Some("sess-claude-failed"),
                Some("claude-sonnet-5"),
                payload("session_discovered", json!({ "source": "claude" })),
            )],
            expected_metrics: RunMetrics {
                provider_event_count: 2,
                turn_count: 1,
                errors: 1,
                ..RunMetrics::default()
            },
            expected_session_id: Some("sess-claude-failed"),
            expected_resolved_model: Some("claude-sonnet-5"),
            expected_failure_reason: Some("maximum turns reached"),
        },
        ConformanceCase {
            provider: "claude",
            version: "captured-current",
            fixture_path: "tests/fixtures/claude/unknown_event.jsonl",
            fixture: include_str!("../../../tests/fixtures/claude/unknown_event.jsonl"),
            harness: HarnessKind::Claude,
            expected_events: Vec::new(),
            expected_metrics: RunMetrics {
                provider_event_count: 1,
                ..RunMetrics::default()
            },
            expected_session_id: None,
            expected_resolved_model: None,
            expected_failure_reason: None,
        },
    ]
}

fn opencode_cases() -> Vec<ConformanceCase> {
    vec![
        ConformanceCase {
            provider: "opencode",
            version: "captured-current",
            fixture_path: "tests/fixtures/opencode/successful_run.jsonl",
            fixture: include_str!("../../../tests/fixtures/opencode/successful_run.jsonl"),
            harness: HarnessKind::OpenCode,
            expected_events: vec![
                event(
                    "session.discovered",
                    Some("ses_494719016ffe85dkDMj0FPRbHK"),
                    Some("github-copilot/gpt-5"),
                    payload("session_discovered", json!({ "source": "opencode" })),
                ),
                event(
                    "tool.completed",
                    Some("ses_494719016ffe85dkDMj0FPRbHK"),
                    Some("github-copilot/gpt-5"),
                    payload(
                        "tool_completed",
                        json!({
                            "tool_id": "prt_tool",
                            "tool_name": "bash",
                            "success": true,
                            "duration_ms": 400
                        }),
                    ),
                ),
                event(
                    "usage.reported",
                    Some("ses_494719016ffe85dkDMj0FPRbHK"),
                    Some("github-copilot/gpt-5"),
                    payload(
                        "usage_reported",
                        json!({
                            "input_tokens": 100,
                            "cached_input_tokens": 20,
                            "cache_write_tokens": 2,
                            "output_tokens": 25,
                            "reasoning_tokens": 5
                        }),
                    ),
                ),
            ],
            expected_metrics: RunMetrics {
                provider_event_count: 3,
                tool_calls: 1,
                tool_time_ms: 400,
                turn_count: 1,
                active_time_ms: 662,
                token_usage: TokenUsage {
                    input_tokens: Some(100),
                    cached_input_tokens: Some(20),
                    cache_write_tokens: Some(2),
                    output_tokens: Some(25),
                    reasoning_tokens: Some(5),
                },
                ..RunMetrics::default()
            },
            expected_session_id: Some("ses_494719016ffe85dkDMj0FPRbHK"),
            expected_resolved_model: Some("github-copilot/gpt-5"),
            expected_failure_reason: None,
        },
        ConformanceCase {
            provider: "opencode",
            version: "captured-current",
            fixture_path: "tests/fixtures/opencode/unknown_event.jsonl",
            fixture: include_str!("../../../tests/fixtures/opencode/unknown_event.jsonl"),
            harness: HarnessKind::OpenCode,
            expected_events: vec![event(
                "session.discovered",
                Some("ses_future"),
                None,
                payload("session_discovered", json!({ "source": "opencode" })),
            )],
            expected_metrics: RunMetrics {
                provider_event_count: 1,
                ..RunMetrics::default()
            },
            expected_session_id: Some("ses_future"),
            expected_resolved_model: None,
            expected_failure_reason: None,
        },
        ConformanceCase {
            provider: "opencode",
            version: "captured-current",
            fixture_path: "tests/fixtures/opencode/malformed_event.jsonl",
            fixture: include_str!("../../../tests/fixtures/opencode/malformed_event.jsonl"),
            harness: HarnessKind::OpenCode,
            expected_events: vec![event(
                "session.discovered",
                Some("ses_ok"),
                None,
                payload("session_discovered", json!({ "source": "opencode" })),
            )],
            expected_metrics: RunMetrics {
                provider_event_count: 1,
                errors: 1,
                ..RunMetrics::default()
            },
            expected_session_id: Some("ses_ok"),
            expected_resolved_model: None,
            expected_failure_reason: None,
        },
    ]
}
