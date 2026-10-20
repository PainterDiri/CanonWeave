#!/bin/zsh

cd "$(dirname "$0")"
./quick-start.sh "$@"
status=$?

if [[ $status -ne 0 ]]; then
  echo
  echo "启动失败。请查看上方错误信息。"
  read -k 1 "?按任意键关闭窗口…"
fi

exit $status
