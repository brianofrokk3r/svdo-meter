# TypeSafe Judge Calculator Example

This example shows TypeSafe scoring against a concrete generated artifact: a
small Python calculator CLI.

```text
calc.py
test_calc.py
.svdo/evals/calculator-quality.yaml
.svdo/standards/calculator-quality.md
.svdo/rubrics/calculator-implementation.yaml
```

Run the local tests:

```bash
python -m unittest test_calc.py
```

Run the eval from the repository root:

```bash
export TYPESAFE_API_KEY=...
cargo run -p svdo-meter --bin svdo-meter -- \
  eval run calculator-quality \
  --workspace examples/typesafe-judge
```

The eval uses a command check for calculator behavior and a TypeSafe-backed
judge check for implementation quality. The TypeSafe request includes the task,
standard, ordered Score criteria, git snapshot data when available, and a
bounded snapshot of source files such as `calc.py` and `test_calc.py`.
