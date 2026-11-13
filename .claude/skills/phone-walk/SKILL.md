---
name: phone-walk
description: 在真手机上把 Fibo 手机 App 的每个页面走一遍并截图（docs/phone-app/walkthrough.md 的整张清单）。改了手机能看到或能操作的东西、换了电脑 App 或手机 App、更新了插件之后，说做完之前用；用户说「走查」「真机走一遍」「手机全测一遍」「截图验一下手机」时用。
---

# 手机全量走查

清单是 `docs/phone-app/walkthrough.md`，工具是 `scripts/phone-walk.py`。每次都走整张表，不只走改到的那一块。没走的项会被报告列成「没验」，要在汇报里原样写出来。

## 步骤

```sh
w(){ python3 scripts/phone-walk.py "$@"; }
w start          # 新建本次记录：~/Documents/claude-pages/mooring/<日期>-phone-walk/
w setup          # 临时 git 目录 + 测试 Claude 会话，自动答「信任这个文件夹」
w open-chat      # 深链直接进测试会话
```

然后按清单逐项走。每项至少一张截图，看过截图再记结果：

```sh
w texts                      # 屏幕上的文字和坐标，先看再点
w tap "发送"                 # 按文字点；同一行的重复节点算一个
w type "phone-walk ..."      # 只能打 ASCII；发给 Claude 的话都带 phone-walk
w shot chat.send "回复后"     # 截图 + UI dump，记在这一项下面
w mark chat.send 过 "walk-send-ok 回来，消息不重复"
w report                     # 生成 index.html，打印过 / 不过 / 跳过 / 没验
w teardown                   # 关测试终端（含手机开的新任务），删目录、计划文件和测试图
```

用 `open <记录目录>/index.html` 给用户看报告。

## 规矩

- 只进测试会话。用 `open-chat` 进，不在首页点卡片。
- 「允许」「拒绝」「发送」等按钮，屏幕上没有 `phone-walk` 时 `tap` 会拒绝按。「移除」「以后都允许」永远不按。
- 「只看」的项不按任何按钮。`todo.new` 会留下一件要用户标记完成的待办，用户同意才走。
- `tap` 只在 Fibo 在前台时点；系统选图页要点「添加」时加 `--any-app`。前台变成别的 App（短信、安装框）就停下，问用户，别替他点。
- 收键盘用 `w key hide`（键盘开着才按返回）。`w key back` 在键盘收起时会退出当前页面。
- 系统选图页、首页都会露出私人内容。系统选图页不截；截图只放在 `~/Documents/claude-pages`，不进仓库。
- 选测试图用 `w pick-image`，它按推图的时间找那一张，找不到就不点；不要按坐标点第一格，第一格常常是用户自己的照片。
- 一台手机同时只能一个会话走查：`start` 发现别的终端 10 分钟内还在走就拒绝。不要在别人走查时换电脑 App。
- 测试会话的 `sleep` 会被 Claude 放到后台，或被钩子拦下。要一轮跑久一点，就让它在前台跑 `ping -c N 127.0.0.1`。

## 收尾

- 汇报写：过 / 不过 / 跳过 / 没验 各几项。每个「不过」和小毛病单独列一行。
- 跳过的项写原因。
- 新发现的、只有真机才看得到的问题，在 `docs/phone-app/real-device-findings.md` 加一行。
