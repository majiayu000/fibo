# 它在做事时，再发一句会怎样（各家 CLI 实测）

**要支撑的决定：** 手机上「它在做事时发的消息」能不能做到和在电脑上直接用 CLI 一样：排队、马上插进去（引导）、中断、换强度，每家各按什么键。

11-09 在本机真开 Claude Code v2.1.319 和 Codex（GPT-6.1-Sol），用手机同样的两条路（`send` 粘贴、`keys` / `input` 按键）逐项试。

| 做什么 | Claude Code | Codex | OpenCode |
|---|---|---|---|
| 它在做事时，粘贴后按回车 | **排队**：画面写「ctrl+enter to send now」，输入框写「Press up to edit queued messages」，做完手头这步才看 | **马上插进去**：没等后台命令跑完就回了 STEERED | 没测 |
| 排队 | 回车 | **Tab**（画面写「tab to queue message」）；排着的显示在「Queued follow-up inputs」，⇧← 改最后一条 | 没测 |
| 马上插进去（引导） | **Ctrl+Enter**，终端里是 `ESC[13;5u`（kitty 键码）；实测它中途改口回了 STEERED | 回车 | 没测 |
| 中断这一轮 | Esc（Mooring「停下」已在用） | Esc（画面写「esc to interrupt」） | 插件已声明（`plugins/opencode`） |
| 强度 | `/effort`：low · medium · high · xhigh · max，←/→ 调，回车确认，**s 只改这次对话**，Esc 取消；底栏显示「◐ medium · /effort」 | `/model` 选完模型进第二步：Low · Medium · High · Extra high · More reasoning…（Max、Ultra），回车改默认，**s 只改这次** | 没测 |

## 结论

- **手机现在发消息，在两家上效果相反。** 手机的 `send` 是粘贴加回车：Claude 排队，Codex 马上插进去。手机却对两家都写「已排队 · 它做完手头这一步就会看到」，对 Codex 是错的。
- **排队和引导两家都有，只是键不同**，可以照 `crates/fibo-core/src/agent.rs` 里 `interrupt`（每家声明中断用哪个键）的做法，让每家再声明「排队用什么键、马上发用什么键」。`send` 加一个「怎么发」，后台按这家声明的键发；没声明的 Agent 手机上不出这个选择。
- **强度两家都在菜单里，都有「只改这次对话」的 s**，和现在换模型、换权限是同一种做法（读菜单、按键、核对画面），能接进现有的 `modes` / `setmode`。
- 手机现在的按键列表里没有 Ctrl+Enter，要用 `input` 发原始键码，或者给 `keys` 加一个名字。

## 已做（11-09）

- **排队 / 引导**：每家在适配器里声明键（`Adapter::turn_keys`），`send` 带 `how` 时电脑先粘贴再按这家的键；没声明的 Agent 手机上不出选择。真 Claude、真 Codex 各走通两种。
- **默认排队，点气泡立刻发送**（11-09 晚，用户：「排队引导应该是默认发出去 不分排队还是引导的 如果在排队 可以点击一下 消息气泡 有一个 立刻发送」）：它在跑时手机不再让人先选排队还是引导，一律排队；排着的气泡点一下给「立刻发送」，电脑按这家的引导键、不粘贴任何字（`sendnow`）。Claude 上输入框空着按 Ctrl+Enter 会把排着的几句**一起**发出，所以手机也说「排着的 N 句会一起发出去」。只对 Claude 打开（Codex 没测）。撤回、改一下没做：Claude 按 ↑ 会把整个队列拉回输入框，只撤一句要先清空再把别的重新排上，依赖它输入框怎么删行，容易残字或误发。
- **强度**：`modes` / `setmode` 多一种 `effort`（`agents/modes_effort.rs`）。Claude 读 `/effort` 滑杆上 ▲ 指着哪档（底栏的「● high · /effort」时有时无，不能靠它），换完核对「Set effort level to high (this session only)」；Codex 走 `/model` → 当前模型 → 档位菜单，Max、Ultra 在「More reasoning…」下一层，换完核对底栏。手机在 + 面板里多一行「强度」。
- **Codex 在 Ultra 档时输入框符号是 `»` 不是 `›`**：换模型、换权限、换强度的「在输入框」判断都已认这两种；状态识别（`AgentSpec` 的 prompt markers、`agent_input_ready`）还只认 `›`，没改。

## 没验

- OpenCode：旁屏标签只剩 25×15，OpenCode 一启动就段错误（退出码 139），要动用户窗口布局才能腾地方，没测。
- Claude 的 `ESC[13;5u` 只在 Mooring 自己的终端里试过；别的终端（iTerm、Ghostty 外层）没试。
- 两家改版后提示文字和按键可能变，读画面的规则要跟着看。
