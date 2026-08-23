#!/usr/bin/env bash

set -euo pipefail

readonly EXPECTED_ATVVOICE_TREE="df607e5c9609673fef683de1c02a3411b1acbd5d"

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
readonly REPOSITORY_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

staging_root=""
atvvoice_build_directory=""
enable_services=true
temporary_atvvoice_build=""

locale_is_zh() {
    local locale
    for locale in "${LANGUAGE:-}" "${LC_ALL:-}" "${LC_MESSAGES:-}" "${LANG:-}"; do
        [[ -n "$locale" ]] || continue
        [[ "$locale" == zh* ]]
        return
    done
    return 1
}

usage() {
    if locale_is_zh; then cat <<'EOF'
用法：bash Linux/install-user.sh [选项]

构建并安装OmaVoice Linux 用户态运行时；不使用 sudo，不修改 /etc。

选项：
  --staging-root <目录>              只安装到隔离根目录，不调用 systemctl
  --atvvoice-build-directory <目录>  复用已验证的 ATVVoice 构建输出
  --no-enable                       安装文件，但不启用或启动用户服务
  -h, --help                        显示帮助
EOF
    else cat <<'EOF'
Usage: bash Linux/install-user.sh [options]

Build and install the OmaVoice Linux user runtime without sudo or changes to /etc.

Options:
  --staging-root <directory>             Install under an isolated root; do not run systemctl
  --atvvoice-build-directory <directory> Reuse verified ATVVoice build output
  --no-enable                            Install files without enabling or starting services
  -h, --help                             Show help
EOF
    fi
}

fail() {
    if locale_is_zh; then printf '错误：%s\n' "${2:-$1}" >&2; else printf 'Error: %s\n' "$1" >&2; fi
    exit 1
}

cleanup() {
    if [[ -n "$temporary_atvvoice_build" ]]; then
        rm -rf -- "$temporary_atvvoice_build"
    fi
}
trap cleanup EXIT

