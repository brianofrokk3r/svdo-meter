Could you please implement a Python CLI application named `todo.py` for this repository workspace?

The application should be a small todo manager, and it should save and load its data by using a `.todo.json` file in the current working directory. This persistence is important because the user may run one command, then run another command afterward, and the todo state from the earlier command should still be there for the later command.

The interface that should be supported is shown here:

```text
python todo.py add "Buy milk"
python todo.py list
python todo.py complete 1
python todo.py delete 1
```

Please make sure that the command behavior matches these requirements:

- For the `add <text>` command, the program should append a new todo item that is incomplete, and it should print `Added <id>: <text>`.
- For the `list` command, the program should print the todos with one todo on each line. If an item is incomplete, it should be printed as `<id>. [ ] <text>`, and if an item is complete, it should be printed as `<id>. [x] <text>`.
- For the `complete <id>` command, the program should find an existing todo item with that id, mark it complete, and print `Completed <id>: <text>`.
- For the `delete <id>` command, the program should find an existing todo item with that id, remove it, and print `Deleted <id>: <text>`.
- The ids for todo items should start at `1`, and after an item is deleted, the ids of the remaining existing items should not be changed.
- If a command is unknown, if a required argument is missing, if an id is not valid, or if an id does not refer to an existing todo item, the program should write an error message to stderr and should exit with a non-zero status code.
- Please keep the implementation limited to the Python standard library, without adding third-party dependencies.

