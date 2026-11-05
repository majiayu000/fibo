// Screenshot fixture: three computers answering from canned data shaped like the design's
// mockups (docs/phone-app/design.html), so every screen can be laid beside its mockup.
// Loaded only when the bundle is built with EXPO_PUBLIC_FIXTURE=1 (`_layout.tsx`); a normal
// build drops it. Nothing here reaches a real computer.
import { RemoteError } from './remote/client';
import { stage } from './remote/hub';
import type { Link } from './remote/link';
import { enqueue, outboxReady, unsent } from './store/outbox';

const now = () => Math.floor(Date.now() / 1000);
const link = (machine: string, name: string, relay: string): Link => ({
  relay,
  code: new Uint8Array(32),
  device: `fixture-${machine}`,
  key: new Uint8Array(32),
  machine,
  name,
});

const LINKS = [
  link('agent-os', 'Agent-OS', 'ws://192.168.1.20:8790'),
  link('dev-linux', 'dev-linux', 'wss://relay.example.com'),
  link('air', 'MacBook Air', 'wss://relay.example.com'),
];

const ALL_OPS = ['hello', 'list', 'read', 'send', 'keys', 'answer', 'ask', 'trust', 'chat', 'events', 'open', 'end', 'outcome', 'close', 'ping', 'diff', 'dirs', 'mkdir', 'quota', 'stop'];

const card = (o: Record<string, unknown>) => ({
  space: 's-mooring',
  tab: 'tab',
  cwd: '/Users/demo/code/mooring',
  phase: 'idle',
  agent: 'claude',
  session: `sess-${o.id}`,
  turn: 'turn-1',
  outcome: 'open',
  needs_you: false,
  ...o,
});

function cards(machine: string) {
  const t = now();
  if (machine === 'dev-linux') {
    return [card({ id: 'b1', space: 's-blog', cwd: '/home/demo/blog', title: '整理十月的周报草稿', phase: 'working', status: 'running', status_label: '在跑', updated: t - 26 * 60 })];
  }
  return [
    card({ id: 'a1', title: '把 main 推到远端', phase: 'waiting', approval: 'Do you want to proceed?', status: 'waiting', status_label: '等你回答', needs_you: true, updated: t - 120 }),
    card({ id: 'a2', title: '侧栏额度条换成竖条纹', agent: 'codex', cwd: '/Users/demo/code/mooring-quota', phase: 'waiting', waiting: '用竖条纹还是保持平滑？\n❯ 1. 竖条纹\n  2. 保持平滑', status: 'waiting', status_label: '等你回答', needs_you: true, updated: t - 300 }),
    card({ id: 'a3', title: 'cargo test 失败 3 个用例', cwd: '/Users/demo/code/mooring-pty', status: 'error', status_label: '出错了', needs_you: true, updated: t - 500 }),
    card({ id: 'a4', title: '手机端键盘挡住输入框', phase: 'working', status: 'running', status_label: '在跑', updated: t - 12 * 60 }),
    card({ id: 'a5', title: '合并 feat/ux-polish', status: 'review', status_label: '等你验收', updated: t - 3600 }),
    card({ id: 'a6', title: '诊断页取消采样改临时文件', status: 'review', status_label: '等你验收', outcome: 'deferred', updated: t - 7200 }),
    card({ id: 'a7', title: '中继加心跳和断线重连', agent: 'codex', status: 'review', status_label: '等你验收', outcome: 'deferred', updated: t - 9000 }),
    ...['配对链接带上电脑名', '审批条显示终端最后几行', '图标全集加第 12 轮', '修 CI 的 rustfmt', '侧栏分组标题上色'].map((title, i) =>
      card({ id: `d${i}`, title, status: 'closed', status_label: '做完了', outcome: 'done', updated: t - 1800 * (i + 1) }),
    ),
    card({ id: 'sh', agent: null, session: null, phase: 'shell', title: null }),
  ];
}

const SCREEN = `╭───────────────────────────────────────────╮
│ Bash command                              │
│                                           │
│   git push origin main                    │
│   Push 6 commits to origin                │
│                                           │
│ Do you want to proceed?                   │
│ ❯ 1. Yes                                  │
│   2. Yes, and don't ask again for git push commands │
│   3. No, and tell Claude what to do differently (esc) │
╰───────────────────────────────────────────╯`;

function chat(terminal: string) {
  const t = now();
  if (terminal === 'a4') {
    return {
      session: 'sess-a4',
      more: false,
      messages: [
        { seq: 1, at: t - 260, role: 'user', text: '安卓上键盘弹出来把输入框挡住了，看不到自己在打什么' },
        {
          seq: 2,
          at: t - 120,
          role: 'agent',
          text: 'RN 0.88 在 Android 上强制全屏绘制，键盘弹出时窗口不再缩小。改了两处：\n- `KeyboardAvoidingView` 两个平台都用 padding\n- 偏移量取真实的标题栏高度\n\n类型检查通过。要我重新打包装到手机上吗？',
          tools: { commands: 3, files: [{ path: 'src/app/chat.tsx', added: 6, removed: 2 }] },
        },
      ],
    };
  }
  return {
    session: `sess-${terminal}`,
    more: false,
    messages: [
      { seq: 1, at: t - 400, role: 'user', text: '测完把 main 推上去' },
      { seq: 2, at: t - 130, role: 'agent', text: '本地检查都过了，准备推送。' },
    ],
  };
}