while (( $# > 0 )); do
    case "$1" in
        --staging-root)
            (( $# >= 2 )) || fail "--staging-root requires a directory" "--staging-root 需要一个目录"
            staging_root="$2"
            shift 2
            ;;
        --atvvoice-build-directory)
            (( $# >= 2 )) || fail "--atvvoice-build-directory requires a directory" "--atvvoice-build-directory 需要一个目录"
            atvvoice_build_directory="$2"
            shift 2
            ;;
        --no-enable)
            enable_services=false
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unrecognized argument: $1" "无法识别的参数“$1”"
            ;;
    esac
done

(( EUID != 0 )) || fail "run as a regular user, without sudo" "请以普通用户运行，不要使用 sudo"

for command_name in cargo git install mktemp mv; do
    command -v "$command_name" >/dev/null 2>&1 || \
        fail "required command missing: $command_name" "缺少命令：$command_name"
done

home_directory="${HOME:-}"
[[ "$home_directory" == /* ]] || fail "HOME must be an absolute path" "HOME 必须是绝对路径"

config_home="${XDG_CONFIG_HOME:-$home_directory/.config}"
data_home="${XDG_DATA_HOME:-$home_directory/.local/share}"
[[ "$config_home" == /* ]] || fail "XDG_CONFIG_HOME must be an absolute path" "XDG_CONFIG_HOME 必须是绝对路径"
[[ "$data_home" == /* ]] || fail "XDG_DATA_HOME must be an absolute path" "XDG_DATA_HOME 必须是绝对路径"

if [[ -n "$staging_root" ]]; then
    [[ "$staging_root" == /* ]] || fail "--staging-root must be an absolute path" "--staging-root 必须是绝对路径"
    mkdir -p -- "$staging_root"
    staging_root="$(cd -- "$staging_root" && pwd -P)"
    [[ "$staging_root" != "/" ]] || fail "--staging-root cannot be the system root" "--staging-root 不得为系统根目录"
    enable_services=false
fi

process_pids_by_argv0() {
    local expected_name="$1"
    local process_path
    local argv0

    for process_path in /proc/[0-9]*; do
        argv0=""
        IFS= read -r -d '' argv0 2>/dev/null < "$process_path/cmdline" || true
        if [[ "${argv0##*/}" == "$expected_name" ]]; then
            printf '%s\n' "${process_path##*/}"
        fi
    done
}

if [[ "$enable_services" == true ]]; then
    command -v systemctl >/dev/null 2>&1 || fail "required command missing: systemctl" "缺少命令：systemctl"
    while IFS= read -r _; do
        fail \
            "a manually started atvvoice process is running; stop the PoC before installing" \
            "检测到手动启动的 atvvoice；请先正常停止 PoC，再重新运行安装"
    done < <(process_pids_by_argv0 "atvvoice")

    managed_atvvoice_pid="$(systemctl --user show --property MainPID --value omavoice-atvvoice.service 2>/dev/null || true)"
    while IFS= read -r pid; do
        if [[ -z "$managed_atvvoice_pid" || "$managed_atvvoice_pid" == "0" || "$pid" != "$managed_atvvoice_pid" ]]; then
            fail \
                "an unmanaged omavoice-atvvoice process is running; stop it before installing" \
                "检测到不受用户服务管理的 omavoice-atvvoice；请先停止该进程"
        fi
    done < <(process_pids_by_argv0 "omavoice-atvvoice")

    managed_settings_pid="$(systemctl --user show --property MainPID --value omavoice-settings.service 2>/dev/null || true)"
    while IFS= read -r pid; do
        if [[ -z "$managed_settings_pid" || "$managed_settings_pid" == "0" || "$pid" != "$managed_settings_pid" ]]; then
            fail \
                "an unmanaged omavoice-settings process is running; quit it before installing" \
                "检测到不受用户服务管理的 omavoice-settings；请先退出该进程"
        fi
    done < <(process_pids_by_argv0 "omavoice-settings")

    managed_statistics_pid="$(systemctl --user show --property MainPID --value omavoice-statistics.service 2>/dev/null || true)"
    while IFS= read -r pid; do
        if [[ -z "$managed_statistics_pid" || "$managed_statistics_pid" == "0" || "$pid" != "$managed_statistics_pid" ]]; then
            fail \
                "an unmanaged omavoice-statistics process is running; stop it before installing" \
                "检测到不受用户服务管理的 omavoice-statistics；请先停止该进程"
        fi
    done < <(process_pids_by_argv0 "omavoice-statistics")
fi

if [[ -z "$atvvoice_build_directory" ]]; then
    temporary_atvvoice_build="$(mktemp -d -t omavoice-atvvoice-install.XXXXXX)"
    atvvoice_build_directory="$temporary_atvvoice_build"
    bash "$SCRIPT_DIR/ATVVoice/build-patched.sh" "$atvvoice_build_directory"
fi

[[ -d "$atvvoice_build_directory/source/.git" ]] || \
    fail "ATVVoice build directory has no source Git checkout" "ATVVoice 构建目录缺少 source Git checkout"
[[ -x "$atvvoice_build_directory/target/release/atvvoice" ]] || \
    fail "ATVVoice build directory has no release binary" "ATVVoice 构建目录缺少 release 二进制"
actual_tree="$(git -C "$atvvoice_build_directory/source" rev-parse 'HEAD^{tree}')"
[[ "$actual_tree" == "$EXPECTED_ATVVOICE_TREE" ]] || \
    fail "ATVVoice source tree does not match the pinned candidate" "ATVVoice 源码 tree 不匹配固定候选"

readonly omavoice_target_directory="$SCRIPT_DIR/OmaVoiceLinux/target"
if locale_is_zh; then
    printf '构建 OmaVoice Linux release 二进制...\n'
else
    printf 'Building OmaVoice Linux release binaries...\n'
fi
cargo build \
    --manifest-path "$SCRIPT_DIR/OmaVoiceLinux/Cargo.toml" \
    --locked \
    --release \
    --bins \
    --target-dir "$omavoice_target_directory"

destination() {
    printf '%s%s' "$staging_root" "$1"
}

install_file() {
    local source="$1"
    local target="$2"
    local mode="$3"
    local directory
    local temporary
    directory="$(dirname -- "$target")"
    install -d -m 0755 -- "$directory"
    temporary="$(mktemp --tmpdir="$directory" ".$(basename -- "$target").tmp.XXXXXX")"
    if ! install -m "$mode" -- "$source" "$temporary"; then
        rm -f -- "$temporary"
        return 1
    fi
    mv -f -- "$temporary" "$target"
}

bin_directory="$(destination "$home_directory/.local/bin")"
unit_directory="$(destination "$config_home/systemd/user")"
application_directory="$(destination "$data_home/applications")"
icon_directory="$(destination "$data_home/icons/hicolor/1024x1024/apps")"

install_file "$atvvoice_build_directory/target/release/atvvoice" "$bin_directory/omavoice-atvvoice" 0755
install_file "$omavoice_target_directory/release/omavoice-doctor" "$bin_directory/omavoice-doctor" 0755
install_file "$omavoice_target_directory/release/omavoice-settings" "$bin_directory/omavoice-settings" 0755
install_file "$omavoice_target_directory/release/omavoice-statistics" "$bin_directory/omavoice-statistics" 0755
install_file "$SCRIPT_DIR/omavoicectl" "$bin_directory/omavoicectl" 0755
install_file "$SCRIPT_DIR/systemd/omavoice-atvvoice.service" "$unit_directory/omavoice-atvvoice.service" 0644
install_file "$SCRIPT_DIR/systemd/omavoice-settings.service" "$unit_directory/omavoice-settings.service" 0644
install_file "$SCRIPT_DIR/systemd/omavoice-statistics.service" "$unit_directory/omavoice-statistics.service" 0644
install_file "$SCRIPT_DIR/app.omavoice.Settings.desktop" "$application_directory/app.omavoice.Settings.desktop" 0644
install_file "$REPOSITORY_ROOT/Resources/OmaVoice.png" "$icon_directory/app.omavoice.Settings.png" 0644
install -d -m 0700 -- "$(destination "$data_home/omavoice")"

if [[ -n "$staging_root" ]]; then
    if locale_is_zh; then
        printf '隔离安装完成：%s\n' "$staging_root"
        printf '未调用 systemctl，未修改当前用户服务状态。\n'
    else
        printf 'Staged installation complete: %s\n' "$staging_root"
        printf 'systemctl was not called and current user services were not changed.\n'
    fi
    exit 0
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$application_directory"
fi

if [[ "$enable_services" == false ]]; then
    if locale_is_zh; then
        printf '用户态文件安装完成，尚未启用服务。\n'
        printf '确认后运行：omavoicectl start\n'
    else
        printf 'User files are installed, but services are not enabled.\n'
        printf 'After review, run: omavoicectl start\n'
    fi
    exit 0
fi

systemctl --user daemon-reload
systemctl --user enable omavoice-atvvoice.service omavoice-settings.service omavoice-statistics.service
systemctl --user restart omavoice-atvvoice.service omavoice-settings.service omavoice-statistics.service

if locale_is_zh; then
    printf 'OmaVoice Linux 用户态运行时安装完成。\n'
else
    printf 'The OmaVoice Linux user runtime is installed.\n'
fi
systemctl --user --no-pager --full status omavoice-atvvoice.service omavoice-settings.service omavoice-statistics.service || true
