---
name: mooring
description: "用户说「记一下」「记下来」「加个待办」「回头再看」，或有这次不处理但之后要跟进的事时，记进 Mooring 待办。当场问完答完的事不记。"
---
<!-- 由 Mooring 写入，App 启动时会更新这个文件。 -->

# 记进 Mooring 待办

1. 先运行 `mooring todo list`，看是不是已经有这件事。
2. 已经有：`mooring todo note --task <编号> "补充的内容"`。
3. 没有：`mooring todo add "一句话标题" --said "用户原话"`。报错说要指定空间时，加 `--space <空间>`，错误里有可选的空间。

记完用一句话告诉用户记了什么。

完成由用户在 Mooring 窗口里标记，你不能标。

`mooring` 找不到或连不上时，告诉用户没记上和报错内容。不要绕过命令，直接写 Mooring 的 socket 或数据库。
