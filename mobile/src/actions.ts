// What a tap does when it changes something on the computer, with the one check this phone
// adds first: a second confirmation for dangerous commands. The computer checks permissions
// itself. No fingerprint or face check: docs/decisions/2026-11-28-phone-no-lock.md.
import { Alert, Platform } from 'react-native';
import { tr } from './i18n';
import { type Card, getMachine, NOT_YET, refresh, write } from './remote/hub';
import { readSettings } from './store/settings';
import { FULL_CONFIRM, type ModeKind, type ModeOption } from './ui/modes';
import { type Ask, asksTwice } from './ui/parse';

/** Yes or no from the person, as a system dialog. */
export function confirm(title: string, detail: string, ok: string, destructive = false): Promise<boolean> {
  if (Platform.OS === 'web') return Promise.resolve(!!globalThis.confirm?.(`${title}\n${detail}`));
  return new Promise((done) =>
    Alert.alert(title, detail, [
      { text: tr('取消'), style: 'cancel', onPress: () => done(false) },
      { text: ok, style: destructive ? 'destructive' : 'default', onPress: () => done(true) },
    ]),
  );
}

// All approval entry points share this lock, including taps before React redraws.
const answering = new Set<string>();
async function answerOnce(machine: string, terminal: string, run: () => Promise<boolean | void>) {
  const key = JSON.stringify([machine, terminal]);
  if (answering.has(key)) return false;
  answering.add(key);
  try {
    return await run();
  } finally {
    answering.delete(key);
  }
}

/**
 * Allows the approval `card` shows. `ask` is what it asks about, for the second
 * confirmation; the machine refuses if the screen has moved on to another. `always` picks
 * the menu's "don't ask again" option, the one `ask.always` words: refused if it has changed.
 */
export async function approve(machine: string, card: Card, ask: Ask, always = false): Promise<boolean> {
  const done = await answerOnce(machine, card.id, async () => {
    if ((await readSettings()).confirmDanger && asksTwice(ask)) {
      const what = ask.known ? ask.subject : tr('电脑没读出它要做什么，打开终端看一眼更稳妥。');
      const sure = await confirm(tr('确定允许？'), always ? `${what}\n\n${ask.always}` : what, tr('允许'));
      if (!sure) return false;
    }
    await write(machine, 'answer', {
      terminal: card.id,
      question: card.approval,
      allow: true,
      ...(ask.known ? { subject: ask.subject } : {}),
      ...(always ? { always: true, always_label: ask.always } : {}),
    });
    setTimeout(() => refresh(machine), 300);
    return true;
  });
  return done === true;
}

/** Refuses it, then sends feedback only after the original conversation leaves its menu. */
export async function deny(machine: string, card: Card, ask: Ask, feedback?: string): Promise<boolean> {
  const target = { id: card.id, session: card.session };
  return answerOnce(machine, card.id, async () => {
    await write(machine, 'answer', { terminal: card.id, question: card.approval, allow: false, ...(ask.known ? { subject: ask.subject } : {}) });
    setTimeout(() => refresh(machine), 300);
    if (!feedback) return true;
    // Keep the shared lock through the acknowledgement, menu exit, and feedback delivery.
    const until = Date.now() + 5000;
    while (Date.now() < until) {
      await refresh(machine);
      const m = getMachine(machine);
      const current = m?.cards.find((c) => c.id === target.id);
      if (m?.status !== 'online' || !current || !target.session || current.session !== target.session) {
        throw new Error(tr('原对话已离开这个终端，说明未发送'));
      }
      if (!current.approval && !current.trust) {
        await sendToConversation(machine, target, feedback);
        return true;
      }
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
    throw new Error(tr('审批框还没有退出，说明未发送；请稍后在对话里发送'));
  }).then((done) => done === true);
}

/** Says whether to trust the folder a card asks about; the machine presses the keys. */
export async function answerTrust(machine: string, card: Card, trust: boolean) {
  const folder = card.trust?.folder;
  if (!folder) return;
  return answerOnce(machine, card.id, async () => {
    await write(machine, 'trust', { terminal: card.id, folder, trust });
    setTimeout(() => refresh(machine), 300);
  });
}

/**
 * Picks a model or permission mode the computer offered; it presses the keys and says what the CLI
 * shows then. The one that stops asking goes through a second confirmation first, and
 * only then carries `allow_full`, without which the computer refuses it. False: the person said no.
 */
export async function switchMode(machine: string, terminal: string, kind: ModeKind, option: ModeOption): Promise<boolean> {
  if (option.full) {
    if (!(await confirm(FULL_CONFIRM.title, FULL_CONFIRM.detail, FULL_CONFIRM.ok, true))) return false;
  }
  await write(machine, 'setmode', { terminal, kind, choice: option.key, ...(option.full ? { allow_full: true } : {}) });
  setTimeout(() => refresh(machine), 200);
  return true;
}

/** Feedback belongs to the conversation shown when the action started, even after a reconnect. */
export function sendToConversation(machine: string, card: Pick<Card, 'id' | 'session'>, text: string) {
  return write(machine, 'send', { terminal: card.id, session: card.session ?? '', text, enter: true });
}

/** Settles a conversation: 已处理 is `done`, 稍后提醒 `deferred` (the machine brings it back in an hour), back to the inbox `open`. */
export async function settle(machine: string, card: Card, outcome: 'done' | 'deferred' | 'abandoned' | 'open') {
  if (!card.session) throw new Error(tr('这个终端里没有 Agent 对话'));
  await write(machine, 'outcome', {
    session: card.session,
    outcome,
    ...(card.turn && outcome !== 'open' ? { turn: card.turn } : {}),
  });
  setTimeout(() => refresh(machine), 200);
}

/** Shows an error the way the platform does. */
export function oops(text: string) {
  if (Platform.OS === 'web') globalThis.alert?.(text);
  else Alert.alert(text);
}

/** 先不管 a space on the computer, or take it back (`spacequiet`); the list then says so. */
export async function setSpaceQuiet(machine: string, space: string, quiet: boolean) {
  await write(machine, 'spacequiet', { space, quiet });
  setTimeout(() => refresh(machine), 200);
}
