#!/bin/sh
set -eu

REPO_OWNER=${SVDO_METER_REPO_OWNER:-brianofrokk3r}
REPO_NAME=${SVDO_METER_REPO_NAME:-svdo-meter}
BINARY_NAME=svdo-meter
DEFAULT_INSTALL_DIR="${HOME:-}/.local/bin"
INSTALL_DIR=${SVDO_METER_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}
RELEASE_BASE_URL=${SVDO_METER_RELEASE_BASE_URL:-"https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/latest/download"}

info() {
    printf '%s\n' "svdo-meter installer: $*"
}

fail() {
    printf '%s\n' "svdo-meter installer: error: $*" >&2
    exit 1
}

command_exists() {
    command -v "$1" >/dev/null 2>&1
}

normalize_os() {
    case "$1" in
        Linux) printf '%s\n' "linux" ;;
        Darwin) printf '%s\n' "macos" ;;
        *) return 1 ;;
    esac
}

normalize_arch() {
    case "$1" in
        x86_64 | amd64) printf '%s\n' "x86_64" ;;
        arm64 | aarch64) printf '%s\n' "aarch64" ;;
        *) return 1 ;;
    esac
}

platform_asset_suffix() {
    os=$(normalize_os "$1") || return 1
    arch=$(normalize_arch "$2") || return 1

    case "${os}-${arch}" in
        linux-x86_64 | macos-x86_64 | macos-aarch64)
            printf '%s\n' "${os}-${arch}"
            ;;
        *)
            return 1
            ;;
    esac
}

download_file() {
    url=$1
    output=$2

    if command_exists curl; then
        curl -fL --retry 3 --connect-timeout 15 -o "$output" "$url"
    elif command_exists wget; then
        wget -O "$output" "$url"
    else
        fail "curl or wget is required to download release assets"
    fi
}

verify_checksum() {
    archive=$1
    checksum=$2
    archive_name=$(basename "$archive")
    checksum_name=$(basename "$checksum")
    archive_dir=$(dirname "$archive")

    if command_exists sha256sum; then
        (cd "$archive_dir" && sha256sum -c "$checksum_name") >/dev/null
    elif command_exists shasum; then
        (cd "$archive_dir" && shasum -a 256 -c "$checksum_name") >/dev/null
    else
        fail "sha256sum or shasum is required to verify ${archive_name}"
    fi
}

verify_installed_binary() {
    binary_path=$1

    if "$binary_path" --help >/dev/null 2>&1; then
        return 0
    fi

    return 1
}

main() {
    if [ -z "${SVDO_METER_INSTALL_DIR:-}" ] && [ -z "${HOME:-}" ]; then
        fail "HOME is not set; set SVDO_METER_INSTALL_DIR to a writable bin directory"
    fi
    [ -n "$INSTALL_DIR" ] || fail "install directory is empty"
    command_exists uname || fail "uname is required to detect this platform"
    command_exists tar || fail "tar is required to extract release archives"
    command_exists mktemp || fail "mktemp is required to create a temporary directory"

    uname_os=$(uname -s)
    uname_arch=$(uname -m)
    suffix=$(platform_asset_suffix "$uname_os" "$uname_arch") || {
        fail "unsupported platform ${uname_os}/${uname_arch}; supported platforms: Linux x86_64, macOS x86_64, macOS arm64"
    }

    asset="${BINARY_NAME}-${suffix}.tar.gz"
    checksum_asset="${asset}.sha256"
    archive_url="${RELEASE_BASE_URL}/${asset}"
    checksum_url="${RELEASE_BASE_URL}/${checksum_asset}"
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

    info "downloading ${asset}"
    download_file "$archive_url" "${tmp_dir}/${asset}" || fail "failed to download ${archive_url}"

    info "downloading ${checksum_asset}"
    download_file "$checksum_url" "${tmp_dir}/${checksum_asset}" || fail "failed to download ${checksum_url}"

    info "verifying checksum"
    verify_checksum "${tmp_dir}/${asset}" "${tmp_dir}/${checksum_asset}" || fail "checksum verification failed for ${asset}"

    mkdir -p "${tmp_dir}/extract"
    tar -xzf "${tmp_dir}/${asset}" -C "${tmp_dir}/extract" || fail "failed to extract ${asset}"
    [ -f "${tmp_dir}/extract/${BINARY_NAME}" ] || fail "archive did not contain ${BINARY_NAME}"

    mkdir -p "$INSTALL_DIR" || fail "failed to create install directory ${INSTALL_DIR}"
    cp "${tmp_dir}/extract/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}" || fail "failed to install ${BINARY_NAME} to ${INSTALL_DIR}"
    chmod 755 "${INSTALL_DIR}/${BINARY_NAME}" || fail "failed to mark ${INSTALL_DIR}/${BINARY_NAME} executable"

    info "verifying installed binary"
    verify_installed_binary "${INSTALL_DIR}/${BINARY_NAME}" || fail "${INSTALL_DIR}/${BINARY_NAME} --help failed"

    info "installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"
    case ":${PATH:-}:" in
        *":${INSTALL_DIR}:"*) ;;
        *) info "add ${INSTALL_DIR} to PATH to run ${BINARY_NAME} from any directory" ;;
    esac
}

if [ "${SVDO_METER_INSTALLER_TEST:-}" != "1" ]; then
    main "$@"
fi
