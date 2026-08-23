#!/usr/bin/env bash
set -euo pipefail
fail(){ echo "package-release: $*" >&2; exit 1; }
usage(){ cat <<'EOF'
Usage: Linux/package-release.sh --version VERSION --output-directory ABSOLUTE \
 --atvvoice-build-directory ABSOLUTE --handy-build-directory ABSOLUTE --commit 40_HEX
Bundles verified, already-built x86_64 release artifacts. Output must be empty.
EOF
}
version= out= atv= handy= commit=
while (($#)); do case $1 in --version) (($#>1))||fail 'missing --version'; version=$2; shift 2;; --output-directory) (($#>1))||fail 'missing --output-directory'; out=$2; shift 2;; --atvvoice-build-directory) (($#>1))||fail 'missing --atvvoice-build-directory'; atv=$2; shift 2;; --handy-build-directory) (($#>1))||fail 'missing --handy-build-directory'; handy=$2; shift 2;; --commit) (($#>1))||fail 'missing --commit'; commit=$2; shift 2;; -h|--help) usage; exit;; *) fail "unknown option: $1";; esac; done
[[ -n $version && -n $out && -n $atv && -n $handy && -n $commit ]]||fail 'all five options are required'
[[ $commit =~ ^[0-9a-f]{40}$ ]]||fail 'commit must be 40 lowercase hexadecimal characters'
for d in "$out" "$atv" "$handy"; do [[ $d == /* && $d != / ]]||fail "path must be absolute and non-root: $d"; done
[[ $(uname -m) == x86_64 ]]||fail 'build host must be x86_64'
for c in cargo rustc python3 git sha256sum find file readelf ldd strings cmp cut xargs realpath tar zstd desktop-file-validate systemd-analyze ln; do command -v "$c" >/dev/null||fail "required command missing: $c"; done
HERE=$(cd -- "$(dirname -- "$0")/.." && pwd -P); manifest=$HERE/Linux/OmaVoiceLinux/Cargo.toml
actual=$(sed -n '/^version = /{s/^version = "\([^"]*\)"/\1/p;q}' "$manifest"); [[ $version == "$actual" ]]||fail "version mismatch (Cargo.toml: $actual)"
git -C "$HERE" cat-file -e "$commit^{commit}" 2>/dev/null||fail 'commit does not exist in this repository'
[[ $(git -C "$HERE" rev-parse HEAD) == $commit ]]||fail 'commit must equal the checked-out HEAD'
[[ -z $(git -C "$HERE" status --porcelain --untracked-files=all) ]]||fail 'repository must be clean before packaging'
[[ -d $atv && -d $handy ]]||fail 'build directories must exist'; atv=$(realpath "$atv"); handy=$(realpath "$handy")
if [[ -e $out ]]; then [[ -d $out && -z $(find "$out" -mindepth 1 -print -quit) ]]||fail 'output directory is not clean'; else mkdir -p "$out"; fi; out=$(realpath "$out")
ATV_TREE=df607e5c9609673fef683de1c02a3411b1acbd5d; ATV_BASE=f36286d8185cb2b9b219cd91a9c0e08091999c9d
H_COMMIT=9bcb6d9d46c88517d2b5519d3a4f900ee3968c99; H_TREE=65254d74f1a0465ac684790f29a79c9c894c5dc1
[[ -x $atv/target/release/atvvoice && -d $atv/source/.git ]]||fail 'invalid ATVVoice build layout'
[[ $(git -C "$atv/source" rev-parse 'HEAD^{tree}') == $ATV_TREE ]]||fail 'unverified ATVVoice final tree'
(cd "$HERE/Linux/ATVVoice"&&sha256sum --quiet -c SHA256SUMS)||fail 'ATVVoice patch hashes failed'
[[ -f $handy/BUILD-METADATA && -f $handy/SHA256SUMS ]]||fail 'Handy build metadata missing'
grep -Fqx "HANDY_COMMIT=$H_COMMIT" "$handy/BUILD-METADATA"&&grep -Fqx "HANDY_TREE=$H_TREE" "$handy/BUILD-METADATA"||fail 'unverified Handy metadata'
[[ $(git -C "$handy/source" rev-parse HEAD) == $H_COMMIT && $(git -C "$handy/source" rev-parse 'HEAD^{tree}') == $H_TREE ]]||fail 'unverified Handy source'
expected=$(mktemp); actual_list=$(mktemp); work=$(mktemp -d); trap 'rm -rf "$work" "$expected" "$actual_list"' EXIT
cut -c67- "$handy/SHA256SUMS"|sort -u >"$expected"
while read -r p; do [[ $p != /* && $p != *'/../'* && $p != ../* ]]||fail "unsafe Handy checksum path: $p"; case $p in target/release/handy|source/LICENSE|source/src-tauri/icons/128x128@2x.png|source/src-tauri/transcribe-libs/*|target/release/resources/*);; *) fail "unexpected Handy checksum path: $p";; esac; done <"$expected"
(cd "$handy"; find target/release/handy source/LICENSE source/src-tauri/icons/128x128@2x.png source/src-tauri/transcribe-libs target/release/resources -type f|sort -u >"$actual_list"; sha256sum --quiet -c SHA256SUMS)||fail 'Handy checksums failed'
cmp -s "$expected" "$actual_list"||fail 'Handy SHA256SUMS does not exactly cover runtime output'
root="$work/OmaVoice-v$version-omarchy-arch-x86_64"; mkdir -p "$root"/{docs,Screenshots,payload/bin,payload/lib/Handy,payload/systemd/user,payload/share/applications,payload/share/icons/hicolor/{1024x1024,256x256}/apps,payload/share/licenses/Handy,optional-keyd,LICENSES/Rust/{OmaVoice,ATVVoice,Handy},LICENSES/JavaScript/Handy}
install -m0755 "$HERE/Linux/release/install.sh" "$root/install.sh"; install -m0755 "$HERE/Linux/release/uninstall.sh" "$root/uninstall.sh"
for f in README.md README.zh-CN.md LICENSE.md NOTICE.md THIRD_PARTY_NOTICES.md CHANGELOG.md CONTRIBUTING.md SECURITY.md RELEASE_NOTES.md; do install -m0644 "$HERE/$f" "$root/$f"; done
install -m0644 "$HERE"/docs/*.md "$root/docs/"
install -m0644 "$HERE"/Screenshots/*.png "$root/Screenshots/"
install -m0644 "$HERE/Linux/release/VERIFYING.md" "$HERE/Linux/release/VERIFYING.zh-CN.md" "$root/"
install -m0644 "$HERE/LICENSE.md" "$root/LICENSES/OmaVoice-GPL-3.0.txt"; install -m0644 "$HERE/LICENSES/ATVVoice-MIT.txt" "$HERE/LICENSES/Handy-MIT.txt" "$HERE/LICENSES/Silero-VAD-MIT.txt" "$root/LICENSES/"
cargo metadata --locked --offline --filter-platform x86_64-unknown-linux-gnu --format-version 1 --manifest-path "$manifest" >"$work/omavoice-metadata.json"; python3 "$HERE/Linux/release/rust-licenses.py" "$work/omavoice-metadata.json" "$root/LICENSES/Rust/OmaVoice"
cargo metadata --locked --offline --filter-platform x86_64-unknown-linux-gnu --format-version 1 --manifest-path "$atv/source/Cargo.toml" >"$work/atvvoice-metadata.json"; python3 "$HERE/Linux/release/rust-licenses.py" "$work/atvvoice-metadata.json" "$root/LICENSES/Rust/ATVVoice"
cargo metadata --locked --offline --filter-platform x86_64-unknown-linux-gnu --format-version 1 --manifest-path "$handy/source/src-tauri/Cargo.toml" >"$work/handy-metadata.json"; python3 "$HERE/Linux/release/rust-licenses.py" "$work/handy-metadata.json" "$root/LICENSES/Rust/Handy"
python3 "$HERE/Linux/release/js-licenses.py" "$handy/source/node_modules" "$root/LICENSES/JavaScript/Handy"
[[ $(sha256sum "$handy/target/release/resources/models/silero_vad_v4.onnx"|cut -d' ' -f1) == a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28 ]]||fail 'unexpected Silero VAD model'
install -m0755 "$atv/target/release/atvvoice" "$root/payload/bin/omavoice-atvvoice"
for n in omavoice-doctor omavoice-settings omavoice-statistics omavoice-keyd-helper; do [[ -x $HERE/Linux/OmaVoiceLinux/target/release/$n ]]||fail "missing release binary: $n"; done
for n in omavoice-doctor omavoice-settings omavoice-statistics; do install -m0755 "$HERE/Linux/OmaVoiceLinux/target/release/$n" "$root/payload/bin/$n"; done
install -m0755 "$HERE/Linux/omavoicectl" "$root/payload/bin/omavoicectl"; install -m0755 "$handy/target/release/handy" "$root/payload/bin/handy"; install -m0755 "$HERE/Linux/Handy/omavoice-handy" "$root/payload/bin/omavoice-handy"
cp -a "$handy/source/src-tauri/transcribe-libs/." "$root/payload/lib/Handy/"; mkdir "$root/payload/lib/Handy/resources"; cp -a "$handy/target/release/resources/." "$root/payload/lib/Handy/resources/"; printf '%s\n' "$H_COMMIT" >"$root/payload/lib/Handy/.omavoice-managed"; install -m0644 "$handy/BUILD-METADATA" "$root/payload/lib/Handy/BUILD-METADATA"; install -m0644 "$handy/SHA256SUMS" "$root/payload/lib/Handy/BUILD-SHA256SUMS"
install -m0644 "$HERE"/Linux/systemd/*.service "$HERE/Linux/Handy/omavoice-handy.service" "$root/payload/systemd/user/"; install -m0644 "$HERE/Linux/app.omavoice.Settings.desktop" "$HERE/Linux/Handy/com.pais.handy.desktop" "$root/payload/share/applications/"
install -m0644 "$HERE/Resources/OmaVoice.png" "$root/payload/share/icons/hicolor/1024x1024/apps/app.omavoice.Settings.png"; install -m0644 "$handy/source/src-tauri/icons/128x128@2x.png" "$root/payload/share/icons/hicolor/256x256/apps/handy.png"; install -m0644 "$handy/source/LICENSE" "$root/payload/share/licenses/Handy/LICENSE"
install -m0755 "$HERE/Linux/release/keyd-install.sh" "$root/optional-keyd/install.sh"; install -m0755 "$HERE/Linux/release/keyd-uninstall.sh" "$root/optional-keyd/uninstall.sh"; install -m0755 "$HERE/Linux/OmaVoiceLinux/target/release/omavoice-keyd-helper" "$root/optional-keyd/"; install -m0644 "$HERE/Linux/polkit/app.omavoice.keyd.policy" "$root/optional-keyd/"
epoch=$(git -C "$HERE" show -s --format=%ct "$commit"); [[ $epoch =~ ^[0-9]+$ ]]||fail 'invalid commit timestamp'
{ echo "VERSION=$version"; echo "COMMIT=$commit"; echo "SOURCE_DATE_EPOCH=$epoch"; echo "BUILD_DATE=$(date -u -d "@$epoch" +%FT%TZ)"; echo "KERNEL=$(uname -sr)"; echo "RUSTC=$(rustc --version)"; echo "CARGO=$(cargo --version)"; echo "ATVVOICE_BASE=$ATV_BASE"; echo "ATVVOICE_TREE=$ATV_TREE"; sed 's/^/ATVVOICE_PATCH_SHA256=/' "$HERE/Linux/ATVVoice/SHA256SUMS"; cat "$handy/BUILD-METADATA"; command -v pacman >/dev/null&&pacman -Q gcc glibc pipewire gtk4 libadwaita webkit2gtk-4.1 zstd 2>/dev/null|sed 's/^/PACMAN=/'||:; } >"$root/BUILD-METADATA"
find "$root" -type d -exec chmod 0755 {} +; find "$root" -type f ! -perm -0100 -exec chmod 0644 {} +
bad=$(find "$root" \( -type l -o ! -type d -a ! -type f -o -perm /6000 -o -perm -0002 \) -print -quit); [[ -z $bad ]]||fail "unsafe archive entry: $bad"
for b in "$root"/payload/bin/omavoice-atvvoice "$root"/payload/bin/omavoice-{doctor,settings,statistics} "$root/payload/bin/handy" "$root/optional-keyd/omavoice-keyd-helper"; do file "$b"|grep -q 'ELF 64-bit LSB.*x86-64'||fail "not x86_64 ELF: $b"; for leaked in "$HERE" "$atv" "$handy"; do strings "$b"|grep -Fq "$leaked"&&fail "build path embedded: $b"; done; done
for b in "$root"/payload/bin/omavoice-atvvoice "$root"/payload/bin/omavoice-{doctor,settings,statistics} "$root/optional-keyd/omavoice-keyd-helper"; do readelf -d "$b"|grep -Eq 'RPATH|RUNPATH'&&fail "RPATH present: $b"; done
handy_runpath=$(readelf -d "$root/payload/bin/handy"|sed -n 's/.*(RUNPATH).*\[\(.*\)\]/\1/p'); [[ $handy_runpath == '$ORIGIN/../lib/Handy:$ORIGIN/../lib' ]]||fail "unexpected Handy RUNPATH: $handy_runpath"
LD_LIBRARY_PATH="$root/payload/lib/Handy" ldd "$root/payload/bin/handy"|grep -q 'not found'&&fail 'Handy has missing runtime dependencies'; for b in "$root"/payload/bin/omavoice-atvvoice "$root"/payload/bin/omavoice-{doctor,settings,statistics}; do ldd "$b"|grep -q 'not found'&&fail "missing runtime dependency: $b"; done
desktop-file-validate "$root"/payload/share/applications/*; verify_home="$work/systemd-home"; mkdir -p "$verify_home/.local/bin"; for n in omavoice-atvvoice omavoice-settings omavoice-statistics omavoice-handy; do ln -s /usr/bin/true "$verify_home/.local/bin/$n"; done; HOME="$verify_home" systemd-analyze verify --user "$root"/payload/systemd/user/*.service; bash -n "$root"/{install.sh,uninstall.sh} "$root"/optional-keyd/{install.sh,uninstall.sh}
python3 "$HERE/Linux/release/check-markdown-links.py" "$root"
(cd "$root"; find . -type f ! -name PAYLOAD-SHA256SUMS -printf '%P\0'|sort -z|xargs -0 sha256sum >PAYLOAD-SHA256SUMS)
"$HERE/Linux/release/test-lifecycle.sh" "$root"
archive="OmaVoice-v$version-omarchy-arch-x86_64.tar.zst"; tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner --pax-option=delete=atime,delete=ctime -C "$work" -cf - "${root##*/}"|zstd -19 -T1 -q -o "$out/$archive"; (cd "$out"&&sha256sum "$archive" >SHA256SUMS)
echo "Created $out/$archive"
