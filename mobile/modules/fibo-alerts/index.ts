// The Android side of 后台保持连接 and its notifications; null anywhere else (iOS, web, tests).
import { requireOptionalNativeModule } from 'expo';

type FiboAlerts = {
  /** False when the system refused: a foreground service starts only with the app in front. */
  startKeepConnected(): boolean;
  stopKeepConnected(): void;
  show(tag: string, title: string, body: string, quiet: boolean): void;
  hide(tag: string): void;
  /** Whether this app may put up notifications at all. */
  enabled(): boolean;
  /** Whether the system's battery limits leave this app alone. ColorOS cuts the connection otherwise. */
  batteryFree(): boolean;
  /** The system's own question; the answer shows in `batteryFree` when the app comes back. */
  askBatteryFree(): void;
};

export default requireOptionalNativeModule<FiboAlerts>('FiboAlerts');
