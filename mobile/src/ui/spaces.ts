// Which space a new task lands in. The computer used to decide alone: the space whose folder
// holds the directory, else its first space, so a folder in none went to 工作空间 and the
// phone never said so. Now the phone shows where it goes and sends the space it shows.
import { tr } from '../i18n';

export type Space = { id: string; name: string; cwd: string };

/** `/a/b` holds `/a/b/c` but not `/a/bc`: by whole folder names, as the computer compares. */
export function holds(folder: string, path: string): boolean {
  const f = folder.replace(/\/+$/, '');
  if (!f) return false;
  return path === f || path.startsWith(`${f}/`);
}

/** The space whose folder holds `cwd` most closely, as the computer picks one by itself. */
export function spaceOf(spaces: Space[], cwd?: string): Space | undefined {
  if (!cwd) return undefined;
  return spaces.filter((s) => holds(s.cwd, cwd)).sort((a, b) => b.cwd.length - a.cwd.length)[0];
}

export type Landing = {
  space?: Space;
  /** picked: you chose it. folder: the directory is in it. first: neither, so the first one. */
  why: 'picked' | 'folder' | 'first';
};

/** Where the task goes. A pick that is gone counts as none. */
export function landing(spaces: Space[], picked: string | undefined, cwd: string | undefined): Landing {
  const chosen = picked ? spaces.find((s) => s.id === picked) : undefined;
  if (chosen) return { space: chosen, why: 'picked' };
  const there = spaceOf(spaces, cwd);
  if (there) return { space: there, why: 'folder' };
  return { space: spaces[0], why: 'first' };
}

/** A line under the space when it may surprise: the folder is not that space's own. */
export function landingNote(l: Landing, cwd: string | undefined): string {
  if (!cwd) return '';
  if (!l.space) return tr('电脑上还没有空间，这个文件夹会成为第一个');
  // A space with no folder of its own holds whatever folder it is given.
  if (!l.space.cwd || holds(l.space.cwd, cwd)) return '';
  if (l.why === 'first') return tr('这个文件夹不在任何空间里，会放进「{name}」', { name: l.space.name });
  return tr('文件夹不在「{name}」的目录里，也会放进这个空间', { name: l.space.name });
}

/**
 * Picking a space: its folder becomes the directory, as a new tab there opens in it. One
 * already inside that space's folder stays.
 */
export function pickSpace(s: Space, cwd: string | undefined): { space: string; cwd?: string; reset: boolean } {
  if (cwd && holds(s.cwd, cwd)) return { space: s.id, cwd, reset: false };
  return { space: s.id, cwd: s.cwd || cwd, reset: !!s.cwd };
}

/**
 * Picking a directory: the space whose folder holds it, else what was picked stays. An
 * explicit pick in another space is dropped, since the directory says where it belongs.
 */
export function pickDir(spaces: Space[], picked: string | undefined, cwd: string): string | undefined {
  return spaceOf(spaces, cwd)?.id ?? picked;
}

/**
 * An older computer does not read the space and says none back: it put the tab where the
 * folder decides. Not what was shown: say so.
 */
export function ignoredPick(spaces: Space[], sent: string | undefined, cwd: string | undefined, reply: { space?: string }): string {
  if (!sent || reply.space) return '';
  const went = spaceOf(spaces, cwd) ?? spaces[0];
  if (!went || went.id === sent) return '';
  return tr('电脑上的 Fibo 版本较旧，任务放进了「{name}」。换新版后可以选空间', { name: went.name });
}

/** A space put aside on a computer (先不管), with the computer it is on. */
export type Aside<M> = { m: M; id: string; name: string };

/** Every online computer's spaces put aside, by name; a computer that says none lists none. */
export function asideSpaces<M extends { status: string; quietSpaces?: Space[] }>(machines: M[]): Aside<M>[] {
  return machines
    .filter((m) => m.status === 'online')
    .flatMap((m) => (m.quietSpaces ?? []).map((s) => ({ m, id: s.id, name: s.name })))
    .sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * The space a home-screen chip names, for 新任务 opened while it is shown: on the computer
 * the draft already has when that one has it, else the first online one that does. Chips
 * go by name, the draft by id; a name no computer lists (a folder's) finds nothing. A space
 * with no folder of its own starts where its latest conversation is, not with no directory,
 * skipping conversations in a system temp folder: those are a test's or a script's, and the
 * folder may be gone (a phone walk's test dir took a real task once and closed it with the walk).
 */
const TEMP = ['/tmp/', '/private/tmp/', '/var/folders/', '/private/var/folders/'];
export function temporary(path: string): boolean {
  return TEMP.some((t) => path.startsWith(t) || `${path}/` === t);
}

type Held = { space: string; cwd: string; updated?: number | null };
export function chipSpace<M extends { id: string; status: string; spaceList?: Space[]; cards: Held[] }>(
  machines: M[],
  name: string | null,
  current: string | undefined,
): { machine: string; space: string; cwd?: string } | undefined {
  if (name === null) return undefined;
  const order = [...machines.filter((m) => m.id === current), ...machines.filter((m) => m.id !== current && m.status === 'online')];
  for (const m of order) {
    const s = m.spaceList?.find((x) => x.name === name);
    if (!s) continue;
    if (s.cwd) return { machine: m.id, space: s.id };
    const latest = m.cards.filter((c) => c.space === s.id && c.cwd && !temporary(c.cwd)).sort((a, b) => (b.updated ?? 0) - (a.updated ?? 0))[0];
    return { machine: m.id, space: s.id, ...(latest && { cwd: latest.cwd }) };
  }
  return undefined;
}
