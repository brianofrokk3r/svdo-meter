# TypeSafe Judge Example

This example shows the intended split between judge backend, standard, and rubric.

```text
.svdo/evals/api-contract.yaml
.svdo/standards/api-architecture.md
.svdo/rubrics/architecture-alignment.yaml
```

Run from a workspace containing those files:

```bash
export TYPESAFE_API_KEY=...
svdo-meter eval run api-contract --workspace .
```

The eval selects TypeSafe as the judge backend. The standard is the rulebook, and the rubric is the ordered grading scale TypeSafe uses for the Score question.
