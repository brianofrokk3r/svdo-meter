# Expanded Todo CLI Quality Standard

Judge the generated implementation against `TASK.md`. Apply the same standard to both prompt variants.

Award a high score when:

- `todo.py` implements every requirement from R01 through R15 without changing the command contract.
- State persistence, stable IDs, dates, normalized labels, combined filters, and grouped label reporting are correct and deterministic.
- The implementation reads the supplied legacy root-array fixture without rewriting it during a read-only command and preserves IDs and future ID uniqueness if a successful mutation migrates the state.
- Invalid input and invalid stored data produce clear stderr errors, nonzero exit status, and no state corruption.
- `test_todo.py` meaningfully covers success and failure behavior and runs with `python -m unittest -v`.
- The implementation uses only the Python standard library and stays within the two requested deliverables. Supplied files under `.svdo/` are experiment inputs, not generated deliverables.
- The code has clear responsibilities, avoids unnecessary abstraction, and is maintainable for the size of the fixture.

Lower the score for:

- Missing, weakened, or reinterpreted requirements.
- Output that is nondeterministic or differs from the specified format.
- Validation that happens after writing state, silent repair of invalid data, or broad exception suppression.
- Third-party dependencies, network access, generated scaffolding, unrelated files, or unrelated features.
- Tests that are absent, cannot run, or merely duplicate implementation details without checking user behavior.

Do not award or remove points because of the prompt's writing style. Judge only the repository output and its alignment with the shared task.
