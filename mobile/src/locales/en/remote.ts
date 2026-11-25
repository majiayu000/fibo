import type { Entry } from './index';

const plural = (n: unknown, one: string, many: string) => (Number(n) === 1 ? one : many);

export const remote: Record<string, Entry> = {
  // src/ui/parse.ts
  '{agent} 问：信任这个文件夹吗？': '{agent} asks: trust this folder?',
  '信任后，它能读、改、运行这个文件夹里的文件。这个决定会记在电脑上。不信任的话，它不会在这里工作。':
    'Once trusted, it can read, change and run the files in this folder. The choice is saved on the computer. If you don’t trust it, it won’t work here.',
  '更早的消息没有读进来：电脑第一次看到这段对话时，只读了日志的最后一段。':
    'Earlier messages weren’t loaded: when the computer first saw this conversation, it read only the end of the log.',
  '这段日志电脑只读了最后一段，里面没有消息。之后的新消息会显示，更早的要回电脑上看。':
    'The computer read only the end of this log, and it had no messages. New messages will show here; see earlier ones on the computer.',
  '正在读更早的消息…': 'Loading earlier messages…',
  '{why}。点这里再试一次': '{why}. Tap to try again',
  '更早的消息还在电脑的日志里。继续往上翻，或者点这里读': 'Earlier messages are still in the computer’s log. Keep scrolling up, or tap to load them',
  '5 小时窗口': '5-hour window',
  本周: 'This week',
  今天: 'Today',
  '{d} 天': (a) => `{d} ${plural(a.d, 'day', 'days')}`,
  '{h} 小时': (a) => `{h} ${plural(a.h, 'hour', 'hours')}`,
  '{minutes} 分钟': '{minutes} min',
  周日: 'Sun',
  周一: 'Mon',
  周二: 'Tue',
  周三: 'Wed',
  周四: 'Thu',
  周五: 'Fri',
  周六: 'Sat',
  '{m} 分后重置': 'Resets in {m} min',
  '{h} 时 {m} 分后重置': 'Resets in {h} h {m} min',
  '{h} 时后重置': 'Resets in {h} h',
  '{day}重置': 'Resets {day}',
  '{secs} 秒': '{secs} s',
  '{h} 小时 {m} 分钟': '{h} h {m} min',
  '{command} 通过': '{command} passed',
  '{command} 没通过': '{command} failed',
  '{check}，之后又改过': '{check}, changed since',
  电脑: 'Computer',
  这一轮改了哪些文件: 'Files changed this turn',
  这一轮还没改文件: 'No files changed this turn yet',
  这台手机的权限只显示文件名和行数: 'This phone’s access shows file names and line counts only',
  未提交的改动: 'Uncommitted changes',
  没有未提交的改动: 'No uncommitted changes',

  // src/remote/*
  '中继版本太旧：请升级中继，或者换一个新的中继': 'The relay is too old: upgrade it, or switch to a newer relay',
  '这台手机没有配对，或者已经被取消；重新扫码配对': 'This phone isn’t paired, or was unpaired. Scan the code to pair again',
  '电脑换了配对码，需要重新配对': 'The computer changed its pairing code. Pair again',
  '电脑那边断开了这台手机，5 分钟后才能再连': 'The computer disconnected this phone. You can reconnect in 5 minutes',
  '这台手机的权限改了，正在按新权限重连': 'This phone’s access changed. Reconnecting with the new access',
  '电脑上的 Fibo 不在线': 'Fibo on the computer is offline',
  '网络太慢，中继断开了连接': 'The network is too slow; the relay dropped the connection',
  '连不上：手机没网，或者中继、电脑不在线。先确认手机能上网，再看电脑上的 Fibo 开着没有':
    'Can’t connect: the phone has no network, or the relay or computer is offline. Check the phone is online, then that Fibo is running on the computer',
  '连不上：手机没网，或者中继、电脑不在线。先确认手机能上网，再看电脑上的 Fibo 开着没有（{code}）':
    'Can’t connect: the phone has no network, or the relay or computer is offline. Check the phone is online, then that Fibo is running on the computer ({code})',
  '中继连得上，但没接通电脑：多半是电脑上的 Fibo 没连到中继，看看电脑是不是睡着了、Fibo 开着没有；也可能是中继这会儿连接太多，暂时拒绝了，过一会儿再试':
    'The relay answers but didn’t reach the computer. Most likely Fibo on the computer isn’t connected to it: check the computer is awake and Fibo is running. The relay may also be turning away connections because it has too many; try again in a while',
  '中继没有正常应答，可能不在线或在重启。过一会儿再试': 'The relay isn’t answering properly; it may be down or restarting. Try again in a while',
  '连不上中继：先看看手机有没有网；有网的话，可能是中继不在线':
    'Can’t reach the relay. Check the phone has a network; if it does, the relay may be down',
  连接超时: 'Connection timed out',
  '电脑没认出这台手机，配对信息对不上。请在电脑的 Fibo 里点标题栏的手机图标，选「配对新手机」重新扫码':
    'The computer didn’t recognize this phone; the pairing no longer matches. In Fibo on the computer, click the phone icon in the title bar, choose “Pair a new phone” and scan again',
  '这不是 Fibo 的配对链接。请在电脑的 Fibo 里点标题栏的手机图标，选「配对新手机」，扫屏幕上的二维码':
    'This isn’t a Fibo pairing link. In Fibo on the computer, click the phone icon in the title bar, choose “Pair a new phone” and scan the QR code it shows',
  '这个配对链接来自另一个版本的 Fibo，手机认不出。请把电脑和手机上的 Fibo 都更新到最新，再重新配对':
    'This pairing link comes from another version of Fibo that the phone can’t read. Update Fibo on both the computer and the phone, then pair again',
  '配对链接不完整，可能没复制全。请重新扫码，或者重新复制整段链接': 'The pairing link is incomplete; it may not have been copied in full. Scan again, or copy the whole link again',
  '详情：{detail}': 'Details: {detail}',
  连接已断开: 'Disconnected',
  '电脑上的 Fibo 没接受这台手机：{why}': 'Fibo on the computer turned this phone away: {why}',
  '这台电脑的 Fibo 还不支持，更新后可用': 'Fibo on this computer doesn’t support this yet. Update it to use this',
  '连接断了，不确定发出去没有；看一眼对话再决定要不要重发': 'The connection dropped; not sure it was sent. Check the conversation before sending again',
  '看不懂的中继地址 {relay}': 'Unrecognized relay address {relay}',
};
