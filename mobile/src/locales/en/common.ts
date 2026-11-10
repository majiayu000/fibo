import type { Entry } from './index';

/** State words, as the desktop says them (crates/mooring-core/locales/en/core.toml). */
export const common: Record<string, Entry> = {
  等你回答: 'Waiting for you',
  出错了: 'Error',
  在跑: 'Running',
  等后台: 'In background',
  等你验收: 'Ready for review',
  新开的: 'New',
  停下了: 'Stopped',
  很久没动: 'Idle',
  稍后再看: 'Snoozed',
  做完了: 'Done',
  等你审批: 'Needs approval',
  终端: 'Terminal',
  要你处理: 'Needs you',
  先不管: 'Quiet',
  接着管: 'Resume',
  未开始: 'Not started',
  进行中: 'In progress',
  会话都做完了: 'Sessions done',
  验收齐了: 'All checks met',
  已完成: 'Completed',
  已放弃: 'Abandoned',
};
