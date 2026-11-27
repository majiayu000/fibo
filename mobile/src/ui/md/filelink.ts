// What a tap on a link in a chat does: a web address opens in the system, a Markdown or
// plain-text file on the computer opens in the preview (`app/file.tsx`), anything else
// copies its path. Why: docs/decisions/2026-11-27-phone-file-link-preview.md.
import { openable, pathOf } from './parse';

export type LinkTap = { kind: 'open'; url: string } | { kind: 'preview'; path: string } | { kind: 'copy'; path: string };

/** A file's path without the `#L12` an Agent adds to point at a line. */
const withoutAnchor = (path: string) => path.replace(/#[^/]*$/, '');

/** The files the computer's `file` op previews; it checks again on its side. */
export const previewable = (path: string) => /\.(?:md|markdown|txt)$/i.test(withoutAnchor(path));

/** `canPreview`: the screen knows which conversation the link is in, so which folder. */
export function tapOf(url: string, canPreview: boolean): LinkTap {
  if (openable(url)) return { kind: 'open', url };
  const path = pathOf(url);
  if (canPreview && previewable(path)) return { kind: 'preview', path: withoutAnchor(path) };
  return { kind: 'copy', path };
}
