#!/bin/sh
# Regenerates plugins/index.json, signs it and pushes both to main. Only CI runs this: see
# .github/workflows/sign-catalog.yml. Usage: publish-catalog.sh <mooring binary> <key file>
set -eu
bin=$1
key=$2

# It resets the checkout between tries.
if [ "${CI:-}" != "true" ]; then
    echo "publish-catalog.sh 只给 CI 用：它会丢掉工作区的改动" >&2
    exit 1
fi

for try in 1 2 3; do
    "$bin" plugins index plugins --key "$key"
    # A valid signature from the wrong key would make every app refuse the catalog.
    said=$("$bin" plugins index plugins --check)
    echo "$said"
    case "$said" in
        *官方目录*) ;;
        *)
            echo "签名不是内置的官方密钥，没有发布" >&2
            exit 1
            ;;
    esac
    if git diff --quiet -- plugins/index.json plugins/index.sig; then
        echo "目录没有变化"
        exit 0
    fi
    git add plugins/index.json plugins/index.sig
    git commit -q -m "chore(plugins): CI 重新生成目录并签名"
    if git push -q origin HEAD:main; then
        exit 0
    fi
    # Someone pushed meanwhile: sign what is there now.
    echo "第 $try 次推送没成功，用最新的 main 重来" >&2
    git fetch -q origin main
    git reset -q --hard origin/main
done
echo "推不上去，放弃了" >&2
exit 1
