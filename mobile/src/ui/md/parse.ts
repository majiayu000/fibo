// An Agent reply turned into a Markdown tree, once. Pure, so tests and the corpus tool run
// it without React Native. See docs/phone-app/markdown.md for why this parser.
import type { Root, RootContent } from 'mdast';

/** Codex's memory citations end a reply; they are for its own bookkeeping, not for reading. */
const CITATION = /^[ \t]*<oai-mem-citation>[\s\S]*?(?:<\/oai-mem-citation>[ \t]*$|(?![\s\S]))/gm;

/** What a reply says once its bookkeeping is gone: what is drawn, and what copying gives. */
export function readable(text: string): string {
  return text.includes('<oai-mem-citation>') ? text.replace(CITATION, '').trimEnd() : text;
}

/** Nothing here can mean Markdown, so the parser has nothing to add: half of all replies. */
const MARKUP = /[*_`~[\]<>|#&\\!@]|^\s*(?:\d+[.)]|[-+=])(?:\s|$)|https?:|www\./m;

type Parser = { parse: (text: string) => Root };
let parser: Parser | null = null;

/** Loaded on the first reply that needs it, not when the app starts. */
function remark(): Parser {
  if (!parser) {
    const { unified } = require('unified') as typeof import('unified');
    const parse = (require('remark-parse') as typeof import('remark-parse')).default;
    const gfm = (require('remark-gfm') as typeof import('remark-gfm')).default;
    const cjk = (require('remark-cjk-friendly') as typeof import('remark-cjk-friendly')).default;
    parser = unified().use(parse).use(gfm).use(cjk).freeze() as unknown as Parser;
  }
  return parser;
}

function plain(text: string): Root {
  const paragraphs = text.split(/\n[ \t]*\n/).filter((p) => p.trim());
  return {
    type: 'root',
    children: paragraphs.map((p): RootContent => ({ type: 'paragraph', children: [{ type: 'text', value: p.trim() }] })),
  };
}

export function parseMarkdown(text: string): Root {
  const src = readable(text.replace(/\r\n/g, '\n'));
  return MARKUP.test(src) ? remark().parse(src) : plain(src);
}

const CACHE_MAX = 400;
const cache = new Map<string, Root>();

/** The tree for a reply, from memory when it was drawn before: lists draw a row many times. */
export function parsed(text: string): Root {
  const hit = cache.get(text);
  if (hit) return hit;
  const root = parseMarkdown(text);
  if (cache.size >= CACHE_MAX) cache.delete(cache.keys().next().value as string);
  cache.set(text, root);
  return root;
}

/** A link opens only when the phone can open it; anything else names a file on the computer. */
export const openable = (url: string) => /^(?:https?:|mailto:)/i.test(url);

/** A file link's target as the path a person would paste: no `file://`, no URL escapes. */
export function pathOf(url: string): string {
  const bare = url.replace(/^file:\/\//i, '');
  try {
    return decodeURI(bare);
  } catch {
    return bare;
  }
}
