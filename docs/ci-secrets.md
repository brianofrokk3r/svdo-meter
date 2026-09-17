# CI Secrets and Harness Credentials

SVDO Meter measures local harness CLI runs by default. For `svdo-meter run`, the selected harness process reads credentials from the environment it inherits from your shell or CI runner. TypeSafe-backed eval judging is the direct provider API path: `svdo-meter eval run --judge-backend typesafe` reads the TypeSafe API key from `TYPESAFE_API_KEY` by default, or from the variable named by `--typesafe-api-key-env`.

Measured runs invoke harness commands shaped like:

```bash
codex exec --json ...
claude -p --output-format stream-json ...
opencode run --format json ...
```

Configure and validate the harness CLI first, then run `svdo-meter run` from the same environment. For runnable task flows, see [SVDO Meter Examples](examples.md) and the example projects under [`examples/`](../examples/).

## Credential Variables

| Harness | Environment variables |
|---|---|
| Codex CLI | Set `OPENAI_API_KEY` for API-key based OpenAI/Codex usage. Set `OPENAI_PROJECT_ID` when the harness should target a specific OpenAI project. |
| Claude Code CLI | Set `ANTHROPIC_API_KEY` when using API-key based Claude authentication. |
| OpenCode CLI | Set the variables required by the configured OpenCode provider and model. For OpenAI-backed models, this commonly includes `OPENAI_API_KEY` and optionally `OPENAI_PROJECT_ID`. For Anthropic-backed models, this commonly includes `ANTHROPIC_API_KEY`. |
| TypeSafe eval judge | Set `TYPESAFE_API_KEY`, or pass `--typesafe-api-key-env <ENV>` to use a differently named secured variable. |

OpenAI's official documentation lists `OPENAI_API_KEY` as the API key environment variable for CLI use and lists `OPENAI_PROJECT_ID` as an optional CLI environment variable. The OpenAI API reference also documents API credentials as bearer credentials. Keep this as upstream context for OpenAI-backed harnesses; SVDO Meter still delegates actual authentication to the harness process.

Never commit API keys, tokens, `.env` files, shell history, generated credential files, or CI secret values to the repository. Use your shell, CI secret store, or a dedicated secret manager.

## Bitbucket Pipelines

Store provider credentials as secured Bitbucket repository or workspace variables before enabling a measured run:

- `OPENAI_API_KEY`: required for OpenAI/Codex-backed tooling.
- `OPENAI_PROJECT_ID`: optional when using OpenAI projects.
- `TYPESAFE_API_KEY`: required when CI runs `svdo-meter eval run --judge-backend typesafe`.
- `ANTHROPIC_API_KEY`: required when the selected harness or OpenCode provider uses Claude.

Bitbucket exposes repository and workspace variables to the build container as environment variables. When those variables are secured, Bitbucket hides matching values in logs. Use workspace variables for credentials shared across repositories and repository variables for repository-specific credentials.

Add a `bitbucket-pipelines.yml` custom pipeline like this, replacing the harness install placeholder with the install command for `codex`, `claude`, or `opencode`:

```yaml
image: node:22

pipelines:
  custom:
    measured-ai-run:
      - variables:
          - name: SVDO_WORK_ID
            default: ENG-142
          - name: SVDO_PROMPT
            default: Implement the requested change and run the relevant checks.
          - name: SVDO_HARNESS
            default: codex
          - name: SVDO_MODEL
            default: gpt-5
      - step:
          name: Measured AI run
          script:
            - apt-get update && apt-get install -y curl ca-certificates
            - curl -fsSL https://raw.githubusercontent.com/brianofrokk3r/svdo-meter/main/install.sh | bash
            - export PATH="$HOME/.local/bin:$PATH"
            - '# Install the selected harness CLI here, for example codex, claude, or opencode.'
            - svdo-meter run --ticket "$SVDO_WORK_ID" --harness "$SVDO_HARNESS" --model "$SVDO_MODEL" --workspace "$BITBUCKET_CLONE_DIR" "$SVDO_PROMPT"
            - svdo-meter eval run ci --workspace "$BITBUCKET_CLONE_DIR"
            - svdo-meter report "$SVDO_WORK_ID" --workspace "$BITBUCKET_CLONE_DIR"
          artifacts:
            - .svdo/meter/**
            - .svdo/evals/**
```

The example keeps secrets out of `bitbucket-pipelines.yml`. The selected harness CLI reads the secured variables from the process environment, while SVDO Meter records local telemetry under `.svdo/meter/` and eval output under `.svdo/evals/`.

Bitbucket artifacts must be produced under `BITBUCKET_CLONE_DIR` by the end of the step. The example uses `--workspace "$BITBUCKET_CLONE_DIR"` so `.svdo/meter/**` and `.svdo/evals/**` are available for artifact collection.

## GitHub Actions

Use GitHub Actions secrets for provider API keys and expose only the needed values to the job or step environment:

```yaml
env:
  OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
  OPENAI_PROJECT_ID: ${{ secrets.OPENAI_PROJECT_ID }}
  ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
```

GitHub workflow artifacts can preserve `.svdo/meter/**` and `.svdo/evals/**` after a run. A GitHub Actions workflow would normally use `actions/upload-artifact` for those directories. Keep the harness install and `svdo-meter run` commands consistent with the Bitbucket example above.

## References

- OpenAI CLI environment variables: <https://developers.openai.com/api/reference/cli>
- OpenAI API authentication: <https://developers.openai.com/api/reference/overview#authentication>
- Bitbucket variables and secured variables: <https://support.atlassian.com/bitbucket-cloud/docs/variables-and-secrets/>
- Bitbucket pipeline artifacts: <https://support.atlassian.com/bitbucket-cloud/docs/use-artifacts-in-steps/>
- GitHub Actions secrets: <https://docs.github.com/en/actions/concepts/security/secrets>
- GitHub Actions workflow artifacts: <https://docs.github.com/en/actions/concepts/workflows-and-actions/workflow-artifacts>
