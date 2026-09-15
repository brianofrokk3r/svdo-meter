Create `todo.py`: Python CLI, standard library only.

State file: `.todo.json`, current working directory. State persists across separate invocations.

Commands:

```text
python todo.py add "Buy milk"
python todo.py list
python todo.py complete 1
python todo.py delete 1
```

Behavior:

- `add <text>`: append incomplete todo. Print `Added <id>: <text>`.
- `list`: print each todo line. Incomplete format: `<id>. [ ] <text>`. Complete format: `<id>. [x] <text>`.
- `complete <id>`: mark existing todo complete. Print `Completed <id>: <text>`.
- `delete <id>`: remove existing todo. Print `Deleted <id>: <text>`.
- IDs start at `1`. Existing IDs stay unchanged after deletes.
- Unknown command, missing argument, invalid ID, missing ID: print error to stderr, exit non-zero.
- No third-party dependencies.

