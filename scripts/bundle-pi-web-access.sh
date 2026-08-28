#!/bin/bash
# Build the vendored pi-web-access extension under the Pi sidecar.
# Output is gitignored; run before tauri dev/build (see tauri*.conf.json).
set -euo pipefail

PACKAGE_NAME="pi-web-access"
PACKAGE_VERSION="0.24.0"
TYPEBOX_VERSION="1.1.38"
EXPECTED_INTEGRITY="sha512-BVosva1tGDhHveaGpFnc++YS5+pzmWVzJ/5B+1xBavkRjAgyDvMpA1EfVL+GYIviAxXKck9JyRGVzo4ASV7snA=="
REGISTRY="https://registry.npmjs.org"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SIDECAR_ROOT="${REPO_ROOT}/src-tauri/binaries/pi-sidecar"
TARGET="${SIDECAR_ROOT}/builtin-extensions/pi-web-access"
ENTRY="${TARGET}/node_modules/pi-web-access/index.ts"
MANIFEST="${TARGET}/node_modules/pi-web-access/package.json"
TYPEBOX_MANIFEST="${TARGET}/node_modules/typebox/package.json"
MODULES="${TARGET}/node_modules"

if [[ ! -d "${SIDECAR_ROOT}" ]]; then
  echo "Pi sidecar directory is missing: ${SIDECAR_ROOT}" >&2
  exit 1
fi

case "$(cd "$(dirname "${TARGET}")" && pwd)/" in
  "${SIDECAR_ROOT}/"*) ;;
  *)
    echo "Refusing to write outside the Pi sidecar directory." >&2
    exit 1
    ;;
esac

already_bundled() {
  [[ -f "${ENTRY}" && -f "${MANIFEST}" && -f "${TYPEBOX_MANIFEST}" ]] || return 1
  local installed typebox
  installed="$(node -e "console.log(JSON.parse(require('fs').readFileSync(process.argv[1],'utf8')).version)" "${MANIFEST}")"
  typebox="$(node -e "console.log(JSON.parse(require('fs').readFileSync(process.argv[1],'utf8')).version)" "${TYPEBOX_MANIFEST}")"
  [[ "${installed}" == "${PACKAGE_VERSION}" && "${typebox}" == "${TYPEBOX_VERSION}" ]] || return 1
  [[ ! -f "${MODULES}/pi-web-access/pi-web-fetch-demo.mp4" ]] || return 1
  [[ ! -f "${MODULES}/pi-web-access/banner.png" ]] || return 1
  [[ ! -d "${MODULES}/@mixmark-io/domino/test" ]] || return 1
  return 0
}

if already_bundled; then
  echo "Bundled ${PACKAGE_NAME}@${PACKAGE_VERSION} already present at ${TARGET}"
  exit 0
fi

if ! command -v npm >/dev/null; then
  echo "npm is required to bundle ${PACKAGE_NAME}." >&2
  exit 1
fi

TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/piwork-web-access.XXXXXX")"
cleanup() {
  rm -rf "${TEMP_ROOT}"
}
trap cleanup EXIT

pushd "${TEMP_ROOT}" >/dev/null
PACK_JSON="$(npm pack "${PACKAGE_NAME}@${PACKAGE_VERSION}" --json --ignore-scripts --registry="${REGISTRY}")"
popd >/dev/null

FILENAME="$(node -e "
const packs = JSON.parse(process.argv[1]);
if (!Array.isArray(packs) || packs.length !== 1) process.exit(2);
if (packs[0].integrity !== process.argv[2]) process.exit(3);
process.stdout.write(packs[0].filename);
" "${PACK_JSON}" "${EXPECTED_INTEGRITY}")" || {
  echo "The downloaded ${PACKAGE_NAME} package did not match the pinned integrity." >&2
  exit 1
}
TARBALL="${TEMP_ROOT}/${FILENAME}"

if [[ -e "${TARGET}" ]]; then
  RESOLVED_TARGET="$(cd "${TARGET}" && pwd)"
  case "${RESOLVED_TARGET}/" in
    "${SIDECAR_ROOT}/"*) ;;
    *)
      echo "Refusing to replace a target outside the Pi sidecar directory." >&2
      exit 1
      ;;
  esac
  rm -rf "${TARGET}"
fi
mkdir -p "${TARGET}"

# pi-web-access declares typebox with a caret range, but its current API is
# compatible with the 1.1.x version bundled by Pi, not typebox 1.3.x.
npm install --prefix "${TARGET}" --ignore-scripts --omit=dev --omit=optional \
  --legacy-peer-deps --no-audit --no-fund --no-save --package-lock=false \
  "${TARBALL}" "typebox@${TYPEBOX_VERSION}"

if [[ ! -f "${ENTRY}" ]]; then
  echo "Bundled extension entry point is missing." >&2
  exit 1
fi

INSTALLED="$(node -e "console.log(JSON.parse(require('fs').readFileSync(process.argv[1],'utf8')).version)" "${MANIFEST}")"
if [[ "${INSTALLED}" != "${PACKAGE_VERSION}" ]]; then
  echo "Bundled extension version does not match ${PACKAGE_VERSION}." >&2
  exit 1
fi

INSTALLED_TYPEBOX="$(node -e "console.log(JSON.parse(require('fs').readFileSync(process.argv[1],'utf8')).version)" "${TYPEBOX_MANIFEST}")"
if [[ "${INSTALLED_TYPEBOX}" != "${TYPEBOX_VERSION}" ]]; then
  echo "Bundled typebox version does not match Pi's extension API version." >&2
  exit 1
fi

# Strip packaging weight (tests, docs, yarn metadata, media).
find "${MODULES}" \( -type d \( -name test -o -name tests -o -name docs -o -name .yarn \) \) -prune -exec rm -rf {} +
find "${MODULES}" \( -type f \( -name banner.png -o -name '*.mp4' \) \) -delete

if [[ -f "${MODULES}/pi-web-access/pi-web-fetch-demo.mp4" || -f "${MODULES}/pi-web-access/banner.png" ]]; then
  echo "Packaging weight was not stripped from the bundled extension." >&2
  exit 1
fi

PI_ENTRYPOINT="${SIDECAR_ROOT}/dist/piwork-pi.js"
export PI_CODING_AGENT_DIR="${TEMP_ROOT}/agent"
set +e
SMOKE_OUTPUT="$(printf '%s\n' '{"id":"piwork-extension-smoke","type":"get_state"}' |
  node "${PI_ENTRYPOINT}" --mode rpc --offline --no-extensions --extension "${ENTRY}" --no-session 2>&1)"
SMOKE_STATUS=$?
set -e
unset PI_CODING_AGENT_DIR

if [[ "${SMOKE_STATUS}" -ne 0 || "${SMOKE_OUTPUT}" == *"Failed to load extension"* ]]; then
  echo "Bundled extension failed the Pi RPC load check: ${SMOKE_OUTPUT}" >&2
  exit 1
fi

echo "Bundled ${PACKAGE_NAME}@${PACKAGE_VERSION} at ${TARGET}"
