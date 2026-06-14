#!/usr/bin/env bash
# install.sh — Download and install the latest poshgre binary to /usr/local/bin/posh
# Only macOS (Apple Silicon and Intel) is supported.
set -euo pipefail

REPO="hatohato25/poshgre"
BINARY_NAME="posh"
INSTALL_DIR="/usr/local/bin"

# ---- Platform detection ----
OS="$(uname -s)"
if [[ "$OS" != "Darwin" ]]; then
  echo "Error: poshgre currently supports macOS only. Detected OS: ${OS}" >&2
  exit 1
fi

ARCH="$(uname -m)"
case "$ARCH" in
  arm64)
    TARGET="aarch64-apple-darwin"
    ;;
  x86_64)
    TARGET="x86_64-apple-darwin"
    ;;
  *)
    echo "Error: Unsupported architecture: ${ARCH}" >&2
    exit 1
    ;;
esac

# ---- Fetch latest release version from GitHub API ----
echo "Fetching latest release version..."
VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' \
  | cut -d'"' -f4)"

if [[ -z "$VERSION" ]]; then
  echo "Error: Failed to determine the latest release version." >&2
  exit 1
fi

echo "Latest version: ${VERSION}"
echo "Target:         ${TARGET}"

# ---- Download tarball ----
TARBALL="posh-${VERSION}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${TARBALL}"
TMP_DIR="$(mktemp -d)"
# Ensure the temporary directory is removed on exit, regardless of success or failure
trap 'rm -rf "${TMP_DIR}"' EXIT

echo "Downloading ${DOWNLOAD_URL} ..."
curl -fsSL --output "${TMP_DIR}/${TARBALL}" "${DOWNLOAD_URL}"

# ---- Extract binary ----
tar -xzf "${TMP_DIR}/${TARBALL}" -C "${TMP_DIR}"

if [[ ! -f "${TMP_DIR}/${BINARY_NAME}" ]]; then
  echo "Error: Binary '${BINARY_NAME}' not found in the extracted archive." >&2
  exit 1
fi

chmod +x "${TMP_DIR}/${BINARY_NAME}"

# ---- Install to /usr/local/bin ----
# Use sudo only when the install directory is not writable by the current user.
if [[ -w "$INSTALL_DIR" ]]; then
  mv "${TMP_DIR}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
else
  echo "Installing to ${INSTALL_DIR} requires sudo..."
  sudo mv "${TMP_DIR}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
fi

echo ""
echo "poshgre ${VERSION} has been installed to ${INSTALL_DIR}/${BINARY_NAME}"
echo "Run 'posh --help' to get started."
