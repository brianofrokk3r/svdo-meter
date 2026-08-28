# SVDO Meter Constitution

## Principles

1. Installer paths must avoid privileged writes by default and make any required user action explicit.
2. Release artifacts must be deterministic enough for automation to locate without cloning the repository.
3. Installer failures must stop immediately with actionable messages for unsupported platforms, missing tools, missing artifacts, or failed verification.
4. CI changes must preserve the existing Rust quality gates and keep release-only work isolated from pull request checks.
5. Documentation must describe the exact install command, default install location, supported platforms, and verification behavior.

## Change Control

This constitution is created for the GitHub installer and main-merge release binary workflow. Future changes should keep user-facing install behavior stable or document the migration path.
