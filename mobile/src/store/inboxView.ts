// How you left the inbox, kept on this phone only: whether 等你验收 is unfolded, which space shows.
import { useSyncExternalStore } from 'react';
import { persisted } from './kv';

type InboxView = { review: boolean; space: string | null };

const view = persisted<InboxView>('mooring.inbox', { review: false, space: null });

export const useInboxView = () => useSyncExternalStore(view.subscribe, view.get);
export const setInboxView = (patch: Partial<InboxView>) => view.set(patch);
