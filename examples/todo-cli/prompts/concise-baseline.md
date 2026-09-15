Build a Python CLI named `todo.py`.

The CLI must store todos in `.todo.json` in the current working directory so state survives across separate commands.

Usage:

```text
python todo.py add "Buy milk"
python todo.py list
python todo.py complete 1
python todo.py delete 1
```

Required behavior:

- `add <text>` appends an incomplete todo and prints `Added <id>: <text>`.
- `list` prints one todo per line as `<id>. [ ] <text>` for incomplete items and `<id>. [x] <text>` for completed items.
- `complete <id>` marks an existing todo complete and prints `Completed <id>: <text>`.
- `delete <id>` removes an existing todo and prints `Deleted <id>: <text>`.
- Todo ids start at `1` and remain stable for existing items after deletes.
- Unknown commands, missing arguments, invalid ids, or ids that do not exist must print an error message to stderr and exit non-zero.
- Use only the Python standard library.
