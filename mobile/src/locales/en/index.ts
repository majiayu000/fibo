// English for every `tr('中文')` key, one file per area. A value may be a function for plurals.
export type Entry = string | ((args: Record<string, string | number | null | undefined>) => string);
import { chat } from './chat';
import { common } from './common';
import { home } from './home';
import { remote } from './remote';
import { settings } from './settings';

export const EN: Record<string, Entry> = { ...common, ...home, ...chat, ...settings, ...remote };
