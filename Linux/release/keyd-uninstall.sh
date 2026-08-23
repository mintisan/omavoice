#!/usr/bin/env bash
set -euo pipefail

zh() {
  local locale
  for locale in "${LANGUAGE:-}" "${LC_ALL:-}" "${LC_MESSAGES:-}" "${LANG:-}"; do
    [[ -n $locale ]] || continue
    [[ $locale == zh* ]]
    return
  done
  return 1
}

if zh; then
  cat <<'EOF'
安全起见，本版本不授权宽泛的特权删除命令。管理员核对路径后可以准确运行：
  sudo rm -- /usr/lib/omavoice/omavoice-keyd-helper
  sudo rm -- /usr/share/polkit-1/actions/app.omavoice.keyd.policy
EOF
else
  cat <<'EOF'
Secure automatic removal is unavailable because this release deliberately does
not authorize a broad privileged delete command. After reviewing the paths, an
administrator may run exactly:
  sudo rm -- /usr/lib/omavoice/omavoice-keyd-helper
  sudo rm -- /usr/share/polkit-1/actions/app.omavoice.keyd.policy
EOF
fi
exit 1
