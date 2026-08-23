#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'release lifecycle test: %s\n' "$*" >&2
  exit 1
}

[[ $# == 1 ]] || fail 'usage: test-lifecycle.sh RELEASE_ROOT'
root=$(realpath "$1")
[[ -x $root/install.sh && -x $root/uninstall.sh && -f $root/PAYLOAD-SHA256SUMS ]] ||
  fail 'invalid release root'

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stage=$work/stage
home=/home/omavoice-release-test
config=$home/.config
data=$home/.local/share
environment=(
  env
  HOME=$home
  XDG_CONFIG_HOME=$config
  XDG_DATA_HOME=$data
  LANG=C
  LANGUAGE=
  LC_ALL=
  LC_MESSAGES=
)

"${environment[@]}" "$root/install.sh" --staging-root "$stage" >/dev/null
manifest=$stage$data/omavoice/install-manifest-v1.tsv
[[ -f $manifest ]] || fail 'install manifest was not created'
first_manifest=$(sha256sum "$manifest")
"${environment[@]}" "$root/install.sh" --staging-root "$stage" >/dev/null
second_manifest=$(sha256sum "$manifest")
[[ $first_manifest == "$second_manifest" ]] || fail 'repeated install changed its ownership manifest'

mkdir -p "$stage$data/sayall" "$stage$data/com.pais.handy" "$stage$home/.local/bin"
printf 'keep\n' >"$stage$data/sayall/user-data-sentinel"
printf 'keep\n' >"$stage$data/com.pais.handy/handy-data-sentinel"
printf 'keep\n' >"$stage$home/.local/bin/unrelated-sentinel"
printf 'locally modified\n' >"$stage$home/.local/bin/sayallctl"

"${environment[@]}" "$root/uninstall.sh" --staging-root "$stage" >/dev/null
[[ $(cat "$stage$data/sayall/user-data-sentinel") == keep ]] || fail 'OmaVoice user data was removed'
[[ $(cat "$stage$data/com.pais.handy/handy-data-sentinel") == keep ]] || fail 'Handy user data was removed'
[[ $(cat "$stage$home/.local/bin/unrelated-sentinel") == keep ]] || fail 'an unrelated file was removed'
[[ $(cat "$stage$home/.local/bin/sayallctl") == 'locally modified' ]] ||
  fail 'a locally modified managed file was removed'
[[ ! -e $stage$home/.local/bin/sayall-settings ]] || fail 'an unchanged managed file remains'
[[ ! -e $stage$home/.local/lib/Handy ]] || fail 'the managed Handy library remains'

modified_stage=$work/modified-stage
"${environment[@]}" "$root/install.sh" --staging-root "$modified_stage" >/dev/null
printf 'locally modified\n' >>"$modified_stage$home/.local/lib/Handy/.sayall-managed"
"${environment[@]}" "$modified_stage$home/.local/bin/sayallctl" \
  --staging-root "$modified_stage" uninstall >/dev/null
[[ -d $modified_stage$home/.local/lib/Handy ]] || fail 'a locally modified Handy library was removed'

collision_stage=$work/collision-stage
mkdir -p "$collision_stage$home/.local/bin"
printf 'unmanaged\n' >"$collision_stage$home/.local/bin/sayallctl"
if "${environment[@]}" "$root/install.sh" --staging-root "$collision_stage" >/dev/null 2>&1; then
  fail 'an unmanaged destination file was overwritten'
fi
[[ $(cat "$collision_stage$home/.local/bin/sayallctl") == unmanaged ]] ||
  fail 'an unmanaged destination file was changed'

outside=$work/outside
mkdir -p "$outside"
printf 'keep\n' >"$outside/sentinel"
if env HOME=/../outside XDG_CONFIG_HOME= XDG_DATA_HOME= LANG=C \
  "$root/uninstall.sh" --staging-root "$work/unsafe-stage" >/dev/null 2>&1; then
  fail 'an unsafe HOME path was accepted'
fi
[[ $(cat "$outside/sentinel") == keep ]] || fail 'an unsafe HOME path escaped staging'

symlink_stage=$work/symlink-stage
mkdir -p "$symlink_stage$home/.local"
ln -s "$outside" "$symlink_stage$home/.local/bin"
if "${environment[@]}" "$root/install.sh" --staging-root "$symlink_stage" >/dev/null 2>&1; then
  fail 'a staging symlink escape was accepted'
fi
[[ $(cat "$outside/sentinel") == keep ]] || fail 'a staging symlink escaped its root'

echo 'Release install lifecycle passed.'
