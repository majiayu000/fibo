import { expect, test } from 'bun:test';
import { chatDraft, keepChatDraft } from '../src/store/chatDraft';

// The box used to hold its own words: back to the home screen and in again, they were gone,
// and a deep link to another conversation on the same screen carried them along (11-18 walk).
test("what is written in a conversation's box stays with it, and is there on the way back", () => {
  const picture = { uri: 'file:///p.jpg', base64: 'AAAA' };
  keepChatDraft('mac', 'claude-1', { text: '先别合并，等我看完', pictures: [picture] });

  expect(chatDraft('mac', 'codex-2')).toEqual({ text: '', pictures: [] });
  expect(chatDraft('other-mac', 'claude-1')).toEqual({ text: '', pictures: [] });

  // The other conversation's box is written in and sent; the first one's is untouched.
  keepChatDraft('mac', 'codex-2', { text: 'hi', pictures: [] });
  keepChatDraft('mac', 'codex-2', { text: '', pictures: [] });

  expect(chatDraft('mac', 'claude-1')).toEqual({ text: '先别合并，等我看完', pictures: [picture] });
  expect(chatDraft('mac', 'codex-2')).toEqual({ text: '', pictures: [] });
});

test('sent, the box is empty the next time too', () => {
  keepChatDraft('mac', 'sent', { text: '发出去的话', pictures: [] });
  keepChatDraft('mac', 'sent', { text: '', pictures: [] });
  expect(chatDraft('mac', 'sent').text).toBe('');
});
