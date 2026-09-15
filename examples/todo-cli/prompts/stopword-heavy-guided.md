Please build a Python command-line program named `todo.py` for this small todo-list task.

The program should keep its todo data in a file named `.todo.json` in the current working directory, and it should do that so the same todo state is still available when the user runs one command and then later runs another command in a separate process.

The user should be able to run the program in these ways:

```text
python todo.py add "Buy milk"
python todo.py list
python todo.py complete 1
python todo.py delete 1
```

The required behavior is the following:

- When the user runs `add <text>`, the program should add a new incomplete todo item and should print exactly `Added <id>: <text>`.
- When the user runs `list`, the program should print one line for each todo. For a todo that is not complete, the line should be `<id>. [ ] <text>`. For a todo that is complete, the line should be `<id>. [x] <text>`.
- When the user runs `complete <id>`, the program should mark the existing todo with that id as complete and should print exactly `Completed <id>: <text>`.
- When the user runs `delete <id>`, the program should remove the existing todo with that id and should print exactly `Deleted <id>: <text>`.
- Todo ids should begin at `1`, and ids for existing todos should remain the same after other todos are deleted.
- If the user gives an unknown command, leaves out a required argument, gives an invalid id, or gives an id that does not exist, the program should print an error message to stderr and exit with a non-zero status code.
- Please use only the Python standard library.

