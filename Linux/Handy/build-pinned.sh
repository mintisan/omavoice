#!/usr/bin/env bash
set -euo pipefail

HANDY_REPOSITORY="https://github.com/cjpais/handy.git"
HANDY_COMMIT="9bcb6d9d46c88517d2b5519d3a4f900ee3968c99"
HANDY_TREE="65254d74f1a0465ac684790f29a79c9c894c5dc1"
HANDY_CARGO_LOCK_BLOB="02cadd4eaaef5f10863a17b3bbe48b4b2a12fdcb"
HANDY_BUN_LOCK_BLOB="6ac7785818835bee3aef3f5439ff77a60245bef1"
HANDY_PACKAGE_JSON_BLOB="9d1e1086769c3d47b09285afc84988428d748666"
BUN_VERSION="1.3.14"

usage() {
  cat <<'EOF'
Usage: build-pinned.sh [OUTPUT_DIRECTORY]

Build the OmaVoice-reviewed Handy revision. OUTPUT_DIRECTORY must not exist or
must be empty. The default is Linux/Handy/build.

This downloads Handy source plus its locked npm and Cargo dependencies. It does
not download a speech-recognition model.
EOF
}

fail() {
  printf 'Handy build: %s\n' "$*" >&2
  exit 1
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

[[ $# -le 1 ]] || {
  usage >&2
  exit 2
}

SCRIPT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIRECTORY="${1:-${SCRIPT_DIRECTORY}/build}"

for command in git mise cargo rustc cmake clang glslc pkg-config sha256sum find sort xargs; do
  command -v "$command" >/dev/null 2>&1 || fail "required command not found: ${command}"
done

for package in alsa gtk+-3.0 webkit2gtk-4.1 ayatana-appindicator3-0.1 gtk-layer-shell-0 openblas vulkan; do
  pkg-config --exists "$package" || fail "required pkg-config package not found: ${package}"
done

if [[ -e "$OUTPUT_DIRECTORY" ]]; then
  [[ -d "$OUTPUT_DIRECTORY" ]] || fail "output path is not a directory: ${OUTPUT_DIRECTORY}"
  [[ -z "$(find "$OUTPUT_DIRECTORY" -mindepth 1 -maxdepth 1 -print -quit)" ]] || \
    fail "output directory is not empty: ${OUTPUT_DIRECTORY}"
else
  mkdir -p "$OUTPUT_DIRECTORY"
fi

OUTPUT_DIRECTORY="$(cd -- "$OUTPUT_DIRECTORY" && pwd)"
SOURCE_DIRECTORY="${OUTPUT_DIRECTORY}/source"
TARGET_DIRECTORY="${OUTPUT_DIRECTORY}/target"

git init --quiet "$SOURCE_DIRECTORY"
git -C "$SOURCE_DIRECTORY" remote add origin "$HANDY_REPOSITORY"
git -C "$SOURCE_DIRECTORY" fetch --quiet --depth=1 origin "$HANDY_COMMIT"
git -C "$SOURCE_DIRECTORY" checkout --quiet --detach FETCH_HEAD

[[ "$(git -C "$SOURCE_DIRECTORY" rev-parse HEAD)" == "$HANDY_COMMIT" ]] || \
  fail "source commit does not match the reviewed revision"
[[ "$(git -C "$SOURCE_DIRECTORY" rev-parse 'HEAD^{tree}')" == "$HANDY_TREE" ]] || \
  fail "source tree does not match the reviewed revision"
[[ "$(git -C "$SOURCE_DIRECTORY" rev-parse HEAD:src-tauri/Cargo.lock)" == "$HANDY_CARGO_LOCK_BLOB" ]] || \
  fail "Cargo.lock does not match the reviewed revision"
[[ "$(git -C "$SOURCE_DIRECTORY" rev-parse HEAD:bun.lock)" == "$HANDY_BUN_LOCK_BLOB" ]] || \
  fail "bun.lock does not match the reviewed revision"
[[ "$(git -C "$SOURCE_DIRECTORY" rev-parse HEAD:package.json)" == "$HANDY_PACKAGE_JSON_BLOB" ]] || \
  fail "package.json does not match the reviewed revision"

mise where "bun@${BUN_VERSION}" >/dev/null 2>&1 || \
  fail "bun ${BUN_VERSION} is not installed in mise"
[[ "$(MISE_BUN_VERSION="$BUN_VERSION" mise x "bun@${BUN_VERSION}" -- bun --version)" == "$BUN_VERSION" ]] || \
  fail "mise did not select bun ${BUN_VERSION}"

(
  cd "$SOURCE_DIRECTORY"
  MISE_BUN_VERSION="$BUN_VERSION" mise x "bun@${BUN_VERSION}" -- bun install --frozen-lockfile
  CARGO_TARGET_DIR="$TARGET_DIRECTORY" \
    MISE_BUN_VERSION="$BUN_VERSION" \
    mise x "bun@${BUN_VERSION}" -- bun run tauri build --no-bundle
)

RELEASE_DIRECTORY="${TARGET_DIRECTORY}/release"
LIBRARY_DIRECTORY="${SOURCE_DIRECTORY}/src-tauri/transcribe-libs"
RESOURCE_DIRECTORY="${RELEASE_DIRECTORY}/resources"

[[ -x "${RELEASE_DIRECTORY}/handy" ]] || fail "release binary was not built"
[[ -f "${LIBRARY_DIRECTORY}/libtranscribe.so.0" ]] || fail "libtranscribe.so.0 was not built"
[[ -f "${LIBRARY_DIRECTORY}/libggml.so.0" ]] || fail "libggml.so.0 was not built"
[[ -f "${LIBRARY_DIRECTORY}/libggml-base.so.0" ]] || fail "libggml-base.so.0 was not built"
[[ -f "${LIBRARY_DIRECTORY}/libggml-vulkan.so" ]] || fail "Vulkan inference library was not built"
compgen -G "${LIBRARY_DIRECTORY}/libggml-cpu*.so" >/dev/null || fail "CPU inference library was not built"
[[ -f "${RESOURCE_DIRECTORY}/default_settings.json" ]] || fail "default settings resource is missing"
[[ -f "${RESOURCE_DIRECTORY}/models/silero_vad_v4.onnx" ]] || fail "VAD resource is missing"
[[ -f "${SOURCE_DIRECTORY}/LICENSE" ]] || fail "Handy license is missing"
[[ -f "${SOURCE_DIRECTORY}/src-tauri/icons/128x128@2x.png" ]] || fail "Handy icon is missing"
UNEXPECTED_ENTRY="$(find "$LIBRARY_DIRECTORY" "$RESOURCE_DIRECTORY" -mindepth 1 ! -type d ! -type f -print -quit)"
[[ -z "$UNEXPECTED_ENTRY" ]] || fail "runtime output contains a non-regular entry: ${UNEXPECTED_ENTRY}"

cat >"${OUTPUT_DIRECTORY}/BUILD-METADATA" <<EOF
HANDY_REPOSITORY=${HANDY_REPOSITORY}
HANDY_COMMIT=${HANDY_COMMIT}
HANDY_TREE=${HANDY_TREE}
HANDY_CARGO_LOCK_BLOB=${HANDY_CARGO_LOCK_BLOB}
HANDY_BUN_LOCK_BLOB=${HANDY_BUN_LOCK_BLOB}
HANDY_PACKAGE_JSON_BLOB=${HANDY_PACKAGE_JSON_BLOB}
BUN_VERSION=${BUN_VERSION}
RUSTC_VERSION=$(rustc --version)
EOF

(
  cd "$OUTPUT_DIRECTORY"
  find \
    target/release/handy \
    source/LICENSE \
    source/src-tauri/icons/128x128@2x.png \
    source/src-tauri/transcribe-libs \
    target/release/resources \
    -type f -print0 \
    | sort -z \
    | xargs -0 sha256sum >SHA256SUMS
)

printf 'Handy build complete: %s\n' "$OUTPUT_DIRECTORY"
printf 'Install with: %s/install-user.sh --build-directory %q\n' "$SCRIPT_DIRECTORY" "$OUTPUT_DIRECTORY"
