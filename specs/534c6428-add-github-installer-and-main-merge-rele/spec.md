# Functional Specification: Add GitHub Installer and Main-Merge Release Binaries

## User Story

As a user who wants to run `svdo-meter` without cloning the repository or installing Rust, I can run a one-line shell command from GitHub that downloads the right prebuilt CLI binary, installs it into a user-writable bin directory, and verifies that the installed binary starts.

## Functional Requirements

- Provide a top-level executable `install.sh` suitable for `curl -fsSL .../install.sh | bash`.
- Detect the user's operating system and CPU architecture and map them to published binary asset names.
- Download release assets from a stable GitHub-hosted location.
- Install `svdo-meter` into `$SVDO_METER_INSTALL_DIR` when set, otherwise `$HOME/.local/bin`.
- Avoid requiring root by default.
- Verify checksums when checksum assets are available and fail when checksum verification cannot be completed.
- Verify the installed binary with `svdo-meter --help` when possible.
- Print clear success and failure messages.
- Fail fast for unsupported platforms, missing `HOME`, missing downloader, missing archive tool, missing release asset, checksum mismatch, or failed binary verification.
- Build and publish release binaries when changes are pushed to `main`.
- Use the `main` merge commit SHA as the GitHub Release tag/name identifier.
- Document install command, install location, and supported platforms in `README.md`.

## Supported Platforms

- Linux x86_64
- macOS x86_64
- macOS arm64/aarch64

Other platforms must fail with a clear unsupported-platform message until release assets are added.

## Non-Goals

- Installing dependencies for the Codex harness.
- Replacing source builds or `cargo install`.
- Requiring package managers such as Homebrew, apt, or yum.
- Supporting privileged global installation by default.

## Acceptance Criteria

- `install.sh` exists at repository root and is executable.
- The documented raw GitHub command installs `svdo-meter` without cloning the repository on supported platforms.
- `.github/workflows/` contains a workflow that publishes release binaries on push to `main`.
- Release assets use predictable names that match installer platform mapping.
- Default installation target is documented as `$HOME/.local/bin`.
- Installer verifies the installed binary with `--help`.
- Unsupported platforms and missing release assets produce clear errors.
- README includes install command, supported platforms, and install location.
- Rust CI remains intact and adds lightweight installer validation where practical.
