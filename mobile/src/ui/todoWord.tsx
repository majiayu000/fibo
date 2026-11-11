// A todo's state in the computer's words, coloured: yours to judge, being worked on, quiet.
import { StyleSheet, Text } from 'react-native';
import { hostWords } from '../i18n';
import type { Todo } from '../remote/hub';
import type { Theme } from './theme';
import { todoTone } from './todos';

export function StateWord({ todo, t }: { todo: Todo; t: Theme }) {
  if (!todo.state_label) return null;
  const word = hostWords(todo.state_label);
  const tone = todoTone(todo);
  const fg = tone === 'ok' ? t.ok : tone === 'run' ? t.run : t.dim;
  const bg = tone === 'ok' ? t.okBg : t.well;
  return <Text style={[s.state, { color: fg, backgroundColor: bg }]}>{word}</Text>;
}

const s = StyleSheet.create({
  state: { fontSize: 12, borderRadius: 6, paddingHorizontal: 6, paddingVertical: 1, overflow: 'hidden', marginTop: 2 },
});
