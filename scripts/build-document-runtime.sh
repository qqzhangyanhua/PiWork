#!/bin/bash
set -euo pipefail

if [[ -d /opt/homebrew/bin ]]; then
  PATH="/opt/homebrew/bin:${PATH}"
fi

PROFILE="${1:-debug}"
if [[ "${PROFILE}" != "debug" && "${PROFILE}" != "release" ]]; then
  echo "Profile must be debug or release, got: ${PROFILE}" >&2
  exit 1
fi

if ! command -v cmake >/dev/null; then
  echo "cmake is required to build the document-runtime helper on macOS." >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="${REPO_ROOT}/src-tauri/document-runtime/Cargo.toml"
TARGET_ROOT="${REPO_ROOT}/src-tauri/target/document-runtime"
EXECUTABLE_NAME="piwork-document-runtime"

CARGO_ARGS=(build --manifest-path "${MANIFEST}" --target-dir "${TARGET_ROOT}")
if [[ "${PROFILE}" == "release" ]]; then
  CARGO_ARGS+=(--release)
fi

cargo "${CARGO_ARGS[@]}"

BUILT="${TARGET_ROOT}/${PROFILE}/${EXECUTABLE_NAME}"
RUNTIME_RESOURCE="${REPO_ROOT}/src-tauri/binaries/document-runtime/${EXECUTABLE_NAME}"
RUNTIME_BESIDE_APP="${REPO_ROOT}/src-tauri/target/${PROFILE}/document-runtime/${EXECUTABLE_NAME}"

mkdir -p "$(dirname "${RUNTIME_RESOURCE}")"
mkdir -p "$(dirname "${RUNTIME_BESIDE_APP}")"
cp "${BUILT}" "${RUNTIME_RESOURCE}"
cp "${BUILT}" "${RUNTIME_BESIDE_APP}"
chmod +x "${RUNTIME_RESOURCE}" "${RUNTIME_BESIDE_APP}"
