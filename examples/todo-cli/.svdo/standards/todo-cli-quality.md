# Todo CLI Quality Standard

Judge whether the implemented `todo.py` is suitable for the stopword prompt impact study fixture.

Award a high score when:

- The CLI implements `add`, `list`, `complete`, and `delete` exactly as described in `TASK.md`.
- Todo state persists in `.todo.json` between separate command invocations.
- IDs start at `1` and stay stable for existing todos after deletion.
- Error paths are clear, use stderr, and exit non-zero.
- The implementation uses only the Python standard library.
- The code is simple enough for repeated agent runs to finish quickly.

Lower the score for:

- Behavior that only works inside one process.
- Output formats that are hard for deterministic checks to parse.
- Hidden dependencies, network calls, or unnecessary project scaffolding.
- Overly broad changes outside the fixture task.
