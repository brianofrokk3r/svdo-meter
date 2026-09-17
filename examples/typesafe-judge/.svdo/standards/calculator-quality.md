# Calculator Quality Standard

- The CLI should support `add`, `subtract`, `multiply`, and `divide`.
- Numeric parsing should reject invalid operands with a non-zero exit status.
- Division by zero should fail with a clear error instead of crashing.
- Core arithmetic should be separated from argument parsing enough to be tested directly.
- Output should be minimal and predictable so scripts can consume it.
