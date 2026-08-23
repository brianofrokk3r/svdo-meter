# Feature Specification: Add PR CI Workflow for Rust Quality Checks

## User Story

As a contributor, I want Rust quality checks to run automatically on pull requests so I can see formatting, linting, test, and dependency policy failures before merge.

## Functional Requirements

- The repository must define a GitHub Actions workflow under `.github/workflows/`.
- The workflow must run on `pull_request` events.
- The workflow must run `cargo fmt` in check mode.
- The workflow must run `cargo clippy` and fail on lint warnings.
- The workflow must run `cargo test`.
- The workflow must install or configure `cargo-deny`.
- The workflow must run `cargo deny check`.
- The workflow name and job/step names must clearly communicate that the workflow validates Rust CI checks.

## Acceptance Criteria

- Opening or updating a pull request triggers the workflow.
- Formatting failures cause CI to fail.
- Clippy warnings or errors cause CI to fail.
- Test failures cause CI to fail.
- Cargo-deny policy failures cause CI to fail.
- Contributors can identify the failing quality gate from the GitHub Actions UI.

## Constraints

- Use the repository Rust toolchain configuration, which currently specifies `stable`.
- Prefer locked dependency resolution where Cargo supports it.
- Keep the change scoped to workflow automation and requested Spec Kit artifacts.
