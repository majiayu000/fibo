---
status: proposed
date: 2026-11-14
decision-makers: majiayu000
---

# 窗口用本机那套协议直连别的电脑的后台，远程和本机走同一套代码

**要支撑的决定：** 另一台电脑上的会话怎么做到和本机「完全一致」：是继续给远程补一套界面，还是让窗口像连本机一样连它。

用户的话：「能不能两端都做到完全一致？……这个差异是来自于哪里呢？」「先试统一方案」「先出正式方案」。

## Context

**差异从哪来（2026-11-14 查过代码）**

| | 本机 | 远程（现在） |
|---|---|---|
| 连接 | 本机 `workbench.sock`，原生协议 | `ssh host fibo remote --stdio`，手机那套协议 |
| 拿到的数据 | 整个工作区：空间、标签、分屏布局、会话、待办 | 每个终端一张卡片：标题、状态、目录；**没有标签和布局** |
| 能发的请求 | 85 种（分屏、移窗格、改名、调比例……） | 看、打字、审批、开 / 关终端、标完成 |
| 窗口代码 | 读 `self.workspace` 436 处、`self.store` 66 处 | 另写的偷看、主区域、开始页、分屏（PR #340） |

终端画面本身已经是同一套：`Pane::remote` 和本机窗格用同一个终端组件，只是连接不同。

后果：远程每补一样本机功能就要再写一遍，接缝处出错。PR #340 一天里收到 Codex 的 11 条评论（另有 2 条在之后的提交上），全部是接缝问题：焦点落到被盖住的本机窗格、⌘W 关掉看不见的本机 shell（P1）、⌘[ ⌘] 走本机布局、待办表单挡住、机器列表变了开始页指错电脑。`docs/multi-machine/spec.md` 的放弃条件早写过：「App 要为本机和远程维护两套终端代码，而且差异越来越大 → 改成本机也走远程协议，只留一套。」

**原型（分支 `proto/attach-remote-host`，提交 `29c917e1`）**

- 在 agentos 上另起一个临时后台（`/tmp/fibo-proto`），`ssh -L` 把它的 socket 转到本机 `/tmp/fibo-attach/workbench.sock`，窗口加 `--attach --runtime /tmp/fibo-attach` 直连。
- `MOORING_ATTACH_EVIDENCE` 在挂接窗口里走 ⌘T、⌘D、⇧⌘D、⌘W、运行 Agent：**PASS**。agentos 后台里存下了这个标签和左右分屏，关掉的终端是 Exited，Codex v0.169 在 agentos 上跑起来了。
- 原生协议本来就是一个窗格一条连接，`ssh -L` 转发 Unix socket 天然支持多条（两台都是 OpenSSH 10.2）。
- 跑通前踩到两处「默认是本机」：
  1. 版本不同的后台会被当成旧后台。窗口会去接管它的终端，或者**按那台的 pid 停本机进程**。原型加了挂接模式，这种情况一律不启动、不停止、不替换后台。
  2. 开终端前在本机查目录是否存在，不在就退回本机 home，被那台后台拒绝。挂接时改成交给那台后台判断。

## Decision

窗口对每台电脑都用原生协议连它自己的后台。界面只有一套代码：标签、分屏、菜单、快捷键、看板、待办都读「当前这台」的工作区。

- **连接由窗口自己管。** 每台电脑起一个 `ssh -N -L <本机 socket>:<那台 socket> <host>`，断了按退避重连，ssh 选项沿用 `machines.rs` 现有那组（BatchMode、ConnectTimeout、ServerAlive）。本机 socket 放在 `<runtime>/machines/<host>/workbench.sock`。
- **挂接的后台不归这个窗口管。** 不启动、不停止、不替换、不当成旧后台。这一条已在原型里，正式版加测试守住。
- **那台的 socket 在哪由那台说。** 后台新增 `fibo socket`，打印自己正在用的 socket 路径（agentos 用的还是 `~/.local/share/keepline-workbench`）；没有这个命令的旧版提示「先更新那台的 Fibo」。
- **协议主版本不同就不连。** 和本机现在的规则一样（`Hello { major }`），并说清是哪台、该更新哪边。

**不做的：**

- 不给手机协议补标签和布局。手机继续用它，桌面不再用它画主区域。
- 不做跨电脑的分屏（一格本机、一格 agentos 在同一个标签里）。一个标签属于一台电脑的一个空间，和 [电脑 → 空间 → 会话](2026-11-07-machines-hold-spaces.md) 一致。
- 不同步两台的空间和设置。

## 本机专属的功能怎么处理

| 功能 | 现在读哪里 | 挂接时 |
|---|---|---|
| 开终端的目录、「目录不见了」提示 | 本机文件系统 | 交给那台后台判断（原型已做） |
| 新空间的默认目录、目录选择器、最近目录 | 本机 home、本机列目录 | 用那台的 home；列目录走后台请求（手机已有 `dirs`，原生协议补一个） |
| 欢迎页和设置里装 Hook | 本机 `~/.claude` 等 | 在那台上执行 `ssh host fibo hooks install <agent>`；不写本机配置 |
| 状态栏额度 | 本机 Agent 日志 | 读那台后台算好的（手机已有 `usage`、`quota`） |
| 在访达中显示、点文件链接、用编辑器打开 | 本机路径 | 不提供，菜单项置灰并说明原因；以后再考虑 `ssh` 打开 |
| 侧栏标题「本机」、状态栏「本机 · N 个会话」 | 写死 | 用那台后台 `machine.name` |
| 主题、字号、快捷键、插件界面 | 本机配置 | 保持本机：这些是窗口的，不是那台电脑的 |

