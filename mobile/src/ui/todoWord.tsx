// A todo's state in the computer's words, coloured: yours to judge, being worked on, quiet.
import { StyleSheet, Text } from 'react-native';
import { tk, tr } from '../i18n';
import type { Todo } from '../remote/hub';
import type { Theme } from './theme';
import { todoTone } from './todos';

/** By the host's key, in the phone's language; an unknown key keeps the host's label. */
const WORDS: Record<string, string> = {
  not_started: tk('未开始'), working: tk('进行中'), settled: tk('会话都做完了'), proven: tk('验收齐了'),
  completed: tk('已完成'), deferred: tk('稍后再看'), abandoned: tk('已放弃'),
};

export function StateWord({ todo, t }: { todo: Todo; t: Theme }) {
  const key = todo.state;
  const word = key && WORDS[key] ? tr(WORDS[key]) : todo.state_label;
  if (!word) return null;
  const tone = todoTone(todo);
  const fg = tone === 'ok' ? t.ok : tone === 'run' ? t.run : t.dim;
  const bg = tone === 'ok' ? t.okBg : t.well;
  return <Text style={[s.state, { color: fg, backgroundColor: bg }]}>{word}</Text>;
}

const s = StyleSheet.create({
  state: { fontSize: 12, borderRadius: 6, paddingHorizontal: 6, paddingVertical: 1, overflow: 'hidden', marginTop: 2 },
});
