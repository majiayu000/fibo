// Where a new task lands: what the phone shows is what it sends, and a folder in no space
// says where it is going instead of quietly going to the first space.
import { describe, expect, test } from 'bun:test';
import { asideSpaces, holds, ignoredPick, landing, landingNote, pickDir, pickSpace, spaceOf } from '../src/ui/spaces';

const work = { id: 'w', name: '工作空间', cwd: '/Users/me' };
const app = { id: 'a', name: 'mooring', cwd: '/Users/me/code/mooring' };
const mobile = { id: 'm', name: 'phone', cwd: '/Users/me/code/mooring/mobile' };
const spaces = [work, app, mobile];

describe('which space holds a folder', () => {
  test('by whole folder names, the closest one', () => {
    expect(holds('/a/b', '/a/b/c')).toBe(true);
    expect(holds('/a/b/', '/a/b')).toBe(true);
    expect(holds('/a/b', '/a/bc')).toBe(false);
    expect(holds('', '/a')).toBe(false);
    expect(spaceOf(spaces, '/Users/me/code/mooring/mobile/src')?.id).toBe('m');
    expect(spaceOf(spaces, '/Users/me/code/mooringx')?.id).toBe('w');
    expect(spaceOf([app], '/tmp')).toBeUndefined();
  });
});

describe('where it lands', () => {
  test('what you picked, else the folder, else the first', () => {
    expect(landing(spaces, 'a', '/tmp')).toEqual({ space: app, why: 'picked' });
    expect(landing(spaces, undefined, '/Users/me/code/mooring/crates')).toEqual({ space: app, why: 'folder' });
    expect(landing([app, mobile], undefined, '/tmp')).toEqual({ space: app, why: 'first' });
    expect(landing(spaces, 'gone', '/Users/me/code/mooring')).toEqual({ space: app, why: 'folder' });
    expect(landing([], undefined, '/tmp').space).toBeUndefined();
  });
  test('a folder in none says so, before it opens', () => {
    expect(landingNote(landing([app], undefined, '/tmp/x'), '/tmp/x')).toContain('不在任何空间里');
    expect(landingNote(landing([app], 'a', '/tmp/x'), '/tmp/x')).toContain('也会放进这个空间');
    expect(landingNote(landing(spaces, undefined, '/Users/me/code/mooring'), '/Users/me/code/mooring')).toBe('');
    expect(landingNote(landing([], undefined, '/tmp/x'), '/tmp/x')).toContain('第一个');
    expect(landingNote(landing([], undefined, undefined), undefined)).toBe('');
  });
});

describe('picking', () => {
  test('a space brings its folder, unless the directory is already inside it', () => {
    expect(pickSpace(app, '/tmp')).toEqual({ space: 'a', cwd: app.cwd, reset: true });
    expect(pickSpace(app, '/Users/me/code/mooring/crates')).toEqual({ space: 'a', cwd: '/Users/me/code/mooring/crates', reset: false });
    expect(pickSpace({ id: 'n', name: 'n', cwd: '' }, '/tmp')).toEqual({ space: 'n', cwd: '/tmp', reset: false });
  });
  test('a directory moves the space to the one holding it, else keeps the pick', () => {
    expect(pickDir(spaces, 'a', '/Users/me/code/mooring/mobile')).toBe('m');
    expect(pickDir([app], 'a', '/tmp')).toBe('a');
    expect(pickDir([app], undefined, '/tmp')).toBeUndefined();
  });
});

describe('an older computer that does not read the space', () => {
  test('says where it went when that is not what was shown', () => {
    expect(ignoredPick([work, app], 'a', '/tmp', {})).toContain('工作空间');
    expect(ignoredPick([work, app], 'a', '/Users/me/code/mooring', {})).toBe('');
    expect(ignoredPick([work, app], 'a', '/tmp', { space: 'a' })).toBe('');
  });
});

describe('the spaces put aside', () => {
  test('every online computer names its own, by name; one gone away or saying none lists none', () => {
    const on = { name: 'mac', status: 'online', quietSpaces: [{ id: 'y', name: 'youtube', cwd: '/y' }, { id: 'h', name: 'her', cwd: '/h' }] };
    const old = { name: 'old', status: 'online' };
    const away = { name: 'away', status: 'offline', quietSpaces: [{ id: 'v', name: 'vsr', cwd: '/v' }] };
    expect(asideSpaces([on, old, away]).map((a) => `${a.m.name}/${a.name}`)).toEqual(['mac/her', 'mac/youtube']);
  });
});
