# Todo CLI Prompt Variants

These prompt files keep the todo CLI implementation target constant while varying wording style for stopword-impact studies.

Use any variant with `svdo-meter run --prompt-file` from an isolated copy of `examples/todo-cli/`:

```bash
svdo-meter run \
  --ticket STOPWORD-TODO \
  --label concise-baseline-001 \
  --harness codex \
  --model gpt-5.5 \
  --dangerous-bypass \
  --prompt-file prompts/concise-baseline.md
```

`variants.yaml` records report-friendly metadata for each file. Parent study runners should use the `id` value in run labels and keep repetitions balanced across variants.

