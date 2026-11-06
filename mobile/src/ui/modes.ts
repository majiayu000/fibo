// The words around changing a running Agent's model or permission mode. What is offered and what
// is on come from the computer, which reads them off the CLI; a phone counts no keys.

export type ModeKind = 'model' | 'permission';

export type ModeOption = { key: string; label: string; detail?: string; current?: boolean; full?: boolean };
export type ModeOptions = { options: ModeOption[]; note?: string };

/** The word the computer uses on a card for the mode that stops asking. */
export const FULL_WORDS = '全部放行';

export const isFull = (permission: string | null | undefined) => permission === FULL_WORDS;

/** What the second confirmation says: the consequence, in so many words. */
export const FULL_CONFIRM = {
  title: `切到「${FULL_WORDS}」？`,
  detail: '它之后跑命令、改文件都不再问你，手机上也不会再有审批卡片。这次会话一直这样，直到你切回来。',
  ok: `切到${FULL_WORDS}`,
};

export const kindName = (kind: ModeKind) => (kind === 'model' ? '模型' : '权限');
