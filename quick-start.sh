#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

usage() {
  cat <<'EOF'
CanonWeave 快速启动

用法：
  ./quick-start.sh          启动桌面应用
  ./quick-start.sh --web    启动浏览器界面预览
  ./quick-start.sh --build  构建发行版
  ./quick-start.sh --install 只检查环境并安装依赖
  ./quick-start.sh --help   显示帮助
EOF
}

fail() {
  printf '\n错误：%s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "找不到 $1，请先安装 $2。"
}

printf '\nCanonWeave · 检查运行环境\n'
require_command node "Node.js 20.19+ 或 22.12+"
require_command npm "npm"
require_command cargo "Rust 1.77+"

node -e '
  const [major, minor] = process.versions.node.split(".").map(Number);
  const supported =
    (major === 20 && minor >= 19) ||
    (major === 22 && minor >= 12) ||
    major > 22;
  if (!supported) {
    console.error(`Node.js 版本过低：${process.versions.node}`);
    process.exit(1);
  }
' || fail "请升级到 Node.js 20.19+ 或 22.12+。"

if [[ "$(uname -s)" == "Darwin" ]]; then
  xcode-select -p >/dev/null 2>&1 || fail "缺少 Xcode Command Line Tools，请运行 xcode-select --install。"
fi

if [[ ! -f node_modules/.package-lock.json ]] || [[ package-lock.json -nt node_modules/.package-lock.json ]]; then
  printf '\n首次运行或依赖有更新，正在安装 npm 依赖…\n'
  npm install
fi

MODE="${1:-dev}"
case "$MODE" in
  --help|-h)
    usage
    ;;
  --install)
    printf '\n环境与依赖已就绪。\n'
    ;;
  --web)
    printf '\n启动浏览器预览。注意：项目数据功能需要在桌面应用中运行。\n'
    npm run dev
    ;;
  --build)
    printf '\n开始构建 CanonWeave 发行版…\n'
    npm run tauri -- build
    ;;
  dev|--dev)
    printf '\n启动 CanonWeave 桌面应用…\n'
    npm run tauri -- dev
    ;;
  *)
    usage
    fail "未知参数：$MODE"
    ;;
esac
