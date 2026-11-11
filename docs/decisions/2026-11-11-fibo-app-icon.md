---
status: accepted
date: 2026-11-11
decision-makers: majiayu000
---

# App 图标换成 Fibo 标志：f 加金点，晚霞渐变底

## 背景

官网、README 和 favicon 在 11-10 换成了 Fibo 的字标和图标（f 加一颗金点、晚霞渐变底，`docs/site/brand/fibo-icon.svg`），App 的 Dock、标题栏和托盘还是系泊环，两边对不上。用户说：「我们的图标是不是也可以换了？我们用最新的。」

## 决定

- 设置 › 外观 › 应用图标多一个「Fibo 标志」，排第一，没选过的人默认是它。存过「系泊环」「二次元」「吉祥物」的人保持原样，存过的值不迁移。
- Dock：`assets/Mooring.icon` 的两层换成渐变底和 f 加点，仍经 actool 编译，避免 macOS 26 套底板；`assets/Mooring.icns` 由同一张图按苹果网格（1024 里占 824）生成，只给旧系统。运行时的 Dock 图 `dock-fibo.png` 和标题栏图 `title-fibo.png` 也出自这份 SVG。
- 托盘：模板图画 f 加点，在线时有点，连不上后台时去掉点（以前是去掉方桩）。f 的轮廓从 SVG 路径转成 `native.m` 里的 `F_MARK` 数据，不另画。

不做：不删系泊环、二次元、吉祥物；不改 og.png。

## 后果

- 托盘的 f 比系泊环窄，右边的数字离它更近；点只有约 3 个像素（2 倍屏），在真实菜单栏里要再看。
- 以后要改图标，从 `docs/site/brand/fibo-icon.svg` 重新出这几张图，别单改一张。

## 验证

- `cargo test -p mooring-app app_icon`：没存过、存了不认识的值都回到 Fibo 标志，存过系泊环保持系泊环。
- 设置 › 关于里显示的图标，应和 Dock 上的一样。

## 否掉的

- 只换默认、不保留旧选项：有人主动选过系泊环或二次元，不该替他换。
- 托盘用彩色图标：菜单栏要跟深浅色走，彩色图在深色菜单栏里看不清。

## 更多

取代 [系泊环](2026-11-05-mooring-ring-icon.md)。
