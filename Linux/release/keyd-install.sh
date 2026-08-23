#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)

zh() {
  local locale
  for locale in "${LANGUAGE:-}" "${LC_ALL:-}" "${LC_MESSAGES:-}" "${LANG:-}"; do
    [[ -n $locale ]] || continue
    [[ $locale == zh* ]]
    return
  done
  return 1
}

fail() {
  if zh; then
    printf '错误：%s\n' "${2:-$1}" >&2
  else
    printf 'Error: %s\n' "$1" >&2
  fi
  exit 1
}

((EUID != 0)) || fail 'run as a regular user' '请以普通用户运行'
if (($#)); then
  if [[ $1 == --help && $# == 1 ]]; then
    if zh; then
      echo '用法：optional-keyd/install.sh'
    else
      echo 'Usage: optional-keyd/install.sh'
    fi
    exit 0
  fi
  fail 'unknown option' '未知选项'
fi

for executable in /usr/bin/pkexec /usr/bin/install; do
  [[ -x $executable ]] || fail "missing $executable" "缺少 $executable"
done
(cd "$ROOT" && sha256sum --quiet -c PAYLOAD-SHA256SUMS) ||
  fail 'package verification failed' '包校验失败'
/usr/bin/pkexec /usr/bin/install -D -m 0755 -- \
  "$ROOT/optional-keyd/sayall-keyd-helper" /usr/lib/sayall/sayall-keyd-helper
/usr/bin/pkexec /usr/bin/install -D -m 0644 -- \
  "$ROOT/optional-keyd/app.sayall.keyd.policy" \
  /usr/share/polkit-1/actions/app.sayall.keyd.policy
cmp -s "$ROOT/optional-keyd/sayall-keyd-helper" /usr/lib/sayall/sayall-keyd-helper &&
  cmp -s "$ROOT/optional-keyd/app.sayall.keyd.policy" \
    /usr/share/polkit-1/actions/app.sayall.keyd.policy ||
  fail 'deployed bytes differ' '安装内容不一致'
if zh; then
  echo 'keyd 组件已安装；未修改 /etc/keyd。'
else
  echo 'keyd components installed; /etc/keyd was not changed.'
fi
