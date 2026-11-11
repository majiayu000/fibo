import { expect, test } from 'bun:test';
import type { AgentControls } from '../src/remote/hub';
import { HOW_NOTE, QUEUED_TAP, sendsNow, steerable, whileBusy } from '../src/ui/busy';

const queues: AgentControls = { steer: true, send_now: true, effort: true, busy_input: 'queue' };
const steers: AgentControls = { ...queues, busy_input: 'steer' };
const unknown: AgentControls = { steer: false, send_now: false, effort: false };

test('the host describes what Enter does, without a phone-side brand list', () => {
  expect(whileBusy(queues).note).toContain('排队');
  expect(whileBusy(steers).note).toContain('插进去');
  expect(whileBusy(steers).note).not.toContain('排队');
  for (const other of [unknown, null, undefined]) {
    expect(whileBusy(other).note).not.toContain('排队');
    expect(whileBusy(other).note).not.toContain('插进去');
  }
});

test('controls come from the host, including unknown future Agents and old hosts', () => {
  expect(steerable(queues) && steerable(steers)).toBe(true);
  expect(steerable(unknown) || steerable(null)).toBe(false);
  expect(sendsNow(queues)).toBe(true);
  expect(sendsNow({ ...queues, send_now: false })).toBe(false);
  for (const other of [unknown, null, undefined]) expect(sendsNow(other)).toBe(false);
  expect(HOW_NOTE.queue).toContain('排队');
  expect(HOW_NOTE.steer).toContain('插进去');
  expect(QUEUED_TAP).toContain('立刻发送');
});
