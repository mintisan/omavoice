#!/usr/bin/env bash
set -euo pipefail

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly HELPER_SOURCE="$SCRIPT_DIR/SayAllLinux/target/release/sayall-keyd-helper"
readonly HELPER_TARGET="/usr/lib/sayall/sayall-keyd-helper"
readonly POLICY_SOURCE="$SCRIPT_DIR/polkit/app.sayall.keyd.policy"
readonly POLICY_TARGET="/usr/share/polkit-1/actions/app.sayall.keyd.policy"

locale_is_zh() {
    local locale
    for locale in "${LANGUAGE:-}" "${LC_ALL:-}" "${LC_MESSAGES:-}" "${LANG:-}"; do
        [[ -n "$locale" ]] || continue
        [[ "$locale" == zh* ]]
        return
    done
    return 1
}

fail() {
    if locale_is_zh; then printf '错误：%s\n' "${2:-$1}" >&2; else printf 'Error: %s\n' "$1" >&2; fi
    exit 1
}

(( EUID != 0 )) || fail "run as a regular user; only fixed install commands request PolicyKit authorization" "请以普通用户运行；脚本只让固定的 install 命令请求 PolicyKit 授权"
command -v cargo >/dev/null 2>&1 || fail "cargo is required" "缺少 cargo"
[[ -x /usr/bin/pkexec ]] || fail "/usr/bin/pkexec is required" "缺少 /usr/bin/pkexec"
[[ -x /usr/bin/install ]] || fail "/usr/bin/install is required" "缺少 /usr/bin/install"
[[ -f "$POLICY_SOURCE" ]] || fail "PolicyKit policy is missing" "缺少 PolicyKit policy"

cargo build \
    --manifest-path "$SCRIPT_DIR/SayAllLinux/Cargo.toml" \
    --locked \
    --release \
    --bin sayall-keyd-helper

/usr/bin/pkexec /usr/bin/install -D -m 0755 -- "$HELPER_SOURCE" "$HELPER_TARGET"
/usr/bin/pkexec /usr/bin/install -D -m 0644 -- "$POLICY_SOURCE" "$POLICY_TARGET"

cmp -s -- "$HELPER_SOURCE" "$HELPER_TARGET" || \
    fail "the installed helper differs from the release build" "系统 helper 与 release 构建不一致"
cmp -s -- "$POLICY_SOURCE" "$POLICY_TARGET" || \
    fail "the installed PolicyKit policy differs from the repository" "系统 PolicyKit policy 与仓库版本不一致"

if locale_is_zh; then printf 'OmaVoice keyd 系统组件已安装。\n'; else printf 'The OmaVoice keyd system component was installed.\n'; fi
