// Writes named by a request id (docs/request-envelope): when the link drops before the
// answer, the same request goes again once the machine is back, and the machine applies
// it at most once. Kept apart from the hub so it runs without React Native.
import { tk, tr } from '../i18n';
import { RemoteError } from './client';

/** How long a write waits for its machine to come back before saying it is unsure. */
export const WRITE_PATIENCE_MS = 30_000;

/** What a write needs of a connection. */
export type Link = {
  ended: unknown;
  request<T = any>(op: string, fields?: Record<string, unknown>): Promise<T>;
};

/** A key: the error's message is `tr(UNSURE)`, so compare a message with `tr(UNSURE)`. */
export const UNSURE = tk('连接断了，不确定发出去没有；看一眼对话再决定要不要重发');

export function requestId() {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Sends `op` under the supplied persisted request id, or a fresh one for this call.
 * The machine
 * saying no is the answer; only a connection that ended is tried again, every `stepMs`,
 * until `patienceMs` have gone.
 */
export async function named(
  current: () => Link | undefined,
  op: string,
  fields: Record<string, unknown> & { request?: string },
  patienceMs = WRITE_PATIENCE_MS,
  stepMs = 500,
) {
  const request = fields.request ?? requestId();
  const until = Date.now() + patienceMs;
  for (;;) {
    const r = current();
    if (r && !r.ended) {
      try {
        return await r.request(op, { ...fields, request });
      } catch (e) {
        if (!r.ended) throw e;
      }
    }
    if (Date.now() > until) throw new RemoteError(tr(UNSURE));
    await new Promise((done) => setTimeout(done, stepMs));
  }
}