const PATCH = `@@ -190,8 +191,12 @@
   <KeyboardAvoidingView
-    behavior={ios ? 'padding' : undefined}
-    keyboardVerticalOffset={ios ? 90 : 0}
+    // Android draws edge to edge…
+    behavior={web ? undefined : 'padding'}
+    keyboardVerticalOffset={header}
   >
`;

const DIFF = {
  cwd: '/Users/demo/code/mooring/mobile',
  branch: 'main',
  added: 9,
  removed: 3,
  truncated: false,
  files: [
    { path: 'src/app/chat.tsx', added: 6, removed: 2, status: 'M', patch: PATCH, truncated: false },
    { path: 'app.json', added: 3, removed: 1, status: 'M', patch: '@@ -1,3 +1,5 @@\n {\n-  "a": 1\n+  "a": 2,\n+  "b": 3\n }\n', truncated: false },
  ],
};

const HOME = '/Users/demo';
function dirs(fields: Record<string, unknown>) {
  if (fields.query) {
    const q = String(fields.query);
    return {
      path: HOME,
      home: HOME,
      dirs: [
        { name: 'keepline', path: `${HOME}/code/AI/tools/keepline`, git: { branch: 'main', changes: 0, worktree: false }, sessions: 3, recent: true },
        { name: 'keepline-mobile', path: `${HOME}/code/AI/tools/keepline-mobile`, git: { branch: 'feat/mobile', changes: 0, worktree: true }, sessions: 0 },
        { name: 'keepline-t3-titlebar', path: `${HOME}/code/AI/tools/keepline-t3-titlebar`, git: { branch: 'feat/t3-titlebar', changes: 0, worktree: true }, sessions: 0 },
        { name: 'keep-notes', path: `${HOME}/Documents/keep-notes`, sessions: 0 },
      ].filter((d) => d.name.includes(q)),
      recent: [],
      // Only this query, so the screen's 「没搜完」 line can be looked at.
      truncated: q === 'keep',
    };
  }
  const path = String(fields.path ?? `${HOME}/code/AI/tools`);
  return {
    path,
    parent: path.split('/').slice(0, -1).join('/') || '/',
    home: HOME,
    dirs: [
      { name: 'mooring', path: `${path}/mooring`, git: { branch: 'main', changes: 2, worktree: false }, sessions: 3 },
      { name: 'mooring-mobile', path: `${path}/mooring-mobile`, git: { branch: 'feat/mobile', changes: 0, worktree: true }, sessions: 0 },
      { name: 'alma-relay', path: `${path}/alma-relay`, git: { branch: 'main', changes: 0, worktree: false }, sessions: 0 },
      { name: 'scratch', path: `${path}/scratch`, sessions: 0 },
    ],
    recent: [`${HOME}/code/mooring`, `${HOME}/code/blog`, `${HOME}/code/alma-relay`],
  };
}

function quota() {
  const t = now();
  const weekday = (d: number) => {
    const x = new Date();
    x.setDate(x.getDate() + ((d - x.getDay() + 7) % 7 || 7));
    return Math.floor(x.getTime() / 1000);
  };
  return {
    agents: [
      { agent: 'claude', plan: 'max', at: t - 60, windows: [{ minutes: 300, used: 62, resets_at: t + 100 * 60 }, { minutes: 10080, used: 38, resets_at: weekday(4) }] },
      { agent: 'codex', plan: 'pro', at: t - 60, windows: [{ minutes: 300, used: 15, resets_at: 0 }, { minutes: 10080, used: 81, resets_at: weekday(6) }] },
    ],
  };
}

function remote(machine: string) {
  const answer = (op: string, f: Record<string, unknown>): unknown => {
    switch (op) {
      case 'hello':
        return { version: 1, machine: { id: machine, name: LINKS.find((l) => l.machine === machine)?.name }, ops: ALL_OPS, role: machine === 'dev-linux' ? 'answer' : 'full', requests: true, os: machine === 'dev-linux' ? 'linux' : 'macos', app_version: '0.1.0' };
      case 'list':
        return {
          machine: { id: machine, name: LINKS.find((l) => l.machine === machine)?.name },
          spaces: [
            { id: 's-mooring', name: 'mooring', cwd: '/Users/demo/code/mooring' },
            { id: 's-blog', name: 'blog', cwd: '/home/demo/blog' },
          ],
          terminals: cards(machine),
        };
      case 'read':
        return { text: SCREEN };
      case 'ask':
        return { question: 'Do you want to proceed?', kind: 'Bash command', subject: 'git push origin main', note: '推送 main 到远端' };
      case 'chat':
        return f.after || f.before ? { session: f.session, more: false, messages: [] } : chat(String(f.terminal));
      case 'diff':
        return DIFF;
      case 'dirs':
        return dirs(f);
      case 'quota':
        return quota();
      case 'ping':
        return {};
      default:
        return {};
    }
  };
  return {
    ended: null,
    request: (op: string, f: Record<string, unknown> = {}) =>
      new Promise((done) => setTimeout(() => done(answer(op, f)), op === 'ping' ? (machine === 'dev-linux' ? 64 : 18) : 20)),
    stream: async () => ({ stream: 1, close: async () => undefined }),
    onEnded: () => undefined,
    close: () => undefined,
  };
}

export function install() {
  stage(LINKS, async (l) => {
    if (l.machine === 'air') throw new RemoteError('电脑上的 Mooring 不在线', 1012);
    return remote(l.machine) as never;
  });
  outboxReady.then(() => {
    if (!unsent().length) enqueue({ machine: 'air', terminal: 'x', text: '测完把 APK 装到手机上' });
  });
}
