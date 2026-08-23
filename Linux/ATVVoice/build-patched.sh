#!/usr/bin/env bash

set -euo pipefail

readonly UPSTREAM_URL="https://github.com/b0o/ATVVoice.git"
readonly BASE_COMMIT="f36286d8185cb2b9b219cd91a9c0e08091999c9d"
readonly EXPECTED_TREE="df607e5c9609673fef683de1c02a3411b1acbd5d"

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR

if (( $# > 1 )); then
    printf 'Usage: bash %s [empty-output-directory]\n' "$0" >&2
    exit 2
fi

for command_name in git cargo sha256sum; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
        printf 'Required command not found: %s\n' "$command_name" >&2
        exit 1
    fi
done

if (( $# == 1 )); then
    WORK_DIR="$1"
    mkdir -p -- "$WORK_DIR"
    if [[ -n "$(find "$WORK_DIR" -mindepth 1 -print -quit)" ]]; then
        printf 'Output directory must be empty: %s\n' "$WORK_DIR" >&2
        exit 1
    fi
else
    WORK_DIR="$(mktemp -d -t sayall-atvvoice-build.XXXXXX)"
fi
readonly WORK_DIR
readonly SOURCE_DIR="$WORK_DIR/source"
readonly TARGET_DIR="$WORK_DIR/target"

printf 'Verifying OmaVoice ATVVoice patch set...\n'
(
    cd -- "$SCRIPT_DIR"
    sha256sum --check SHA256SUMS
)

printf 'Fetching ATVVoice base commit %s...\n' "$BASE_COMMIT"
git init --quiet "$SOURCE_DIR"
git -C "$SOURCE_DIR" remote add origin "$UPSTREAM_URL"
git -C "$SOURCE_DIR" fetch --quiet --depth=1 origin "$BASE_COMMIT"
git -C "$SOURCE_DIR" checkout --quiet --detach FETCH_HEAD

actual_base="$(git -C "$SOURCE_DIR" rev-parse HEAD)"
if [[ "$actual_base" != "$BASE_COMMIT" ]]; then
    printf 'Unexpected base commit: expected %s, got %s\n' "$BASE_COMMIT" "$actual_base" >&2
    exit 1
fi

printf 'Applying seven reviewed patches...\n'
while IFS= read -r patch_name; do
    GIT_COMMITTER_NAME="OmaVoice reproducible build" \
        GIT_COMMITTER_EMAIL="build@omavoice.app" \
        git -C "$SOURCE_DIR" am --quiet "$SCRIPT_DIR/$patch_name"
done < <(awk '{print $2}' "$SCRIPT_DIR/SHA256SUMS")

actual_tree="$(git -C "$SOURCE_DIR" rev-parse 'HEAD^{tree}')"
if [[ "$actual_tree" != "$EXPECTED_TREE" ]]; then
    printf 'Unexpected patched tree: expected %s, got %s\n' "$EXPECTED_TREE" "$actual_tree" >&2
    exit 1
fi

printf 'Testing patched ATVVoice...\n'
CARGO_TARGET_DIR="$TARGET_DIR" cargo test \
    --manifest-path "$SOURCE_DIR/Cargo.toml" \
    --locked \
    --all-targets

printf 'Building patched ATVVoice release binary...\n'
CARGO_TARGET_DIR="$TARGET_DIR" cargo build \
    --manifest-path "$SOURCE_DIR/Cargo.toml" \
    --locked \
    --release

printf '\nVerified patched source tree: %s\n' "$actual_tree"
printf 'Source checkout: %s\n' "$SOURCE_DIR"
printf 'Release binary: %s/release/atvvoice\n' "$TARGET_DIR"
printf 'Nothing was installed or started.\n'
