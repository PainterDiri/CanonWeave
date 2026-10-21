#!/usr/bin/env bash

set -euo pipefail

IDENTITY_NAME="${CANONWEAVE_SIGNING_IDENTITY:-CanonWeave Local Development}"

fail() {
  printf '\n错误：%s\n' "$1" >&2
  exit 1
}

[[ "$(uname -s)" == "Darwin" ]] || fail "此脚本只适用于 macOS。"
command -v openssl >/dev/null 2>&1 || fail "找不到 openssl。"
command -v security >/dev/null 2>&1 || fail "找不到 macOS security 工具。"

if security find-identity -v -p codesigning 2>/dev/null | grep -Fq "\"$IDENTITY_NAME\""; then
  printf '签名身份已经存在：%s\n' "$IDENTITY_NAME"
  exit 0
fi

login_keychain="$(security default-keychain -d user | tr -d '"')"
[[ -n "$login_keychain" ]] || fail "无法确定默认登录钥匙串。"

working_dir="$(mktemp -d "${TMPDIR:-/tmp}/canonweave-signing.XXXXXX")"
cleanup() {
  rm -rf "$working_dir"
}
trap cleanup EXIT
umask 077

openssl req -x509 -newkey rsa:2048 -sha256 -nodes -days 3650 \
  -subj "/CN=$IDENTITY_NAME/O=CanonWeave/C=CN" \
  -addext "basicConstraints=critical,CA:TRUE" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" \
  -keyout "$working_dir/key.pem" \
  -out "$working_dir/cert.pem" \
  >/dev/null 2>&1

p12_password="$(openssl rand -hex 24)"
openssl pkcs12 -export \
  -inkey "$working_dir/key.pem" \
  -in "$working_dir/cert.pem" \
  -name "$IDENTITY_NAME" \
  -passout "pass:$p12_password" \
  -out "$working_dir/identity.p12" \
  >/dev/null 2>&1

security import "$working_dir/identity.p12" \
  -k "$login_keychain" \
  -P "$p12_password" \
  -x \
  -T /usr/bin/codesign \
  >/dev/null

security add-trusted-cert \
  -r trustRoot \
  -p codeSign \
  -k "$login_keychain" \
  "$working_dir/cert.pem" \
  >/dev/null

printf '\n一次性授权 codesign 使用新私钥。\n'
printf 'macOS 将弹出系统密码框；密码不会经过本脚本。\n'
security set-key-partition-list \
  -S apple-tool:,apple:,codesign: \
  -s \
  "$login_keychain" \
  >/dev/null

security find-identity -v -p codesigning 2>/dev/null | grep -Fq "\"$IDENTITY_NAME\"" \
  || fail "签名身份创建后仍不可用，请打开“钥匙串访问”检查证书信任设置。"

printf '\n已创建本地签名身份：%s\n' "$IDENTITY_NAME"
printf '现在可运行：npm run dev:macos\n'
