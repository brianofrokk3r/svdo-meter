# Repository Alignment Standard

Evaluate whether the current repository state and completed changes follow the agent-facing instructions for this repository.

The judge should consider the work aligned when:

- AGENTS.md was treated as the root instruction file.
- Any standards referenced by AGENTS.md or .svdo/standards.md were followed.
- Generated standards under .svdo/standards/ were not manually edited.
- The change stayed scoped to the requested task.
- Existing CLI behavior and output contracts were preserved unless the task explicitly required changing them.
- User-facing CLI behavior has fixture-backed or integration test coverage.
- Rust changes follow the repository's error handling, parsing, dependency, and test conventions.
- Documentation was updated when behavior changed.
- Unrelated refactors, formatting churn, or metadata noise were avoided.

Return a failing score if required instructions were ignored, tests were skipped for behavior changes, or the implementation changed orchestration/agent behavior without explicit need.