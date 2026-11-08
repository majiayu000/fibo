// What the phone offers while an Agent waits on you and it is not an approval or a folder to
// trust (those have their own cards). Every wait gets a way forward: its options as buttons;
// else the terminal's last lines and the keys a terminal menu takes; else, when only the
// computer can do it (a browser page, a form), it says so. A wait the phone could not read
// used to leave 「等你回答」 and Esc alone, and the conversation stuck (2026-11-08).
import { type Choice, choices, questionText } from './parse';

export type WaitView = {
  /** What it asks, without its numbered options. */
  text: string;
  /** One button each; their number key answers. */
  options: Choice[];
  /** ↑ ↓ 回车 for a menu the phone could not read: the terminal lines show what they move. */
  keys: boolean;
  /** Only the computer can answer it: why, said where the buttons would be. */
  computer: string;
};

/** Waits only the computer can finish, by the words the host gives them (reasons.json). */
const COMPUTER: [RegExp, string][] = [
  [/^MCP 请求在浏览器中完成/, '要在电脑的浏览器里完成，手机上做不了'],
  [/^MCP 表单等待填写/, '这个表单要在电脑上填，手机上还填不了'],
];

export function waitView(waiting: string | null | undefined): WaitView | null {
  if (!waiting) return null;
  const options = choices(waiting);
  const text = (options.length ? questionText(waiting) : waiting.trim()) || '它在问你，点「看终端」看问的是什么';
  const computer = COMPUTER.find(([r]) => r.test(waiting))?.[1] ?? '';
  return { text, options, keys: !options.length && !computer, computer };
}

/** Waits whose answer is words: what is typed is the reply. */
const FREE = /^(?:Agent 等待你的输入|等待你的输入|等待你处理|子 Agent)/;

/**
 * Text typed while a menu is open lands in the menu, and its Enter picks whatever is
 * highlighted. So the first send says so and keeps the text; the same text sent again goes,
 * since a question may well want words and the phone cannot always tell.
 */
export function sendBlocked(view: WaitView | null, waiting: string | null | undefined, again: boolean): string {
  if (!view || view.computer || (waiting && FREE.test(waiting))) return '';
  if (again && !view.options.length) return '';
  return view.options.length
    ? '它在等你从上面的选项里选一个；想说别的，先按 Esc 关掉选择框'
    : '它开着一个选择框，这段话会落进框里、回车会选中当前那一项。确定要发，再点一次发送';
}

/**
 * An Agent started but no conversation yet: it may be showing a menu of its own first (an
 * update offer, a login choice), which no log or hook reports. The terminal and its keys.
 */
export const STARTING: WaitView = { text: 'Agent 还没开始对话；它要是在终端里问你什么，在这里答', options: [], keys: true, computer: '' };
