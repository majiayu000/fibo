// The lock said as the phone has it (2026-11-27 walk: an OPPO that lets apps use only its
// fingerprint showed a face and 「Face ID」, then asked for a finger).
import { expect, test } from 'bun:test';
import { cancelledUnseen, inline, lockChoices, lockWay, wayIcon, wayWords, type LockKit } from '../src/ui/lockway';

const oppo: LockKit = { ios: false, level: 3, finger: true, face: true };
const pixelNoFace: LockKit = { ios: false, level: 3, finger: true, face: false };
const faceOnly: LockKit = { ios: false, level: 2, finger: false, face: true };
const pinOnly: LockKit = { ios: false, level: 1, finger: false, face: false };
const iphone: LockKit = { ios: true, level: 3, finger: false, face: true };

test('an Android phone with a fingerprint reads 指纹 and draws a lock, never Face ID', () => {
  expect([wayWords('strong', oppo), wayIcon('strong', oppo)]).toEqual(['指纹', 'lock']);
  expect(wayWords('strong', pinOnly)).toBe('锁屏密码');
  expect([wayWords('weak', faceOnly), wayIcon('weak', faceOnly)]).toEqual(['面容', 'face']);
});

test('only an iPhone with Face ID says Face ID and draws a face', () => {
  expect([wayWords('strong', iphone), wayIcon('strong', iphone)]).toEqual(['Face ID', 'face']);
  expect(wayWords('strong', { ...iphone, face: false, finger: true })).toBe('Touch ID');
});

test('a choice only where strict and lenient differ, and a choice the phone lost reads strict', () => {
  expect(lockChoices(oppo)).toEqual(['strong', 'weak']);
  expect(lockChoices(pixelNoFace)).toEqual([]);
  expect(lockChoices(iphone)).toEqual([]);
  expect(lockWay('weak', oppo)).toBe('weak');
  expect(lockWay('weak', pixelNoFace)).toBe('strong');
  expect(lockWay(undefined, oppo)).toBe('strong');
});

test('Android 9 and 10 use the supported device credential prompt and describe the actual options', () => {
  const android9 = { ...oppo, androidApi: 28 };
  const android10 = { ...pinOnly, androidApi: 29, face: true };
  expect(lockChoices(android9)).toEqual([]);
  expect(lockWay('strong', android9)).toBe('weak');
  expect([wayWords('weak', android9), wayIcon('weak', android9)]).toEqual(['指纹或面容', 'lock']);
  expect(lockChoices(android10)).toEqual([]);
  expect(lockWay(undefined, android10)).toBe('weak');
  expect([wayWords('weak', android10), wayIcon('weak', android10)]).toEqual(['锁屏密码', 'lock']);
  const android11 = { ...oppo, androidApi: 30 };
  expect(lockChoices(android11)).toEqual(['strong', 'weak']);
  expect(lockWay(undefined, android11)).toBe('strong');
});

test('a quick cancellation may be a prompt that did not appear', () => {
  expect(cancelledUnseen('user_cancel', 120)).toBe(true);
  expect(cancelledUnseen('app_cancel', 300)).toBe(true);
  expect(cancelledUnseen('user_cancel', 2500)).toBe(false);
  expect(cancelledUnseen('lockout', 100)).toBe(false);
});

test('inside a Chinese sentence a Latin name gets spaces, a Chinese one none', () => {
  expect(`打开${inline('指纹')}锁`).toBe('打开指纹锁');
  expect(`打开时要${inline('Face ID')}`.trim()).toBe('打开时要 Face ID');
});
