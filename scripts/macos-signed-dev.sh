#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IDENTITY_NAME="${CANONWEAVE_SIGNING_IDENTITY:-CanonWeave Local Development}"
APP_PATH="$ROOT/src-tauri/target/debug/bundle/macos/CanonWeave.app"

cd "$ROOT"

[[ "$(uname -s)" == "Darwin" ]] || {
  printf '错误：此脚本只适用于 macOS。\n' >&2
  exit 1
}

if ! security find-identity -v -p codesigning 2>/dev/null | grep -Fq "\"$IDENTITY_NAME\""; then
  printf '找不到稳定的代码签名身份：%s\n' "$IDENTITY_NAME" >&2
  printf '请先运行：npm run macos:setup-signing\n' >&2
  exit 1
fi

npm run tauri -- build --debug --bundles app --no-sign

codesign \
  --force \
  --deep \
  --identifier com.canonweave.app \
  --sign "$IDENTITY_NAME" \
  "$APP_PATH"

codesign --verify --deep --strict --verbose=2 "$APP_PATH"
printf '\n打开使用稳定签名的调试应用：%s\n' "$APP_PATH"
open "$APP_PATH"
