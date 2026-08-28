#!/bin/sh
set -eu

SVDO_METER_INSTALLER_TEST=1 . ./install.sh

assert_eq() {
    expected=$1
    actual=$2
    label=$3

    if [ "$expected" != "$actual" ]; then
        printf '%s\n' "FAIL: ${label}: expected ${expected}, got ${actual}" >&2
        exit 1
    fi
}

assert_unsupported() {
    os=$1
    arch=$2
    label=$3

    if platform_asset_suffix "$os" "$arch" >/dev/null 2>&1; then
        printf '%s\n' "FAIL: ${label}: expected unsupported platform" >&2
        exit 1
    fi
}

assert_eq "linux-x86_64" "$(platform_asset_suffix Linux x86_64)" "Linux x86_64"
assert_eq "linux-x86_64" "$(platform_asset_suffix Linux amd64)" "Linux amd64"
assert_eq "macos-x86_64" "$(platform_asset_suffix Darwin x86_64)" "macOS x86_64"
assert_eq "macos-aarch64" "$(platform_asset_suffix Darwin arm64)" "macOS arm64"
assert_eq "macos-aarch64" "$(platform_asset_suffix Darwin aarch64)" "macOS aarch64"

assert_unsupported Linux aarch64 "Linux aarch64"
assert_unsupported FreeBSD x86_64 "FreeBSD x86_64"
assert_unsupported Darwin riscv64 "macOS riscv64"

printf '%s\n' "install.sh platform mapping tests passed"
