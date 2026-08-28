# Changelog

## 2026-08-28

- Updated Rust CI Clippy to run with `--all-features` while preserving workspace, all-targets, locked dependency, and denied-warning checks.

## 2026-08-23

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
