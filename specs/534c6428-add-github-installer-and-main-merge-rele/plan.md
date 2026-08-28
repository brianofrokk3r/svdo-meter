# Implementation Plan: Add GitHub Installer and Main-Merge Release Binaries

## Context

The repository is a Rust workspace with the `svdo-meter` binary package under `crates/svdo-meter`. Existing CI runs formatting, clippy, tests, and dependency policy checks on pull requests and pushes. There is no existing release workflow or stronger release convention, so release identity will be derived from the `main` push commit SHA.

## Approach

1. Add a POSIX-compatible `install.sh` with small testable functions for platform mapping, dependency checks, download, checksum verification, extraction, install, and binary verification.
2. Publish release assets with stable names:
   - `svdo-meter-linux-x86_64.tar.gz`
   - `svdo-meter-macos-x86_64.tar.gz`
   - `svdo-meter-macos-aarch64.tar.gz`
3. Publish matching `.sha256` assets and make the installer require checksum verification.
4. Add a release workflow that runs on push to `main`, builds the supported targets, packages the binary archives, creates checksums, and publishes a GitHub Release tagged `main-${GITHUB_SHA}`.
5. Add lightweight shell tests for syntax and platform mapping.
6. Update README with the raw GitHub install command, supported platforms, and install location.

## Validation

- `sh -n install.sh`
- Platform mapping tests via `tests/install_sh_test.sh`
- Existing Rust checks remain in `.github/workflows/rust-ci.yml`

## Risks

- macOS cross-target builds rely on `rustup target add` for Apple targets on macOS runners.
- Linux arm64 is intentionally unsupported until a reliable build target is added.
- The installer uses GitHub latest-release URLs, so the release workflow must keep asset filenames stable.
