// One made-up reply using everything the phone has to draw, for before/after screenshots.
export const GALLERY = [
  `## 改完了，两处要你看

**结论：**键盘不再挡输入框，改了 \`foo_bar_baz.rs\` 和 **\`kit.tsx\`** 两处。详情见 [\`chat.tsx\`](mobile/src/app/chat.tsx:184)，说明在 https://example.com/docs/a_b_c 。

1. 先确认标题栏高度
   - 取 \`useHeaderHeight\`，不再写死 90
   - 两个平台都用 padding
2. 再跑一遍真机：
   \`\`\`sh
   ./gradlew assembleRelease -PreactNativeArchitectures=arm64-v8a
   \`\`\`

| 机型 | 之前 | 之后 |
|---|---|---|
| OPPO | 挡住 | 正常 |
| iPhone 15 | 正常 | 正常 |`,
  `> 引用：手机只做聊天，内容主要是 Agent 的回复。

| 文件 | 改了什么 | 为什么 | 风险 |
|---|---|---|---|
| \`mobile/src/ui/markdown.tsx\` | 解析换成语法树，渲染继续自己画 | 真实语料里三成最终回复被弄坏 | 低，只影响显示 |
| \`crates/mooring-core/src/content/plain.rs\` | 只把真正的表格按列切开 | \`a \\| b\` 这样的行不该被切 | 低 |

\`\`\`rust
fn plain_line(text: &str) -> String { text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ") }
\`\`\`

- [x] 已完成的一项
- [ ] 还没做的一项

截图：![真机截图](/tmp/shot_1.png) ~~旧说法~~`,
];
