#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
HANDY_COMMIT=9bcb6d9d46c88517d2b5519d3a4f900ee3968c99

zh() {
  [[ ${LANGUAGE:-${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}} == zh* ]]
}

die() {
  if zh; then
    printf '错误：%s\n' "${2:-$1}" >&2
  else
    printf 'Error: %s\n' "$1" >&2
  fi
  exit 1
}

usage() {
  if zh; then
    cat <<'EOF'
用法：install.sh [--staging-root 绝对路径] [--no-enable]
为当前用户安装预编译 OmaVoice；普通安装不需要 root 权限。
EOF
  else
    cat <<'EOF'
Usage: install.sh [--staging-root ABSOLUTE] [--no-enable]
Install prebuilt OmaVoice for the current user without root privileges.
EOF
  fi
}

stage=
enable=1
while (($#)); do
  case $1 in
    --staging-root)
      (($# > 1)) || die 'missing --staging-root value' '缺少 --staging-root 参数'
      stage=$2
      shift 2
      ;;
    --no-enable)
      enable=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1" "未知选项：$1"
      ;;
  esac
done

[[ $(uname -m) == x86_64 ]] || die 'x86_64 is required' '需要 x86_64 架构'
((EUID != 0)) || die 'do not run as root' '请勿以 root 运行'

for command_name in sha256sum find sort stat cut install ldd mktemp uname realpath file head cp mv rm mkdir; do
  command -v "$command_name" >/dev/null ||
    die "required command missing: $command_name" "缺少命令：$command_name"
done

if [[ -n $stage ]]; then
  [[ $stage == /* && $stage != / ]] ||
    die 'staging root must be an absolute non-root path' '暂存根必须是非根绝对路径'
  mkdir -p "$stage"
  stage=$(realpath "$stage")
  enable=0
fi

[[ -f $ROOT/PAYLOAD-SHA256SUMS ]] || die 'checksum manifest missing' '缺少校验清单'
bad=$(find "$ROOT/payload" "$ROOT/optional-keyd" -mindepth 1 \
  \( -type l -o ! -type f -a ! -type d \) -print -quit)
[[ -z $bad ]] || die "unsafe package entry: $bad" "不安全的包项目：$bad"
(cd "$ROOT" && sha256sum --quiet -c PAYLOAD-SHA256SUMS) ||
  die 'package checksum verification failed' '包校验失败'

for binary in "$ROOT"/payload/bin/*; do
  if [[ $(head -c4 "$binary") == $'\177ELF' ]]; then
    file "$binary" | grep -q 'ELF 64-bit LSB.*x86-64' ||
      die "invalid executable: $binary" "无效可执行文件：$binary"
  fi
done

export LD_LIBRARY_PATH="$ROOT/payload/lib/Handy${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
for binary in "$ROOT"/payload/bin/*; do
  if [[ $(head -c4 "$binary") == $'\177ELF' ]] && ldd "$binary" 2>/dev/null | grep -q 'not found'; then
    ldd "$binary" 2>/dev/null | grep 'not found' >&2 || :
    die "runtime dependency missing for $binary" "运行库缺失：$binary"
  fi
done

missing_commands=()
for command_name in bluetoothctl keyd wl-copy wl-paste wpctl wtype; do
  command -v "$command_name" >/dev/null || missing_commands+=("$command_name")
done
if ((${#missing_commands[@]})); then
  missing_list=$(IFS=', '; echo "${missing_commands[*]}")
  die "required runtime commands are missing: $missing_list; install the packages listed in README.md" \
    "缺少运行命令：$missing_list；请安装 README.zh-CN.md 中列出的软件包"
fi

safe_absolute() {
  local value=$1
  [[ $value == /* && $value != *'/../'* && $value != */.. &&
    $value != *'/./'* && $value != */. && $value != *'//'* ]]
}

home=${HOME:?}
config=${XDG_CONFIG_HOME:-$home/.config}
data=${XDG_DATA_HOME:-$home/.local/share}
for value in "$home" "$config" "$data"; do
  safe_absolute "$value" || die 'HOME/XDG paths must be normalized absolute paths' \
    'HOME/XDG 路径必须是规范的绝对路径'
done

dst() {
  local logical=$1 resolved
  if [[ -z $stage ]]; then
    destination_result=$logical
    return
  fi
  resolved=$(realpath -m -- "$stage$logical")
  [[ $resolved == "$stage"/* ]] ||
    die 'a staging destination escapes the staging root' '暂存目标超出暂存根'
  destination_result=$resolved
}

file_keys=(
  bin/omavoice-atvvoice bin/omavoice-doctor bin/omavoice-settings bin/omavoice-statistics
  bin/omavoicectl bin/handy bin/omavoice-handy
  unit/omavoice-atvvoice.service unit/omavoice-settings.service
  unit/omavoice-statistics.service unit/omavoice-handy.service
  application/app.omavoice.Settings.desktop application/com.pais.handy.desktop
  icon/app.omavoice.Settings.png icon/handy.png license/Handy/LICENSE data/omavoice/uninstall.sh
)
manifest_keys=("${file_keys[@]}" lib/Handy)
sources=(
  "$ROOT/payload/bin/omavoice-atvvoice" "$ROOT/payload/bin/omavoice-doctor"
  "$ROOT/payload/bin/omavoice-settings" "$ROOT/payload/bin/omavoice-statistics"
  "$ROOT/payload/bin/omavoicectl" "$ROOT/payload/bin/handy"
  "$ROOT/payload/bin/omavoice-handy"
  "$ROOT/payload/systemd/user/omavoice-atvvoice.service"
  "$ROOT/payload/systemd/user/omavoice-settings.service"
  "$ROOT/payload/systemd/user/omavoice-statistics.service"
  "$ROOT/payload/systemd/user/omavoice-handy.service"
  "$ROOT/payload/share/applications/app.omavoice.Settings.desktop"
  "$ROOT/payload/share/applications/com.pais.handy.desktop"
  "$ROOT/payload/share/icons/hicolor/1024x1024/apps/app.omavoice.Settings.png"
  "$ROOT/payload/share/icons/hicolor/256x256/apps/handy.png"
  "$ROOT/payload/share/licenses/Handy/LICENSE"
  "$ROOT/uninstall.sh"
)
targets=()
add_target() {
  dst "$1"
  targets+=("$destination_result")
}
add_target "$home/.local/bin/omavoice-atvvoice"
add_target "$home/.local/bin/omavoice-doctor"
add_target "$home/.local/bin/omavoice-settings"
add_target "$home/.local/bin/omavoice-statistics"
add_target "$home/.local/bin/omavoicectl"
add_target "$home/.local/bin/handy"
add_target "$home/.local/bin/omavoice-handy"
add_target "$config/systemd/user/omavoice-atvvoice.service"
add_target "$config/systemd/user/omavoice-settings.service"
add_target "$config/systemd/user/omavoice-statistics.service"
add_target "$config/systemd/user/omavoice-handy.service"
add_target "$data/applications/app.omavoice.Settings.desktop"
add_target "$data/applications/com.pais.handy.desktop"
add_target "$data/icons/hicolor/1024x1024/apps/app.omavoice.Settings.png"
add_target "$data/icons/hicolor/256x256/apps/handy.png"
add_target "$data/licenses/Handy/LICENSE"
add_target "$data/omavoice/uninstall.sh"
modes=(0755 0755 0755 0755 0755 0755 0755 0644 0644 0644 0644 0644 0644 0644 0644 0644 0755)

dst "$home/.local/lib/Handy"
lib=$destination_result
dst "$data/omavoice/install-manifest-v1.tsv"
manifest=$destination_result
[[ ! -L $lib ]] || die 'refusing a symbolic-link Handy library' '拒绝使用符号链接 Handy 库'
[[ ! -e $lib || -f $lib/.omavoice-managed ]] ||
  die 'refusing to overwrite unmanaged Handy library' '拒绝覆盖非托管 Handy 库'
if [[ -f $lib/.omavoice-managed ]]; then
  grep -Fqx "$HANDY_COMMIT" "$lib/.omavoice-managed" ||
    die 'the managed Handy revision is unknown' 'Handy 托管版本未知'
fi
[[ ! -L $manifest ]] || die 'refusing a symbolic-link install manifest' '拒绝使用符号链接安装清单'

declare -A previous=()
if [[ -e $manifest ]]; then
  [[ -f $manifest ]] || die 'invalid existing install manifest' '已有安装清单无效'
  IFS= read -r header <"$manifest"
  [[ $header == OMAVOICE_INSTALL_MANIFEST_V1 ]] ||
    die 'unsupported existing install manifest' '已有安装清单版本不受支持'
  while IFS=$'\t' read -r hash key extra; do
    [[ -z $extra && $hash =~ ^[0-9a-f]{64}$ ]] ||
      die 'invalid existing install manifest' '已有安装清单无效'
    valid=0
    for expected_key in "${manifest_keys[@]}"; do
      [[ $key == "$expected_key" ]] && valid=1
    done
    ((valid == 1)) || die 'invalid path in existing install manifest' '已有安装清单含无效路径'
    [[ -z ${previous[$key]+x} ]] || die 'duplicate install manifest entry' '安装清单含重复项目'
    previous[$key]=$hash
  done < <(tail -n +2 "$manifest")
  ((${#previous[@]} == ${#manifest_keys[@]})) ||
    die 'existing install manifest is incomplete' '已有安装清单不完整'
fi

tree_hash() {
  local root=$1 path
  [[ -d $root && ! -L $root ]] || return 1
  (
    cd "$root"
    while IFS= read -r -d '' path; do
      printf '%s\0%s\0%s\0' "${path#./}" "$(stat -c '%a' "$path")" \
        "$(sha256sum "$path" | cut -d' ' -f1)"
    done < <(find . -type f -print0 | sort -z)
  ) | sha256sum | cut -d' ' -f1
}

if [[ -e $lib ]]; then
  [[ -n ${previous[lib/Handy]+x} ]] ||
    die 'refusing to overwrite an unmanaged Handy library' '拒绝覆盖非托管 Handy 库'
  [[ $(tree_hash "$lib") == "${previous[lib/Handy]}" ]] ||
    die 'refusing to overwrite a locally modified Handy library' '拒绝覆盖本地修改的 Handy 库'
fi
for index in "${!file_keys[@]}"; do
  key=${file_keys[$index]}
  target=${targets[$index]}
  [[ ! -L $target ]] || die "refusing a symbolic-link destination: $target" \
    "拒绝使用符号链接目标：$target"
  [[ ! -e $target ]] && continue
  [[ -f $target ]] || die "destination is not a regular file: $target" "目标不是普通文件：$target"
  if [[ -n ${previous[$key]+x} ]]; then
    [[ $(sha256sum "$target" | cut -d' ' -f1) == "${previous[$key]}" ]] ||
      die "refusing to overwrite a locally modified file: $target" "拒绝覆盖本地修改的文件：$target"
  else
    die "refusing to overwrite an unmanaged file: $target" "拒绝覆盖非托管文件：$target"
  fi
done

units=(omavoice-atvvoice.service omavoice-settings.service omavoice-statistics.service omavoice-handy.service)
if [[ -z $stage && $enable == 1 ]]; then
  command -v systemctl >/dev/null || die 'systemctl missing' '缺少 systemctl'
  for unit in "${units[@]}"; do
    systemctl --user stop "$unit" 2>/dev/null || :
  done
fi

copy_file() {
  local source=$1 target=$2 mode=$3 temporary
  mkdir -p "$(dirname "$target")"
  temporary=$(mktemp "$(dirname "$target")/.install.XXXXXX")
  install -m "$mode" "$source" "$temporary"
  mv -f "$temporary" "$target"
}

mkdir -p "$(dirname "$lib")"
temporary_library=$(mktemp -d "$(dirname "$lib")/.Handy.XXXXXX")
cp -a "$ROOT/payload/lib/Handy/." "$temporary_library/"
rm -rf "$lib"
mv "$temporary_library" "$lib"

for index in "${!file_keys[@]}"; do
  copy_file "${sources[$index]}" "${targets[$index]}" "${modes[$index]}"
done

mkdir -p "$(dirname "$manifest")"
temporary_manifest=$(mktemp "$(dirname "$manifest")/.manifest.XXXXXX")
{
  printf 'OMAVOICE_INSTALL_MANIFEST_V1\n'
  for index in "${!file_keys[@]}"; do
    printf '%s\t%s\n' "$(sha256sum "${targets[$index]}" | cut -d' ' -f1)" "${file_keys[$index]}"
  done
  printf '%s\tlib/Handy\n' "$(tree_hash "$lib")"
} >"$temporary_manifest"
chmod 0600 "$temporary_manifest"
mv -f "$temporary_manifest" "$manifest"

if [[ -z $stage && $enable == 1 ]]; then
  systemctl --user daemon-reload
  systemctl --user enable --now "${units[@]}"
fi

if zh; then
  echo '安装完成；用户设置、模型和历史记录均未更改。'
else
  echo 'Installed; user settings, models, and history were not changed.'
fi
