#!/usr/bin/env bash
set -euo pipefail

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

stage=
while (($#)); do
  case $1 in
    --staging-root)
      (($# > 1)) || die 'missing --staging-root value' '缺少 --staging-root 参数'
      stage=$2
      shift 2
      ;;
    -h|--help)
      if zh; then
        echo '用法：uninstall.sh [--staging-root 绝对路径]'
      else
        echo 'Usage: uninstall.sh [--staging-root ABSOLUTE]'
      fi
      exit
      ;;
    *)
      die "unknown option: $1" "未知选项：$1"
      ;;
  esac
done

((EUID != 0)) || die 'do not run as root' '请勿以 root 运行'
for command_name in sha256sum find sort stat cut realpath tail rm rmdir; do
  command -v "$command_name" >/dev/null ||
    die "required command missing: $command_name" "缺少命令：$command_name"
done
if [[ -n $stage ]]; then
  [[ $stage == /* && $stage != / ]] || die 'invalid staging root' '无效暂存根'
  mkdir -p "$stage"
  stage=$(realpath "$stage")
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

destination() {
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
  bin/sayall-atvvoice bin/sayall-doctor bin/sayall-settings bin/sayall-statistics
  bin/sayallctl bin/handy bin/sayall-handy
  unit/sayall-atvvoice.service unit/sayall-settings.service
  unit/sayall-statistics.service unit/sayall-handy.service
  application/app.sayall.Settings.desktop application/com.pais.handy.desktop
  icon/app.sayall.Settings.png icon/handy.png license/Handy/LICENSE data/omavoice/uninstall.sh
)
manifest_keys=("${file_keys[@]}" lib/Handy)
targets=()
add_target() {
  destination "$1"
  targets+=("$destination_result")
}
add_target "$home/.local/bin/sayall-atvvoice"
add_target "$home/.local/bin/sayall-doctor"
add_target "$home/.local/bin/sayall-settings"
add_target "$home/.local/bin/sayall-statistics"
add_target "$home/.local/bin/sayallctl"
add_target "$home/.local/bin/handy"
add_target "$home/.local/bin/sayall-handy"
add_target "$config/systemd/user/sayall-atvvoice.service"
add_target "$config/systemd/user/sayall-settings.service"
add_target "$config/systemd/user/sayall-statistics.service"
add_target "$config/systemd/user/sayall-handy.service"
add_target "$data/applications/app.sayall.Settings.desktop"
add_target "$data/applications/com.pais.handy.desktop"
add_target "$data/icons/hicolor/1024x1024/apps/app.sayall.Settings.png"
add_target "$data/icons/hicolor/256x256/apps/handy.png"
add_target "$data/licenses/Handy/LICENSE"
add_target "$data/omavoice/uninstall.sh"

destination "$data/omavoice/install-manifest-v1.tsv"
manifest=$destination_result
if [[ ! -f $manifest || -L $manifest ]]; then
  if zh; then
    echo '未找到可信安装清单；没有删除任何程序或用户数据。'
  else
    echo 'No trusted install manifest was found; no program or user data was removed.'
  fi
  exit 0
fi

IFS= read -r header <"$manifest"
[[ $header == OMAVOICE_INSTALL_MANIFEST_V1 ]] ||
  die 'unsupported install manifest' '安装清单版本不受支持'
declare -A expected=()
while IFS=$'\t' read -r hash key extra; do
  [[ -z $extra && $hash =~ ^[0-9a-f]{64}$ ]] || die 'invalid install manifest' '安装清单无效'
  valid=0
  for expected_key in "${manifest_keys[@]}"; do
    [[ $key == "$expected_key" ]] && valid=1
  done
  ((valid == 1)) || die 'invalid path in install manifest' '安装清单含无效路径'
  [[ -z ${expected[$key]+x} ]] || die 'duplicate install manifest entry' '安装清单含重复项目'
  expected[$key]=$hash
done < <(tail -n +2 "$manifest")
((${#expected[@]} == ${#manifest_keys[@]})) || die 'install manifest is incomplete' '安装清单不完整'

units=(sayall-atvvoice.service sayall-settings.service sayall-statistics.service sayall-handy.service)
if [[ -z $stage ]] && command -v systemctl >/dev/null; then
  systemctl --user disable --now "${units[@]}" 2>/dev/null || :
fi

preserved=0
for index in "${!file_keys[@]}"; do
  key=${file_keys[$index]}
  target=${targets[$index]}
  [[ -n ${expected[$key]+x} ]] || continue
  [[ -e $target || -L $target ]] || continue
  if [[ -f $target && ! -L $target &&
    $(sha256sum "$target" | cut -d' ' -f1) == "${expected[$key]}" ]]; then
    rm -f "$target"
  else
    preserved=1
    if zh; then
      printf '保留已修改或非普通文件：%s\n' "$target" >&2
    else
      printf 'Preserved modified or non-regular file: %s\n' "$target" >&2
    fi
  fi
done

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

destination "$home/.local/lib/Handy"
lib=$destination_result
if [[ -e $lib || -L $lib ]]; then
  if [[ ! -L $lib && -f $lib/.sayall-managed ]] &&
    grep -Fqx "$HANDY_COMMIT" "$lib/.sayall-managed" &&
    [[ $(tree_hash "$lib") == "${expected[lib/Handy]}" ]]; then
    rm -rf "$lib"
  else
    preserved=1
    if zh; then
      printf '保留已修改或非托管 Handy 库：%s\n' "$lib" >&2
    else
      printf 'Preserved modified or unmanaged Handy library: %s\n' "$lib" >&2
    fi
  fi
fi

rm -f "$manifest"
rmdir "$(dirname "$manifest")" 2>/dev/null || :
if [[ -z $stage ]] && command -v systemctl >/dev/null; then
  systemctl --user daemon-reload 2>/dev/null || :
fi

if zh; then
  ((preserved == 0)) && echo '程序已卸载；所有用户数据均已保留。' ||
    echo '程序已卸载；用户数据和本地修改均已保留。'
else
  ((preserved == 0)) && echo 'Programs removed; all user data was preserved.' ||
    echo 'Programs removed; user data and local modifications were preserved.'
fi
