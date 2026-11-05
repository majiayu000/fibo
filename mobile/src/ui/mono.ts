import { Platform } from 'react-native';

/** The one monospace face, bundled: Android's `monospace` is not monospaced on every phone (OPPO). */
export const mono = Platform.OS === 'web' ? 'ui-monospace, Menlo, monospace' : 'JetBrainsMonoNL-Regular';
