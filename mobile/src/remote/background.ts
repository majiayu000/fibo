// 后台保持连接 on Android: a foreground service keeps this process and its encrypted connection
// alive with the app closed, and what the cards say becomes a notification made here, on the phone.
// No account or push service is involved. docs/decisions/2026-11-07-phone-push-channel.md says why.
import { AppRegistry, AppState, PermissionsAndroid, Platform } from 'react-native';
import native from '../../modules/fibo-alerts';
import { setLanguage } from '../i18n';
import { persisted } from '../store/kv';
import { settings } from '../store/settings';
import { followAlerts, keepsConnected, type Memory, NO_MEMORY } from './alerts';
import { allMachines, start, watch } from './hub';

/** Same name in `KeepConnectedService.kt`. */
const TASK = 'FiboKeepConnected';

export const canKeepConnected = Platform.OS === 'android' && !!native;

const memory = persisted<Memory>('mooring.alerts', NO_MEMORY);
let following = false;
let release: (() => void) | null = null;

/** Starts following the hub, once per process, and brings the service up if it was left on. */
export async function beginAlerts() {
  if (following || !native) return;
  following = true;
  const service = native;
  await Promise.all([settings.ready, memory.ready]);
  // The root layout does this for the window; a service started with no window has only this,
  // and a notification's words follow the language chosen in settings like every other word.
  const speak = () => setLanguage(settings.get().language);
  speak();
  settings.subscribe(speak);
  followAlerts({
    memory: memory.get(),
    machines: allMachines,
    subscribe: watch,
    context: () => {
      const v = settings.get();
      return { notify: v.notify, silent: !keepsConnected(v) || AppState.currentState === 'active', hideBody: v.lock, at: new Date() };
    },
    save: (m) => memory.set(m),
    show: (a) => service.show(a.tag, a.title, a.body, !!a.quiet),
    hide: (tag) => service.hide(tag),
  });
  if (keepsConnected(settings.get())) service.startKeepConnected();
}

/** What the service runs: the hub, for as long as the switch stays on. */
export function registerBackgroundTask() {
  AppRegistry.registerHeadlessTask(TASK, () => async () => {
    await start();
    await beginAlerts();
    // Timers stop in the background unless a task is running (JavaTimerManager), and the
    // connection's heartbeat and retries are timers: this task holds them open.
    await new Promise<void>((done) => {
      release = done;
    });
  });
}

/** Without this ColorOS closes the connection seconds after the app leaves the screen. */
export const batteryFree = () => native?.batteryFree() ?? true;
export const askBatteryFree = () => native?.askBatteryFree();

/** Whether the switch ended up on: turning it on needs the permission to notify. */
export async function setKeepConnected(on: boolean): Promise<boolean> {
  if (!native) return false;
  if (!on) {
    await settings.set({ backgroundAlerts: false });
    release?.();
    release = null;
    native.stopKeepConnected();
    return false;
  }
  const asked = await PermissionsAndroid.request(PermissionsAndroid.PERMISSIONS.POST_NOTIFICATIONS);
  // Before Android 13 there is nothing to grant and it answers NEVER_ASK_AGAIN: ask the system.
  if (asked !== PermissionsAndroid.RESULTS.GRANTED && !native.enabled()) return false;
  await settings.set({ backgroundAlerts: true });
  const started = native.startKeepConnected();
  if (started && !native.batteryFree()) native.askBatteryFree();
  return started;
}
