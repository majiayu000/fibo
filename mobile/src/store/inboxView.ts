// How you left the home screen, kept on this phone only: which groups are unfolded.
import { useSyncExternalStore } from 'react';
import { persisted } from './kv';

type InboxView = { review: boolean; running: boolean };

const view = persisted<InboxView>('mooring.inbox', { review: false, running: true });

export const useInboxView = () => useSyncExternalStore(view.subscribe, view.get);
export const setInboxView = (patch: Partial<InboxView>) => view.set(patch);
