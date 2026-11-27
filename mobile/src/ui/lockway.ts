// How this phone proves it is you, said the way the phone has it. Until 2026-11-27 every phone
// read 「Face ID / 指纹」 and the lock page drew a face; on an Android phone that lets apps use
// only its fingerprint, the page looked like face unlock and then asked for a finger.
// Android lets an app choose how strict, not which one: the system's own box shows whatever
// the phone allows at that strictness, and its screen lock as the fallback. So the choice
// offered is strict or lenient, and only where the two differ. Pure, so tests run it.
import { tr } from '../i18n';

/** `strong`: fingerprint and the like (Android class 3); `weak` adds camera face unlock (class 2). */
export type LockWay = 'strong' | 'weak';

/** What the phone has. `level`: 0 nothing, 1 screen lock only, 2 weak biometric, 3 strong. */
export type LockKit = { ios: boolean; androidApi?: number; level: number; finger: boolean; face: boolean };

// Android 9/10 cannot combine Class 3 biometrics with device credentials in one prompt.
function supportsStrict(k: LockKit): boolean {
  return k.androidApi !== 28 && k.androidApi !== 29;
}

/** The ways to offer: two only on an Android phone with face hardware, where lenient may add it. */
export function lockChoices(k: LockKit): LockWay[] {
  return !k.ios && supportsStrict(k) && k.face && k.level >= 2 ? ['strong', 'weak'] : [];
}

/** Keep screen lock fallback on Android 9/10 and use the saved choice where offered. */
export function lockWay(saved: LockWay | undefined, k: LockKit): LockWay {
  if (!supportsStrict(k)) return 'weak';
  return saved && lockChoices(k).includes(saved) ? saved : 'strong';
}

/** What it is called: 「Face ID」 only on an iPhone that has it. */
export function wayWords(way: LockWay, k: LockKit): string {
  if (k.ios) return k.face ? 'Face ID' : k.finger ? 'Touch ID' : tr('锁屏密码');
  if (k.level < 2 || (way === 'strong' && k.level < 3)) return tr('锁屏密码');
  if (way === 'weak' && k.face) return k.finger ? tr('指纹或面容') : tr('面容');
  return k.finger ? tr('指纹') : tr('生物识别');
}

/** One line on what a way means, for the settings page. */
export function wayNote(way: LockWay, k: LockKit): string {
  if (k.ios) return tr('认不出时可以输锁屏密码');
  return way === 'weak' ? tr('手机允许时也能用面容；普通摄像头的面容不如指纹安全') : tr('只认安全级别高的方式；认不出时可以输锁屏密码');
}

/** A face only where the phone really looks at one; a lock everywhere else. */
export function wayIcon(way: LockWay, k: LockKit): 'face' | 'lock' {
  return k.face && (k.ios || (way === 'weak' && k.level >= 2 && !k.finger)) ? 'face' : 'lock';
}

/** A prompt cancelled this quickly on Android foreground return may not have been shown. */
export const UNSEEN_MS = 800;

/** Asked as the app comes back to the front, the system's box can be cancelled before it shows:
 *  back from the background on 2026-11-27 there was only 「解锁」 and no box. Android reports
 *  that as the same `user_cancel` a person's 取消 gives. Only a foreground attempt retries. */
export function cancelledUnseen(error: string | undefined, ms: number): boolean {
  return (error === 'user_cancel' || error === 'app_cancel') && ms < UNSEEN_MS;
}

/** A way's words inside a Chinese sentence: 「打开时要指纹」 but 「打开时要 Face ID」, a space
 *  only around Latin words (2026-11-27: 「打开 指纹 锁」 read as three words). English trims it. */
export function inline(words: string): string {
  return /^[A-Za-z]/.test(words) ? ` ${words} ` : words;
}
