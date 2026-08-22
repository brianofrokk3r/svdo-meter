# Changelog

## 2026-08-22

- Added grouped telemetry inspection commands under `svdo-meter telemetry`.
- Added `sessions`, `runs`, and `inspect <id>` views for local `.svdo/meter.jsonl` debugging.
- Added tolerant JSONL inspection that reports malformed line diagnostics without dropping valid records.
- Added missing token field callouts for token-bearing telemetry records.
- Added fixture-based telemetry inspection tests for valid records, malformed lines, missing token fields, and missing or empty telemetry files.
