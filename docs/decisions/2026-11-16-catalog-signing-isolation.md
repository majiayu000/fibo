---
status: accepted
date: 2026-11-16
decision-makers: majiayu000
---

# 目录生成、密钥签名、写权限分别隔离

## Context

旧工作流虽然把编译和签名拆成 job，签名 job 仍执行刚编译的 fibo，并把私钥交给它。被依赖构建脚本影响的程序因此能读私钥，所在 job 也持有写令牌。用户明确要求「签名做隔离」。

## Decision

采用三个全新 runner：build 只有只读 token，编译后生成 `index.json`；sign 只有只读 token，用系统 Python 标准库包装格式、runner 的 OpenSSL 签 Ed25519，不执行构建产物或仓库脚本；publish 没有私钥和可执行构建产物，只复制签名后的两个数据文件，在 commit 前检查暂存路径仅有 `plugins/index.json` 和 `plugins/index.sig`。

公钥仍以同一提交的 core `OFFICIAL_KEYS` 为准。私钥 JSON 和 `index.sig` 格式不变，不换 key、不动已安装 App 的信任。签名程序写在受审查的 workflow 内，Python 使用 `-I`，防止 checkout/artifact 中的同名模块被加载。私钥只落在 0700 临时目录内，OpenSSL 子进程不继承私钥环境变量。

## Consequences

编译依赖仍能影响生成的目录数据，此隔离保护私钥和发布权限，并不证明目录语义正确。PR/CI 继续审查生成的目录。多一个短 job，签名和发布使用 Ubuntu，不再占 macOS runner。

## Implementation Plan

`.github/workflows/sign-catalog.yml`；删除不再被使用、把程序和 key 放一起的 `scripts/publish-catalog.sh`。Actions 保持完整提交 SHA，其他工作流同样固定已核对的现有版本。

## Verification

`actionlint`；`OPENSSL=/opt/homebrew/opt/openssl@3/bin/openssl python3 -I scripts/test-ci-workflows.py`。使用 RFC 8032 公开向量逐字比较签名和公钥，检查不可信公钥失败、仓库同名模块/下载程序不执行、额外暂存路径阻止发布。同一回放也在本机 Docker 的 `rust:1-bookworm` Linux/OpenSSL 环境通过。没有读取真实私钥或运行远程发布。

## Alternatives

只拆 step：同一 runner 里的恶意程序可以残留并读取后续密钥/token。新签名服务或新密钥格式：当前需求不需要，runner 已有的 OpenSSL 能签同一种格式。让签名 job 再运行 fibo 验签：重新把 key runner 暴露给构建产物，不采用。

## More Information

保留 [main 必须走 PR](2026-11-06-main-requires-ci.md) 的规则。
[OpenSSL Ed25519 签名文档](https://docs.openssl.org/3.0/man1/openssl-pkeyutl/)；密钥封装使用 RFC 8410 的 PKCS#8/SPKI，签名向量来自 RFC 8032 7.1。
