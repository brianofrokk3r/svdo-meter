# Changelog

## 2026-08-23

- Refactored meter engine run requests to pass provider-neutral model, raw event retention, and harness options to adapters.
- Removed provider-specific `HarnessConfig` model and raw-retention matching from `RunEngine`.
- Updated Codex adapter invocation to consume neutral request fields while preserving Codex model, workspace, session resume, and telemetry behavior.
- Updated CLI config mapping and wiring so Claude and Gemini no longer fall through to Codex adapter registration.
- Added tests covering neutral request forwarding, config mapping, Codex fixture behavior, and missing-adapter wiring for unsupported harnesses.

## 2026-08-22

- Added grouped telemetry inspection commands under `svdo-meter telemetry`.
- Added `sessions`, `runs`, and `inspect <id>` views for local `.svdo/meter.jsonl` debugging.
- Added tolerant JSONL inspection that reports malformed line diagnostics without dropping valid records.
- Added missing token field callouts for token-bearing telemetry records.
- Added fixture-based telemetry inspection tests for valid records, malformed lines, missing token fields, and missing or empty telemetry files.
