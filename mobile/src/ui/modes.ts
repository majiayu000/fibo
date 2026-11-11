import type { AgentControls } from '../remote/hub';
// The words around changing a running Agent's model or permission mode. What is offered and what
// is on come from the computer, which reads them off the CLI; a phone counts no keys.
import { notUi, tr } from '../i18n';

export type ModeKind = 'model' | 'permission' | 'effort';

/** CLIs whose effort the computer reads and sets (host `agents/modes_effort.rs`). */
export const hasEffort = (controls: AgentControls | null | undefined) => !!controls?.effort;

export type ModeOption = { key: string; label: string; detail?: string; current?: boolean; full?: boolean };
export type ModeOptions = { options: ModeOption[]; note?: string };

/** The word the computer uses on a card for the mode that stops asking. */
export const FULL_WORDS = notUi('全部放行');

export const isFull = (full: boolean | null | undefined) => full === true;

/**
 * Whether a list row says so: only when it is the exception. Most of a person's sessions may run
 * this way from the start (the first real inbox had the word on all 24), and then it says nothing
 * and takes the room of what each is doing. The chat's own tag always says.
 */
export const rowSaysFull = (card: { permission_full?: boolean | null }, cards: { permission_full?: boolean | null }[]) =>
  isFull(card.permission_full) && cards.some((c) => c.permission_full === false);

/** What the second confirmation says: the consequence, in so many words. Getters, so the words follow the language. */
export const FULL_CONFIRM = {
  get title() {
    return tr('切到「全部放行」？');
  },
  get detail() {
    return tr('它之后跑命令、改文件都不再问你，手机上也不会再有审批卡片。这次会话一直这样，直到你切回来。');
  },
  get ok() {
    return tr('切到全部放行');
  },
};

export const kindName = (kind: ModeKind) => ({ model: tr('模型'), permission: tr('权限'), effort: tr('强度') })[kind];
