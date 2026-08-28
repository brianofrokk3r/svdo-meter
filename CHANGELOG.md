# Changelog

## 2026-08-28

- Added a top-level GitHub installer script for installing prebuilt `svdo-meter` release binaries without cloning the repository.
- Added installer platform mapping, SHA-256 checksum verification, `$HOME/.local/bin` default installation, `SVDO_METER_INSTALL_DIR` override support, and installed-binary verification with `svdo-meter --help`.
- Added a main-branch GitHub Release workflow that builds Linux x86_64, macOS x86_64, and macOS arm64 archives with predictable asset names and checksum files.
- Added lightweight shell validation for installer syntax and platform mapping to Rust CI.
- Documented the raw GitHub install command, default install location, and supported installer platforms.
- Added first-class Claude Code harness support for `svdo-meter run --harness claude`.
- Added Claude Code command construction for non-interactive print mode with `--output-format stream-json` and `--verbose`.
- Added mapped Claude Code options for model, permission mode, tool allow/deny rules, additional directories, MCP configuration, settings, system prompt customization, turn limits, and budget limits.
- Added safe Claude Code continue/resume/session handling with validation for incompatible option combinations.
- Added Claude Code stream JSON normalization for session discovery, model discovery, token usage, tool events, completion status, errors, and retained unknown events.
- Registered the Claude adapter in CLI wiring while preserving Codex behavior and leaving unsupported Gemini wiring explicit.
- Added Claude fixture-based adapter tests and CLI/config wiring tests that do not require live Claude Code execution.
- Updated README and CLI/build documentation with Claude Code prerequisites, examples, supported flags, validation rules, and known limitations.
- Updated Rust CI Clippy to run with `--all-features` while preserving workspace, all-targets, locked dependency, and denied-warning checks.

## 2026-08-24

- Added Codex-specific `svdo-meter run` options: `--codex-profile`, `--codex-sandbox`, `--codex-approve-for-me`, `--codex-yolo`, and repeatable `--codex-config`.
- Added typed Codex runtime configuration for profile, sandbox, approval mode, dangerous bypass mode, and config overrides.
- Updated Codex argv construction to pass profile, sandbox, approval mode, dangerous bypass, repeated config overrides, workspace, model, session resume, and prompt using explicit process arguments.
- Added validation that rejects Codex-only flags for non-Codex harnesses and rejects malformed `--codex-config` overrides before spawning a harness.
- Updated CLI help, docs, unit tests, and integration tests for Codex-specific options while preserving provider-neutral run behavior and telemetry.

## 2026-08-23

- Added explicit versions to internal workspace path dependencies so `cargo deny check` no longer treats them as wildcard dependencies.
- Added live event sink selection for `svdo-meter run` with `--sink jsonl`, `--sink stdout`, and `--emit ndjson`.
- Added stdout NDJSON event streaming while preserving default durable per-run JSONL telemetry under `.svdo/meter/`.
- Added sink fan-out and failure behavior tests for normalized `MeterEvent` output.
- Added per-run telemetry JSONL streams under `.svdo/meter/` so concurrent `svdo-meter run` processes do not append to the same file.
- Updated telemetry replay, reports, and inspection commands to aggregate all `.svdo/meter/*.jsonl` streams.
- Updated CLI help and empty telemetry messages for the directory-backed telemetry store.
- Added fixture-backed storage, wiring, and CLI integration tests covering per-run stream files.
- Refactored meter engine run requests to pass provider-neutral model, raw event retention, and harness options to adapters.
- Removed provider-specific `HarnessConfig` model and raw-retention matching from `RunEngine`.
- Updated Codex adapter invocation to consume neutral request fields while preserving Codex model, workspace, session resume, and telemetry behavior.
- Updated CLI config mapping and wiring so Claude and Gemini no longer fall through to Codex adapter registration.
- Added tests covering neutral request forwarding, config mapping, Codex fixture behavior, and missing-adapter wiring for unsupported harnesses.
- Added `svdo-meter report --pricing-file` for local JSON model pricing configuration keyed by exact model identifier.
- Added per-million-token cost estimation for input, cached input, and output tokens when telemetry includes a configured model.
- Added unavailable cost reporting for telemetry models without configured pricing instead of applying default rates.
- Updated terminal, JSON, and CSV report output plus CLI documentation for per-1,000,000-token pricing units.
- Added fixture-style reducer and CLI tests covering complete category pricing, multiple model rates, and unconfigured model pricing.

## 2026-08-22

- Added grouped telemetry inspection commands under `svdo-meter telemetry`.
- Added `sessions`, `runs`, and `inspect <id>` views for local telemetry debugging.
- Added tolerant JSONL inspection that reports malformed line diagnostics without dropping valid records.
- Added missing token field callouts for token-bearing telemetry records.
- Added fixture-based telemetry inspection tests for valid records, malformed lines, missing token fields, and missing or empty telemetry files.
