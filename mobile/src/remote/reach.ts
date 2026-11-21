// After a dial fails with no close code (the relay turned the socket away before it opened, or
// the network never reached it), one look at the relay's `/healthz` tells whether the relay is
// there at all. The phone asks nobody but the relay it is paired through.
import type { RelayReach } from './close';
import { relayHttpBase } from './link';

type Fetch = (url: string, init?: { signal?: AbortSignal }) => Promise<{ ok: boolean }>;

/** How long the health check may take before it counts as no answer. */
export const REACH_TIMEOUT_MS = 4000;

export async function reachRelay(relay: string, get: Fetch = fetch, timeoutMs = REACH_TIMEOUT_MS): Promise<RelayReach> {
  let url: string;
  try {
    url = `${relayHttpBase(relay)}/healthz`;
  } catch {
    return 'unknown';
  }
  const stop = new AbortController();
  const timer = setTimeout(() => stop.abort(), timeoutMs);
  try {
    const r = await get(url, { signal: stop.signal });
    return r.ok ? 'up' : 'sick';
  } catch {
    return 'none';
  } finally {
    clearTimeout(timer);
  }
}
