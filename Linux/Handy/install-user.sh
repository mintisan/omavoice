#!/usr/bin/env bash
set -euo pipefail

HANDY_COMMIT="9bcb6d9d46c88517d2b5519d3a4f900ee3968c99"
HANDY_TREE="65254d74f1a0465ac684790f29a79c9c894c5dc1"
MANAGED_MARKER=".sayall-managed"

usage() {
  cat <<'EOF'
Usage:
  install-user.sh [--build-directory DIRECTORY] [--staging-root DIRECTORY] [--no-enable]
  install-user.sh --uninstall [--staging-root DIRECTORY]

Install the OmaVoice-reviewed Handy build without sudo. If --build-directory is
omitted, build-pinned.sh creates Linux/Handy/build first. --staging-root is for
package and installer verification; it prefixes every destination path and
does not call systemctl. --no-enable installs the live files without enabling
or starting the graphical-session user service.

The installer does not create or change Handy settings, download a model, or
change the selected model/API. Use Handy's own settings UI for those operations.
EOF
}

fail() {
  printf 'Handy install: %s\n' "$*" >&2
  exit 1
}

SCRIPT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIRECTORY=""
STAGING_ROOT=""
UNINSTALL=0
ENABLE_SERVICE=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --build-directory)
      [[ $# -ge 2 ]] || fail "--build-directory requires a value"
      BUILD_DIRECTORY="$2"
      shift 2
      ;;
    --staging-root)
      [[ $# -ge 2 ]] || fail "--staging-root requires a value"
      STAGING_ROOT="$2"
      shift 2
      ;;
    --uninstall)
      UNINSTALL=1
      shift
      ;;
    --no-enable)
      ENABLE_SERVICE=0
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage >&2
      fail "unknown argument: $1"
      ;;
  esac
done

[[ "$STAGING_ROOT" != "/" ]] || fail "--staging-root cannot be the system root"
if [[ -n "$STAGING_ROOT" ]]; then
  mkdir -p "$STAGING_ROOT"
  STAGING_ROOT="$(cd -- "$STAGING_ROOT" && pwd)"
  ENABLE_SERVICE=0
fi

destination() {
  printf '%s%s' "$STAGING_ROOT" "$1"
}

BIN_DIRECTORY="$(destination "$HOME/.local/bin")"
LIBRARY_PARENT="$(destination "$HOME/.local/lib")"
LIBRARY_DIRECTORY="${LIBRARY_PARENT}/Handy"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
[[ "$CONFIG_HOME" == /* ]] || fail "XDG_CONFIG_HOME must be an absolute path"
UNIT_DIRECTORY="$(destination "$CONFIG_HOME/systemd/user")"
APPLICATION_DIRECTORY="$(destination "$HOME/.local/share/applications")"
ICON_DIRECTORY="$(destination "$HOME/.local/share/icons/hicolor/256x256/apps")"
LICENSE_DIRECTORY="$(destination "$HOME/.local/share/licenses/Handy")"
BINARY="${BIN_DIRECTORY}/handy"
LAUNCHER="${BIN_DIRECTORY}/sayall-handy"
UNIT_FILE="${UNIT_DIRECTORY}/sayall-handy.service"
DESKTOP_FILE="${APPLICATION_DIRECTORY}/com.pais.handy.desktop"
ICON_FILE="${ICON_DIRECTORY}/handy.png"
LICENSE_FILE="${LICENSE_DIRECTORY}/LICENSE"

if (( UNINSTALL )); then
  [[ -z "$BUILD_DIRECTORY" ]] || fail "--build-directory cannot be used with --uninstall"
  (( ENABLE_SERVICE )) || [[ -n "$STAGING_ROOT" ]] || fail "--no-enable cannot be used with --uninstall"
  if [[ ! -e "$LIBRARY_DIRECTORY" ]]; then
    printf 'Handy is not installed by OmaVoice.\n'
    exit 0
  fi
  [[ -f "${LIBRARY_DIRECTORY}/${MANAGED_MARKER}" ]] || \
    fail "refusing to remove an installation not managed by OmaVoice: ${LIBRARY_DIRECTORY}"
  if [[ -z "$STAGING_ROOT" ]] && command -v systemctl >/dev/null 2>&1; then
    systemctl --user disable --now sayall-handy.service 2>/dev/null || true
  fi
  if [[ -z "$STAGING_ROOT" ]] && pgrep -x handy >/dev/null 2>&1; then
    fail "Handy is still running; quit it before uninstalling"
  fi
  rm -f \
    "$BINARY" \
    "$LAUNCHER" \
    "$UNIT_FILE" \
    "${UNIT_DIRECTORY}/graphical-session.target.wants/sayall-handy.service" \
    "$DESKTOP_FILE" \
    "$ICON_FILE" \
    "$LICENSE_FILE"
  rm -rf "$LIBRARY_DIRECTORY"
  rmdir "$LICENSE_DIRECTORY" "$ICON_DIRECTORY" "$UNIT_DIRECTORY" 2>/dev/null || true
  if [[ -z "$STAGING_ROOT" ]] && command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload
  fi
  printf 'Removed the OmaVoice-managed Handy installation. User settings and models were preserved.\n'
  exit 0
fi

if [[ -z "$BUILD_DIRECTORY" ]]; then
  BUILD_DIRECTORY="${SCRIPT_DIRECTORY}/build"
  if [[ ! -f "${BUILD_DIRECTORY}/BUILD-METADATA" ]]; then
    "$SCRIPT_DIRECTORY/build-pinned.sh" "$BUILD_DIRECTORY"
  fi
fi

BUILD_DIRECTORY="$(cd -- "$BUILD_DIRECTORY" && pwd)"
SOURCE_DIRECTORY="${BUILD_DIRECTORY}/source"
RELEASE_DIRECTORY="${BUILD_DIRECTORY}/target/release"
LIBRARY_SOURCE="${SOURCE_DIRECTORY}/src-tauri/transcribe-libs"
RESOURCE_SOURCE="${RELEASE_DIRECTORY}/resources"

for command in git sha256sum find sort comm cut ldd; do
  command -v "$command" >/dev/null 2>&1 || fail "required command not found: ${command}"
done

[[ -f "${BUILD_DIRECTORY}/BUILD-METADATA" ]] || fail "BUILD-METADATA is missing"
[[ -f "${BUILD_DIRECTORY}/SHA256SUMS" ]] || fail "SHA256SUMS is missing"
grep -Fqx "HANDY_COMMIT=${HANDY_COMMIT}" "${BUILD_DIRECTORY}/BUILD-METADATA" || \
  fail "build metadata has an unreviewed Handy commit"
grep -Fqx "HANDY_TREE=${HANDY_TREE}" "${BUILD_DIRECTORY}/BUILD-METADATA" || \
  fail "build metadata has an unreviewed Handy tree"
[[ "$(git -C "$SOURCE_DIRECTORY" rev-parse HEAD)" == "$HANDY_COMMIT" ]] || \
  fail "build source has an unreviewed Handy commit"
[[ "$(git -C "$SOURCE_DIRECTORY" rev-parse 'HEAD^{tree}')" == "$HANDY_TREE" ]] || \
  fail "build source has an unreviewed Handy tree"

EXPECTED_FILES="$(mktemp)"
ACTUAL_FILES="$(mktemp)"
TEMP_LIBRARY=""
cleanup() {
  rm -f "$EXPECTED_FILES" "$ACTUAL_FILES"
  [[ -z "$TEMP_LIBRARY" ]] || rm -rf "$TEMP_LIBRARY"
}
trap cleanup EXIT

cut -c 67- "${BUILD_DIRECTORY}/SHA256SUMS" | sort -u >"$EXPECTED_FILES"
while IFS= read -r path; do
  case "$path" in
    target/release/handy|source/LICENSE|source/src-tauri/icons/128x128@2x.png|\
    source/src-tauri/transcribe-libs/*|target/release/resources/*) ;;
    *) fail "SHA256SUMS contains an unexpected path: ${path}" ;;
  esac
  [[ "$path" != *"/../"* && "$path" != ../* && "$path" != /* ]] || \
    fail "SHA256SUMS contains an unsafe path: ${path}"
done <"$EXPECTED_FILES"

(
  cd "$BUILD_DIRECTORY"
  find \
    target/release/handy \
    source/LICENSE \
    source/src-tauri/icons/128x128@2x.png \
    source/src-tauri/transcribe-libs \
    target/release/resources \
    -type f -print \
    | sort -u >"$ACTUAL_FILES"
)
FILE_DIFFERENCES="$(comm -3 "$EXPECTED_FILES" "$ACTUAL_FILES")"
if [[ -n "$FILE_DIFFERENCES" ]]; then
  printf '%s\n' "$FILE_DIFFERENCES" >&2
  fail "build contents do not match SHA256SUMS"
fi
(
  cd "$BUILD_DIRECTORY"
  sha256sum --quiet --check SHA256SUMS
)

[[ -x "${RELEASE_DIRECTORY}/handy" ]] || fail "release binary is not executable"
[[ -f "${LIBRARY_SOURCE}/libtranscribe.so.0" ]] || fail "libtranscribe.so.0 is missing"
[[ -f "${LIBRARY_SOURCE}/libggml-vulkan.so" ]] || fail "Vulkan inference library is missing"
compgen -G "${LIBRARY_SOURCE}/libggml-cpu*.so" >/dev/null || fail "CPU inference library is missing"
[[ -f "${RESOURCE_SOURCE}/default_settings.json" ]] || fail "default settings resource is missing"
[[ -f "${RESOURCE_SOURCE}/models/silero_vad_v4.onnx" ]] || fail "VAD resource is missing"
UNEXPECTED_ENTRY="$(find "$LIBRARY_SOURCE" "$RESOURCE_SOURCE" -mindepth 1 ! -type d ! -type f -print -quit)"
[[ -z "$UNEXPECTED_ENTRY" ]] || fail "runtime output contains a non-regular entry: ${UNEXPECTED_ENTRY}"

if [[ -e "$LIBRARY_DIRECTORY" && ! -f "${LIBRARY_DIRECTORY}/${MANAGED_MARKER}" ]]; then
  fail "refusing to overwrite an installation not managed by OmaVoice: ${LIBRARY_DIRECTORY}"
fi
if [[ -e "$BINARY" && ! -f "${LIBRARY_DIRECTORY}/${MANAGED_MARKER}" ]]; then
  fail "refusing to overwrite a binary not managed by OmaVoice: ${BINARY}"
fi
if [[ -e "$LAUNCHER" && ! -f "${LIBRARY_DIRECTORY}/${MANAGED_MARKER}" ]]; then
  fail "refusing to overwrite a launcher not managed by OmaVoice: ${LAUNCHER}"
fi
if [[ -z "$STAGING_ROOT" ]] && pgrep -x handy >/dev/null 2>&1; then
  fail "Handy is running; quit it before installing"
fi

mkdir -p "$BIN_DIRECTORY" "$LIBRARY_PARENT" "$UNIT_DIRECTORY" "$APPLICATION_DIRECTORY" \
  "$ICON_DIRECTORY" "$LICENSE_DIRECTORY"
TEMP_LIBRARY="$(mktemp -d "${LIBRARY_PARENT}/.Handy.install.XXXXXX")"
cp -a "${LIBRARY_SOURCE}/." "$TEMP_LIBRARY/"
mkdir -p "${TEMP_LIBRARY}/resources"
cp -a "${RESOURCE_SOURCE}/." "${TEMP_LIBRARY}/resources/"
printf '%s\n' "$HANDY_COMMIT" >"${TEMP_LIBRARY}/${MANAGED_MARKER}"
cp "${BUILD_DIRECTORY}/BUILD-METADATA" "${TEMP_LIBRARY}/BUILD-METADATA"
cp "${BUILD_DIRECTORY}/SHA256SUMS" "${TEMP_LIBRARY}/BUILD-SHA256SUMS"
find "$TEMP_LIBRARY" -type d -exec chmod 0755 {} +
find "$TEMP_LIBRARY" -type f -exec chmod 0644 {} +

rm -rf "$LIBRARY_DIRECTORY"
mv "$TEMP_LIBRARY" "$LIBRARY_DIRECTORY"
TEMP_LIBRARY=""
install -m 0755 "${RELEASE_DIRECTORY}/handy" "$BINARY"
install -m 0755 "${SCRIPT_DIRECTORY}/sayall-handy" "$LAUNCHER"
install -m 0644 "${SCRIPT_DIRECTORY}/sayall-handy.service" "$UNIT_FILE"
install -m 0644 "${SOURCE_DIRECTORY}/src-tauri/icons/128x128@2x.png" "$ICON_FILE"
install -m 0644 "${SOURCE_DIRECTORY}/LICENSE" "$LICENSE_FILE"
install -m 0644 "${SCRIPT_DIRECTORY}/com.pais.handy.desktop" "$DESKTOP_FILE"

if ldd "$BINARY" | grep -q 'not found'; then
  ldd "$BINARY" >&2
  fail "installed Handy binary has unresolved runtime libraries"
fi

if (( ENABLE_SERVICE )); then
  command -v systemctl >/dev/null 2>&1 || fail "systemctl is required to enable the Handy user service"
  systemctl --user daemon-reload
  systemctl --user enable --now sayall-handy.service
fi

printf 'Installed Handy %s for the current user.\n' "$HANDY_COMMIT"
printf 'Settings and models remain owned by Handy under ~/.local/share/com.pais.handy.\n'
