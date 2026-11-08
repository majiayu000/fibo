// What a tap does when it changes something on the computer, with the checks this phone
// adds first: a second confirmation for dangerous commands and Face ID / fingerprint when
// settings ask. The computer checks permissions itself; these only guard the phone.
import * as LocalAuthentication from 'expo-local-authentication';
import { Alert, Platform } from 'react-native';
import { type Card, NOT_YET, refresh, write } from './remote/hub';
import { settings } from './store/settings';
import { FULL_CONFIRM, type ModeKind, type ModeOption } from './ui/modes';
import { type Ask, asksTwice } from './ui/parse';

/** Yes or no from the person, as a system dialog. */
export function confirm(title: string, detail: string, ok: string, destructive = false): Promise<boolean> {
  if (Platform.OS === 'web') return Promise.resolve(!!globalThis.confirm?.(`${title}\n${detail}`));
  return new Promise((done) =>
    Alert.alert(title, detail, [
      { text: '取消', style: 'cancel', onPress: () => done(false) },
      { text: ok, style: destructive ? 'destructive' : 'default', onPress: () => done(true) },
    ]),
  );
}

/** Face ID / fingerprint (or the passcode) when the lock setting is on; true when it is off. */
export async function verify(reason: string): Promise<boolean> {
  if (!settings.get().lock || Platform.OS === 'web') return true;
  try {
    if (!(await LocalAuthentication.hasHardwareAsync())) return true;
    const r = await LocalAuthentication.authenticateAsync({ promptMessage: reason, cancelLabel: '取消' });
    return r.success;
  } catch {
    return false;
  }
}

/**
 * Allows the approval `card` shows. `ask` is what it asks about, for the second
 * confirmation; the machine refuses if the screen has moved on to another. `always` picks
 * the menu's "don't ask again" option, the one `ask.always` words: refused if it has changed.
 */
export async function approve(machine: string, card: Card, ask: Ask, always = false): Promise<boolean> {
  if (settings.get().confirmDanger && asksTwice(ask)) {
    const what = ask.known ? ask.subject : '电脑没读出它要做什么，打开终端看一眼更稳妥。';
    const sure = await confirm('确定允许？', always ? `${what}\n\n${ask.always}` : what, '允许');
    if (!sure) return false;
  }
  if (!(await verify(`允许 ${ask.subject}`))) return false;
  await write(machine, 'answer', {
    terminal: card.id,
    question: card.approval,
    allow: true,
    ...(ask.known ? { subject: ask.subject } : {}),
    ...(always ? { always: true, always_label: ask.always } : {}),
  });
  setTimeout(() => refresh(machine), 300);
  return true;
}

/** Refuses it; `ask` as with `approve`, so an answer that took a while refuses nothing newer. */
export async function deny(machine: string, card: Card, ask: Ask) {
  await write(machine, 'answer', { terminal: card.id, question: card.approval, allow: false, ...(ask.known ? { subject: ask.subject } : {}) });
  setTimeout(() => refresh(machine), 300);
}

/** Says whether to trust the folder a card asks about; the machine presses the keys. */
export async function answerTrust(machine: string, card: Card, trust: boolean) {
  if (!card.trust) return;
  if (trust && !(await verify('信任这个文件夹'))) return;
  await write(machine, 'trust', { terminal: card.id, folder: card.trust.folder, trust });
  setTimeout(() => refresh(machine), 300);
}

/**
 * Picks a model or permission mode the computer offered; it presses the keys and says what the CLI
 * shows then. The one that stops asking goes through a second confirmation and Face ID first, and
 * only then carries `allow_full`, without which the computer refuses it. False: the person said no.
 */
export async function switchMode(machine: string, terminal: string, kind: ModeKind, option: ModeOption): Promise<boolean> {
  if (option.full) {
    if (!(await confirm(FULL_CONFIRM.title, FULL_CONFIRM.detail, FULL_CONFIRM.ok, true))) return false;
    if (!(await verify(FULL_CONFIRM.title))) return false;
  }
  await write(machine, 'setmode', { terminal, kind, choice: option.key, ...(option.full ? { allow_full: true } : {}) });
  setTimeout(() => refresh(machine), 200);
  return true;
}

/** Settles a conversation: 已处理 is `done`, 稍后提醒 `deferred` (the machine brings it back in an hour), back to the inbox `open`. */
export async function settle(machine: string, card: Card, outcome: 'done' | 'deferred' | 'open') {
  if (!card.session) throw new Error('这个终端里没有 Agent 对话');
  try {
    await write(machine, 'outcome', {
      session: card.session,
      outcome,
      ...(card.turn && outcome !== 'open' ? { turn: card.turn } : {}),
    });
  } catch (e) {
    // A host that only knows done, abandoned and open names what it refused.
    if (outcome === 'deferred' && /只能是|deferred/.test((e as Error).message)) throw new Error(`稍后提醒：${NOT_YET}`);
    throw e;
  }
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
