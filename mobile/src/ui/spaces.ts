// Which space a new task lands in. The computer used to decide alone: the space whose folder
// holds the directory, else its first space, so a folder in none went to 工作空间 and the
// phone never said so. Now the phone shows where it goes and sends the space it shows.
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
  if (!l.space) return '电脑上还没有空间，这个文件夹会成为第一个';
  if (holds(l.space.cwd, cwd)) return '';
  if (l.why === 'first') return `这个文件夹不在任何空间里，会放进「${l.space.name}」`;
  return `文件夹不在「${l.space.name}」的目录里，也会放进这个空间`;
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
  return `电脑上的 Mooring 版本较旧，任务放进了「${went.name}」。换新版后可以选空间`;
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
