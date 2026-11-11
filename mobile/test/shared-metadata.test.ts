import { expect, test } from 'bun:test';
import { agentName } from '../src/ui/status';
import { cardTitle } from '../src/ui/format';
import type { Card } from '../src/remote/hub';

test('host names and final titles survive unknown agents and unbound terminals', () => {
  expect(agentName({ agent: 'dsh', agent_name: 'DeepSeek Harness' })).toBe('DeepSeek Harness');
  expect(agentName({ agent: 'qoder', agent_name: 'Qoder CLI' })).toBe('Qoder CLI');
  expect(agentName({ agent: 'future', agent_name: 'Any custom name' })).toBe('Any custom name');
  const c: Card = { id: 't', space: 's', tab: 'wrong tab', cwd: '/wrong/folder', phase: 'shell', note: 'wrong note', title: 'host title' };
  expect(cardTitle(c)).toBe('host title');
  expect(cardTitle({ ...c, title: null })).toBe('t');
  expect(agentName({ agent: 'dsh' })).toBe('dsh');
});
