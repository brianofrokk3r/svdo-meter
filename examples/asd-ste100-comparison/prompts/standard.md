# Implement an expanded todo CLI

Build a Python command-line application named `todo.py`. Complete all requirements below and add `test_todo.py` with automated tests for the required behavior.

## Requirements

- **R01 — Runtime and storage.** Use only the Python standard library. Store state in `.todo.json` in the current working directory so it persists across separate invocations. Do not read or write files outside the current working directory.
- **R02 — Commands.** Support `add`, `list`, `complete`, `delete`, and `report labels`. Accept options only where this prompt defines them.
- **R03 — Add.** `python todo.py add <text> [--due YYYY-MM-DD] [--label LABEL ...]` creates an incomplete todo and prints exactly `Added <id>: <text>`.
- **R04 — Identifiers.** Assign integer IDs starting at `1`. Never reuse an ID. Deleting a todo must not change any remaining ID.
- **R05 — Complete and delete.** `complete <id>` marks an existing todo complete and prints exactly `Completed <id>: <text>`. `delete <id>` removes an existing todo and prints exactly `Deleted <id>: <text>`.
- **R06 — Due dates.** `--due` is optional. A due date must be a real Gregorian calendar date in the exact `YYYY-MM-DD` format. Persist a valid due date.
- **R07 — Labels.** `--label` is optional and repeatable. Normalize labels to lowercase. A label must match `[a-z0-9][a-z0-9-]{0,19}` after normalization. Remove duplicate labels and persist labels in ascending lexical order.
- **R08 — List display.** `list` prints matching todos in ascending ID order. Begin each line with `<id>. [ ] <text>` for an incomplete todo or `<id>. [x] <text>` for a completed todo. Append ` | due: <date>` when the todo has a due date. Then append ` | labels: <label1>,<label2>` when the todo has labels. Do not append a segment for missing metadata. Print no output when no todo matches.
- **R09 — List filters.** `list` accepts `--status open|completed|all`, `--due YYYY-MM-DD`, `--due-before YYYY-MM-DD`, and `--label LABEL`. The default status is `all`. `--due-before` is inclusive. Normalize the label filter to lowercase. Apply all supplied filters with logical AND.
- **R10 — Grouped report.** `report labels` accepts the same filters as `list`. Apply the filters before grouping. Count a todo once in every label group that it has. Count a todo without labels in `(unlabeled)`. Print labeled groups in ascending lexical order, followed by `(unlabeled)` when that group exists. Use exactly `<label>: total=<n> open=<n> completed=<n>` for each line. Print no output when no todo matches.
- **R11 — Validation failures.** Reject unknown commands, unsupported options, missing values, invalid IDs, nonexistent IDs, invalid dates, invalid labels, and unexpected positional arguments. Write a useful error to stderr and exit with a nonzero status.
- **R12 — State integrity.** Validate a command completely before changing state. A failed command must not create or modify `.todo.json`. Report malformed stored JSON or an invalid stored schema as an error instead of discarding or replacing the data.
- **R13 — Existing state.** Treat missing `due` or `labels` fields in otherwise valid stored todo objects as no due date or no labels. This rule provides compatibility with the basic todo fixture.
- **R14 — Tests.** Implement standard-library automated tests in `test_todo.py`. The tests must cover the core commands, persistence, due dates, labels, combined filters, grouped reporting, validation errors, and state integrity. `python -m unittest -v` must run the tests.
- **R15 — Deliverables and scope.** Deliver `todo.py` and `test_todo.py`. Do not add third-party dependencies, network access, unrelated features, or unrelated files.

