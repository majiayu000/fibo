import { expect, test } from 'bun:test';
import { tapOf } from '../src/ui/md/filelink';

test('a file link in a chat opens the preview, a web link the system, other files copy', () => {
  expect(tapOf('https://fibo.page', true)).toEqual({ kind: 'open', url: 'https://fibo.page' });
  expect(tapOf('docs/phone%20unlock.md', true)).toEqual({ kind: 'preview', path: 'docs/phone unlock.md' });
  expect(tapOf('file:///Users/me/p/README.MD#L12', true)).toEqual({ kind: 'preview', path: '/Users/me/p/README.MD' });
  expect(tapOf('notes.txt', true)).toEqual({ kind: 'preview', path: 'notes.txt' });
  expect(tapOf('src/main.rs', true)).toEqual({ kind: 'copy', path: 'src/main.rs' });
  // Nowhere to read it from: copied, as before.
  expect(tapOf('docs/plan.md', false)).toEqual({ kind: 'copy', path: 'docs/plan.md' });
});
