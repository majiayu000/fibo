// The lock said as the phone has it (2026-11-27 walk: an OPPO that lets apps use only its
// fingerprint showed a face and 「Face ID」, then asked for a finger).
import { expect, test } from 'bun:test';
import { cancelledUnseen, lockChoices, lockWay, wayIcon, wayWords, type LockKit } from '../src/ui/lockway';

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

test('a cancel faster than a finger is the system, asked once more', () => {
  expect(cancelledUnseen('user_cancel', 120)).toBe(true);
  expect(cancelledUnseen('app_cancel', 300)).toBe(true);
  expect(cancelledUnseen('user_cancel', 2500)).toBe(false);
  expect(cancelledUnseen('lockout', 100)).toBe(false);
});
