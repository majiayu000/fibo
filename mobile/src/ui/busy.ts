// What a message sent while the Agent works becomes, per CLI: the phone pastes it and presses
// Enter, and each CLI decides. Measured on the real CLIs (docs/phone-app/cli-turn-input.md):
// Claude Code queues it for after the step at hand, Codex puts it into the step now. A CLI not
// measured is not told either way.
import { tk } from '../i18n';

/** Words here are `tk` keys: say them with `tr` where shown. */
export type BusyWords = {
  /** The composer's placeholder while it works. */
  placeholder: string;
  /** Under the message just sent, until the log has it. */
  note: string;
};

const QUEUES: BusyWords = {
  placeholder: tk('它还在做事：现在发的，做完手头这一步就会看到'),
  note: tk('已排队 · 它做完手头这一步就会看到'),
};

const STEERS: BusyWords = {
  placeholder: tk('它还在做事：现在发的会马上插进这一步'),
  note: tk('已插进去 · 它这一步里就会看到'),
};

const UNKNOWN: BusyWords = {
  placeholder: tk('它还在做事：现在发的会交给它'),
  note: tk('已发给它 · 它还在做事'),
};

export type How = 'queue' | 'steer';

/** Said under a message sent with a `how` the computer pressed the key for. */
export const HOW_NOTE: Record<How, string> = { queue: QUEUES.note, steer: STEERS.note };

/** Whether this CLI was measured to queue and steer on keys the computer knows (host `turn_input`). */
export function steerable(agent: string | null | undefined) {
  return agent === 'claude' || agent === 'codex';
}

/**
 * Whether the Agent sends its whole queue now on its steer key with nothing typed: Claude Code
 * says `ctrl+enter to send now` under a queued message, measured 2026-11-09. Codex not yet.
 */
export function sendsNow(agent: string | null | undefined) {
  return agent === 'claude';
}

/** Under a queued message the phone can send now; after that is done. */
export const QUEUED_TAP = tk('排队中 · 点一下可以立刻发送');
export const SENT_NOW = tk('已立刻发送 · 它马上会看到');

/** `agent` as cards name it: `claude`, `codex`… */
export function whileBusy(agent: string | null | undefined): BusyWords {
  if (agent === 'claude') return QUEUES;
  if (agent === 'codex') return STEERS;
  return UNKNOWN;
}
