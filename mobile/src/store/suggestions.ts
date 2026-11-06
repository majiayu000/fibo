// This phone's counts of Claude's next-prompt suggestions; see `suggestionCounts`.
import { useSyncExternalStore } from 'react';
import { persisted } from './kv';
import { NONE, type SuggestionStats } from './suggestionCounts';

export const suggestionStats = persisted<SuggestionStats>('mooring.suggestions', NONE);
export const useSuggestionStats = () => useSyncExternalStore(suggestionStats.subscribe, suggestionStats.get);
