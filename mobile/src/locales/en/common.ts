import type { Entry } from './index';

/** State words, as the desktop says them (crates/fibo-core/locales/en/core.toml). */
export const common: Record<string, Entry> = {
  '连接长时间没有传输进展，正在重连': 'No transfer progress for too long. Reconnecting…',
  '本地数据格式不正确': 'The local data has an invalid format',

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
  等你批准: 'Waiting for approval',
  终端: 'Terminal',
  要你处理: 'Needs you',
  先不管: 'Quiet',
  接着管: 'Resume',
  未开始: 'Not started',
  进行中: 'In progress',
  会话都做完了: 'Sessions done',
  验收齐了: 'All checks met',
  有进展: 'Has progress',
  等你确认: 'Awaiting your check',
  搁着: 'Idle',
  已完成: 'Completed',
  已放弃: 'Abandoned',
};