## 分步

| 步 | 做什么 | 验收 | 估计 |
|---|---|---|---|
| 1 一台一个窗口 | 侧栏机器标题菜单加「在新窗口打开」：窗口自己起 `ssh -L`、挂接、重连；名字改成那台的；上表前三行 | 假 ssh 证据脚本（照 `remote-board-evidence.sh`，再起一个本机后台当另一台）：⌘T / ⌘D / ⇧⌘D / ⌘W / 运行 Agent、断线重连、旧版提示；真 agentos 走一遍 | 1.5–2 天 |
| 2 同一个窗口切换电脑 | `store`、`workspace`、`layout.panes`、`nav` 收进一个按电脑分的结构，点侧栏电脑或它的空间就切过去；其他电脑在侧栏仍用现有卡片摘要 | 切过去再切回来，两边的标签、焦点、分屏都在；性能门（`scripts/perf-gate.py`）不退 | 3–4 天 |
| 3 侧栏和看板也直接读各台工作区 | 每台的工作区都保持订阅，侧栏、看板、⌘J 跨电脑排序用同一套规则；删掉桌面里的偷看远程分支和卡片摘要 | `crates/fibo-core/tests/guard.rs` 的读数只降；远程会话在看板里和本机一样能拖 | 2–3 天 |

第 1 步做完就能日常用（agentos 一个窗口、本机一个窗口）；第 2、3 步做的是「一个窗口看全部」。

## PR #340 怎么办

建议**关掉不合**：它的主区域、开始页、窗口内分屏到第 3 步都会删掉，剩下 9 条接缝问题修了也会被扔。

要留下的只有设置里那句说明（「侧栏里只看，不能打字」已经不对了），放进第 1 步一起改。

## Consequences

- 远程和本机走同一套代码，以后本机加的功能远程自动就有，不用再写一遍。
- 两台电脑的 Fibo 要协议主版本相同，更新要两边都做。
- 本机窗口在那台上开的终端，那台自己的窗口也看得到、也能用，标签和分屏一致。两个窗口同时打字沿用现有的接管规则（后开的接管，另一边变「只读 · 可明确接管」）。
- 能 ssh 进去就等于完全控制那台的 Fibo，和现在 `fibo remote --stdio` 一样，不新增权限面。

## 放弃条件

- 第 1 步做完，经 Tailscale 连 agentos 打字回显 p95 > 200 ms（手机协议实测是 15–46 ms），说明原生协议过 ssh 太慢 → 回到给手机协议补字段。
- 第 2 步发现窗口里按电脑切换要改的地方远超上面的数（`self.workspace` 436 处、`self.store` 66 处），三天收不住 → 停在第 1 步「一台一个窗口」。
- 你两周内没用第 1 步的窗口连过 agentos → 停在第 1 步，不做第 2、3 步。

## 待定问题

| 问题 | 影响 | 最晚 |
|---|---|---|
| 第 1 步先用「新窗口」，还是直接做第 2 步「同一窗口切换」？ | 先新窗口能早两三天用上，但第 2 步要重做入口 | 开工前 |
| PR #340 关掉，还是先合进去当过渡？ | 合进去要先修 9 条接缝问题，约半天，之后再删 | 开工前 |
| 「在访达中显示」这类只能本机的功能，置灰还是隐藏？ | 只影响菜单 | 第 1 步验收前 |

## Verification

```sh
# 原型：agentos 上起临时后台，转发过来，跑挂接场景
ssh agentos 'mkdir -m 700 -p /tmp/fibo-proto && nohup /opt/homebrew/bin/fibo serve --runtime /tmp/fibo-proto >/tmp/fibo-proto/host.out 2>&1 &'
ssh -N -o StreamLocalBindUnlink=yes -L /tmp/fibo-attach/workbench.sock:/tmp/fibo-proto/workbench.sock agentos &
cargo build -p fibo-app --features fibo-app/visual-test
MOORING_ATTACH_EVIDENCE=1 target/debug/fibo-app --runtime /tmp/fibo-attach --attach --visual-evidence /tmp/fibo-attach-shots
```

## Alternatives

- **继续给远程补界面（PR #340 的路）。** 否：每个本机功能都要再写一遍，一天 11 条接缝问题，而且分屏只能存在于这个窗口里，那台自己的窗口看不到。
- **给手机协议补标签、布局和全部编辑请求。** 否：等于把原生协议再抄一遍，两份协议要一起维护。只有当原生协议过 ssh 太慢时才回到这条（见放弃条件）。
- **一条 `ssh host fibo bridge` 走 stdio，把原生协议塞进一条连接。** 否：原生协议一个窗格一条连接，塞进一条要自己做多路复用；`ssh -L` 现成支持。
- **不加挂接模式，直接把窗口指向转发过来的 socket。** 否：版本不同会被当成旧后台，去接管那台的终端，或按那台的 pid 停本机进程。

## More Information

- 原型：分支 `proto/attach-remote-host`，提交 `29c917e1`；场景 `crates/fibo-app/src/harness/attach_evidence.rs`。
- PR #340：https://github.com/majiayu000/fibo/pull/340
- 相关：`docs/multi-machine/spec.md`（第 1–4 步和放弃条件）、[电脑 → 空间 → 会话](2026-11-07-machines-hold-spaces.md)、[发布版自己更新，终端靠后台交接](2026-11-08-self-update.md)。
