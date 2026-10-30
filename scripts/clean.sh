#!/usr/bin/env bash
# 清理构建产物，控制磁盘占用。
#   bash scripts/clean.sh          轻量清理：增量缓存、前端产物、测试截图（保留已编译依赖，下次构建仍快）
#   bash scripts/clean.sh --all    完全清理：删除整个 src-tauri/target（下次构建需重新编译依赖）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="$ROOT/src-tauri/target"

size_of() {
  if [[ -e "$1" ]]; then du -sh "$1" 2>/dev/null | cut -f1; else printf '0B'; fi
}

printf '清理前：target %s · dist %s\n' "$(size_of "$TARGET")" "$(size_of "$ROOT/dist")"

rm -rf "$ROOT/dist" "$ROOT/test-results" "$ROOT/playwright-report" "$ROOT/.cache/imagegen"

if [[ "${1:-}" == "--all" ]]; then
  rm -rf "$TARGET"
else
  # 增量编译缓存与打包产物最占空间，删除后不影响已编译的依赖。
  for profile in debug release; do
    rm -rf "$TARGET/$profile/incremental" "$TARGET/$profile/bundle" "$TARGET/$profile/.fingerprint/canonweave-"*
    rm -f "$TARGET/$profile/deps/canonweave"* "$TARGET/$profile/deps/libcanonweave"*
  done
fi

printf '清理后：target %s\n' "$(size_of "$TARGET")"
