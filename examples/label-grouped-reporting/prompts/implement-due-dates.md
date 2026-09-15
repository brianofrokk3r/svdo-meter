# Implement due-date support for the todo CLI

Implement due-date support for the existing todo CLI fixture.

Use `DUE_DATE_PLAN.md` if it exists. The expected behavior is:

- `add` accepts an optional due date value, such as `--due 2026-10-15`.
- due dates are stored with todo items in `.todo.json`.
- `list` displays due dates next to matching items.
- invalid date input fails clearly and does not corrupt existing todo data.
- existing `add`, `list`, `complete`, and `delete` behavior keeps working.

Add or update lightweight tests or checks when the fixture structure supports them.
