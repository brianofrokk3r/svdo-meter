# Label-Grouped Reporting Example

This example shows how SVDO Meter labels can separate workflow phases for the same feature. It uses a small todo CLI feature story: first ask an agent to plan due-date support, then ask it to implement the plan. Both runs share one work id and use labels to identify the phase:

- `plan`: inspect the existing todo CLI fixture and write a small implementation plan.
- `implement`: add due-date behavior and update checks after the plan is clear.

The included fixture telemetry lets you try the reporting commands without running an agent.

## Inspect The Included Telemetry

From the repository root, render the normal report for the whole feature:

```bash
svdo-meter report LABEL-TODO-DUEDATES --workspace examples/label-grouped-reporting
```

Then filter the same work item by each label:

```bash
svdo-meter report LABEL-TODO-DUEDATES --label plan --workspace examples/label-grouped-reporting
svdo-meter report LABEL-TODO-DUEDATES --label implement --workspace examples/label-grouped-reporting
```

For a side-by-side view of time by phase, use the helper script:

```bash
./examples/label-grouped-reporting/report-label-groups.sh
```

Example output from the fixture:

```text
SVDO Label Group Report - LABEL-TODO-DUEDATES
Workspace: examples/label-grouped-reporting

Label       Runs  Agent time  Input tokens  Output tokens
----------  ----  ----------  ------------  -------------
plan           1  5m 35s             14200           2100
implement      1  13m 50s            38600           7600
----------  ----  ----------  ------------  -------------
total          2  19m 25s            52800           9700

Insight: implement used 71.2% of measured agent time.
```

That view answers a common team question: did this feature spend most agent time on planning, implementation, or both? The labels are copied into canonical telemetry, so the same work id can be reported as one feature or filtered into phase-specific traces.

## Run The Workflow Yourself

To run the full workflow with one command, use:

```bash
./examples/label-grouped-reporting/run-plan-implement-report.sh
```

The script copies the todo CLI fixture into a fresh temporary workspace, runs the `plan` and `implement` phases with the same work id, then prints the overall report, each label-filtered report, and the label group summary.

Use environment variables or flags to adapt it:

```bash
SVDO_METER_BIN=./target/debug/svdo-meter \
./examples/label-grouped-reporting/run-plan-implement-report.sh \
  --workspace /tmp/svdo-label-grouped-reporting \
  --harness codex \
  --model gpt-5.5
```

Create a disposable workspace from the todo CLI fixture:

```bash
WORKSPACE=/tmp/svdo-label-grouped-reporting
rm -rf "$WORKSPACE"
mkdir -p "$WORKSPACE"
cp -R examples/todo-cli/. "$WORKSPACE/"
```

Run the planning phase. This prompt asks the agent to inspect the fixture and write `DUE_DATE_PLAN.md` without implementing code yet:

```bash
svdo-meter run \
  --ticket LABEL-TODO-DUEDATES \
  --label plan \
  --harness codex \
  --model gpt-5.5 \
  --workspace "$WORKSPACE" \
  --prompt-file examples/label-grouped-reporting/prompts/plan-due-dates.md
```

Run the implementation phase against the same work id with a different label:

```bash
svdo-meter run \
  --ticket LABEL-TODO-DUEDATES \
  --label implement \
  --harness codex \
  --model gpt-5.5 \
  --workspace "$WORKSPACE" \
  --prompt-file examples/label-grouped-reporting/prompts/implement-due-dates.md
```

Inspect the complete feature and each phase:

```bash
svdo-meter report LABEL-TODO-DUEDATES --workspace "$WORKSPACE"
svdo-meter report LABEL-TODO-DUEDATES --label plan --workspace "$WORKSPACE"
svdo-meter report LABEL-TODO-DUEDATES --label implement --workspace "$WORKSPACE"
./examples/label-grouped-reporting/report-label-groups.sh LABEL-TODO-DUEDATES --workspace "$WORKSPACE"
```

`svdo-meter report` does not need a special grouping flag here. The helper uses the existing `--label` filter and CSV output to compare the phase totals that SVDO Meter already records.
