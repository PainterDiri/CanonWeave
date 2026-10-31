#!/bin/sh
# cargo runner：启动应用前用稳定的 Apple Development 证书重新签名。
#
# macOS 钥匙串按“分区”放行：没有 Apple 团队 ID 的签名（ad-hoc 或自签名）按代码哈希分区，
# 每次重编译都会变，所以每次都要输密码；带团队 ID 的证书按 teamid 分区，重编译后“始终允许”仍然有效。
# 证书可在 Xcode → Settings → Accounts 里用 Apple ID（免费个人团队即可）创建。
# 也可以用 CANONWEAVE_SIGNING_IDENTITY 指定证书名称；找不到证书时原样启动。
if [ "$(uname)" = "Darwin" ] && [ "$(basename "$1")" = "canonweave" ]; then
  identity="${CANONWEAVE_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null |
    sed -n 's/.*"\(Apple Development: [^"]*\)".*/\1/p' | head -n 1)}"
  if [ -n "$identity" ]; then
    codesign --force --sign "$identity" --identifier com.canonweave.app "$1" >/dev/null 2>&1 ||
      echo "dev-run: 用「$identity」签名失败，按原样启动" >&2
  fi
fi
exec "$@"
