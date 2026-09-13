# SVDO Meter Examples

This page keeps fuller runnable examples out of the README while preserving copy-pasteable workflows.

## Calculator Benchmark: Codex vs OpenCode

The calculator example exercises the full production flow with a small deterministic task:

```text
svdo-meter run -> svdo-meter eval run -> svdo-meter report -> svdo-meter compare
```

It compares Codex and OpenCode on the same prompt and eval:

- Codex model: `gpt-5.5`
- OpenCode model: `codex/gpt-5.5`
- Prompt: `examples/calculator/TASK.md`
- Eval: `examples/calculator/.svdo/evals/calc-implementation.yaml`

Install `svdo-meter`, make sure `svdo-meter`, `codex`, `opencode`, and `python3` are available on `PATH`, then run:

```bash
./examples/calculator/benchmark-gpt55-codex-vs-opencode.sh
```

The script creates an isolated temporary workspace, copies only the calculator eval definition into that workspace, runs each harness with `--prompt-file`, evaluates the generated `calc.py`, writes compare enrichment artifacts under `.svdo/evals/`, and prints both `svdo-meter report` and `svdo-meter compare` output. The temporary workspace is kept so you can inspect generated files and telemetry after the run:

```text
<temp-workspace>/
  calc.py
  .svdo/
    meter/
    evals/
```

You can override the work id or workspace with environment variables:

```bash
SVDO_BENCH_WORK=CALC-GPT55-MANUAL \
SVDO_BENCH_WORKSPACE=/tmp/svdo-calc-gpt55 \
./examples/calculator/benchmark-gpt55-codex-vs-opencode.sh
```

Additional knobs are available for model or agent experiments:

```bash
SVDO_BENCH_CODEX_MODEL=gpt-5.5 \
SVDO_BENCH_OPENCODE_MODEL=codex/gpt-5.5 \
SVDO_BENCH_OPENCODE_AGENT=build \
./examples/calculator/benchmark-gpt55-codex-vs-opencode.sh
```

The benchmark uses `--dangerous-bypass` because it runs in an isolated throwaway workspace. Use that posture only for workspaces where automatic edits and command execution are acceptable.
