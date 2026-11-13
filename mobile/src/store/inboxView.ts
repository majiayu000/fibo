// How you left the home screen, kept on this phone only: which groups are unfolded, which space shows.
import { useSyncExternalStore } from 'react';
import { remembered } from './kv';

type InboxView = { review: boolean; running: boolean; space: string | null; quiet: boolean };

const view = remembered<InboxView>('mooring.inbox', { review: false, running: true, space: null, quiet: false });

export const useInboxView = () => useSyncExternalStore(view.subscribe, view.get);
export const setInboxView = (patch: Partial<InboxView>) => view.set(patch);
