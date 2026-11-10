// Words for the reader, in Chinese or English. Like the desktop's `t!` (named `tr` here, since `t` is
// the theme everywhere in this app): the Chinese is the key and
// the text, `src/locales/en/*.ts` holds the English. A key with no English shows the Chinese.
// `test/i18n.test.ts` checks every shown Chinese literal goes through `tr` (or `tk` / `notUi`)
// and that every key has English.
import { useSyncExternalStore } from 'react';
import { EN } from './locales/en';

export type Lang = 'zh' | 'en';
export type Choice = 'auto' | Lang;

/** The phone's own language: Chinese when its first language is a Chinese one, else English. */
function system(): Lang {
  // Tests pin it (bunfig.toml preloads test/lang.ts), so a run on an English Mac checks the Chinese.
  const pinned = typeof process !== 'undefined' ? process.env?.FIBO_PHONE_LANG : undefined;
  if (pinned === 'zh' || pinned === 'en') return pinned;
  try {
    const nav = (globalThis as { navigator?: { language?: string } }).navigator?.language;
    const locale = nav || Intl.DateTimeFormat().resolvedOptions().locale || '';
    return /^zh/i.test(locale) ? 'zh' : 'en';
  } catch {
    return 'zh';
  }
}
const SYSTEM = system();

// The reader's choice, fed from the settings store by the root layout; kept here so this file
// pulls in no React Native and plain tests can import anything that says words.
let choice: Choice = 'auto';
const listeners = new Set<() => void>();

export function setLanguage(c: Choice) {
  if (c === choice) return;
  choice = c;
  listeners.forEach((f) => f());
}

export function lang(): Lang {
  return choice === 'auto' ? SYSTEM : choice;
}

const subscribe = (f: () => void) => {
  listeners.add(f);
  return () => void listeners.delete(f);
};

/** For the root layout: the whole tree draws again when the language changes. */
export function useLang(): Lang {
  return useSyncExternalStore(subscribe, lang);
}

type Args = Record<string, string | number | null | undefined>;

/** `tr('中文')` is the text in the current language; `tr('{n} 个', { n })` fills `{n}` in. */
export function tr(zh: string, args?: Args): string {
  const en = lang() === 'en' ? EN[zh] : undefined;
  // `电脑#list`: one Chinese word that needs two English forms, as on the desktop; Chinese drops the tag.
  const text = en === undefined ? zh.replace(/#[a-z]+$/, '') : typeof en === 'function' ? en(args ?? {}) : en;
  return args ? text.replace(/\{(\w+)\}/g, (m, k: string) => (k in args ? String(args[k] ?? '') : m)) : text;
}

/** A key kept as the Chinese itself where `t` cannot run yet (a table built at load); say it with `tr` where shown. */
export const tk = (zh: string) => zh;

/** Chinese that is data, not words for the reader (matched against a screen, sent to an Agent). */
export const notUi = (zh: string) => zh;
