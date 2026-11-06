#!/bin/sh
# Regenerates plugins/index.json, signs it and pushes both to the catalog-signed branch,
# which the workflow turns into a pull request. Only CI runs this: see
# .github/workflows/sign-catalog.yml. Usage: publish-catalog.sh <mooring binary> <key file>
set -eu
bin=$1
key=$2

# It commits in the checkout it is given.
if [ "${CI:-}" != "true" ]; then
    echo "publish-catalog.sh 只给 CI 用：它会直接提交工作区" >&2
    exit 1
fi

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
# Forced: a newer run replaces what an unmerged one signed.
git push -q --force origin HEAD:refs/heads/catalog-signed
echo "pushed=true" >>"${GITHUB_OUTPUT:-/dev/null}"
