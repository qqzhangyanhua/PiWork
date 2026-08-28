#!/bin/bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "Bundled Node is downloaded only on Apple Silicon Mac." >&2
  exit 1
fi

NODE_VERSION="v24.13.0"
TARBALL="node-${NODE_VERSION}-darwin-arm64.tar.gz"
URL="https://nodejs.org/dist/${NODE_VERSION}/${TARBALL}"
# Official SHA256 from https://nodejs.org/dist/v24.13.0/SHASUMS256.txt
SHA256="d595961e563fcae057d4a0fb992f175a54d97fcc4a14dc2d474d92ddeea3b9f8"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${REPO_ROOT}/src-tauri/binaries/pi-sidecar/node"
CACHE_DIR="${REPO_ROOT}/src-tauri/target/darwin-node"
CACHE_TAR="${CACHE_DIR}/${TARBALL}"

mkdir -p "${CACHE_DIR}"
mkdir -p "$(dirname "${DEST}")"

tarball_matches_pin() {
  echo "${SHA256}  ${CACHE_TAR}" | shasum -a 256 -c - >/dev/null
}

if [[ ! -f "${CACHE_TAR}" ]] || ! tarball_matches_pin; then
  rm -f "${CACHE_TAR}"
  curl -fL --retry 3 --retry-all-errors -o "${CACHE_TAR}" "${URL}"
  tarball_matches_pin
fi

EXTRACT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/piwork-darwin-node.XXXXXX")"
cleanup() {
  rm -rf "${EXTRACT_DIR}"
}
trap cleanup EXIT

tar -xzf "${CACHE_TAR}" -C "${EXTRACT_DIR}"
NODE_BIN="${EXTRACT_DIR}/node-${NODE_VERSION}-darwin-arm64/bin/node"
if [[ ! -f "${NODE_BIN}" ]]; then
  echo "Official Node tarball did not contain bin/node" >&2
  exit 1
fi

cp "${NODE_BIN}" "${DEST}"
chmod +x "${DEST}"

ACTUAL="$("${DEST}" -v)"
if [[ "${ACTUAL}" != "${NODE_VERSION}" ]]; then
  echo "Bundled Node reported ${ACTUAL}, expected ${NODE_VERSION}" >&2
  exit 1
fi
