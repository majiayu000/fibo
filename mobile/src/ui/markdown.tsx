// An Agent's reply drawn from its Markdown tree (./md/parse.ts). Built for a list of many:
// no nested text is selectable, no scroll view unless the content needs one, and widths come
// from the text so nothing is measured twice. Why this and not a library: docs/phone-app/markdown.md.
import * as Clipboard from 'expo-clipboard';
import type { Code, List, Nodes, Parents, PhrasingContent, Root, RootContent, Table as TableNode } from 'mdast';
import { type ReactNode, memo, useMemo, useState } from 'react';
import { Linking, Pressable, ScrollView, StyleSheet, Text, View, useWindowDimensions } from 'react-native';
import { mono } from './mono';
import { openable, parsed, pathOf } from './md/parse';
import { columnWidths, monoWidth } from './md/table';
import type { Theme } from './theme';
import { tr } from '../i18n';

/** The chat's side padding (`chat.tsx`), which a block's width is measured from. */
const SIDE_PADDING = 36;
const LIST_INDENT = 20;
const QUOTE_INDENT = 11;
const CODE_PAD = 11;
const FOLD_LINES = 24;
const CELL_SIZE = 13;

/** Marks for what is furniture, not content, so the corpus tool reads only what the Agent said. */
const CHROME = 'md-chrome';

type Ctx = {
  t: Theme;
  color: string;
  width: number;
  fold: boolean;
  onNote?: (note: string) => void;
  defs: Map<string, string>;
};

function follow(url: string, ctx: Ctx) {
  if (openable(url)) {
    Linking.openURL(url).catch(() => undefined);
    return;
  }
  const path = pathOf(url);
  Clipboard.setStringAsync(path).catch(() => undefined);
  ctx.onNote?.(tr('已复制路径 {path}', { path }));
}

function flat(n: Nodes): string {
  if ('value' in n && typeof n.value === 'string') return n.value;
  if (n.type === 'image') return n.alt ?? '';
  return 'children' in n ? (n.children as Nodes[]).map(flat).join('') : '';
}

/** What an image is called when it cannot be shown: its caption, or failing that the file name. */
function caption(alt: string | null | undefined, url: string): string {
  return alt?.trim() || url.split(/[/\\]/).pop() || url;
}

function picture(alt: string | null | undefined, url: string, key: number, ctx: Ctx): ReactNode {
  const name = caption(alt, url);
  if (openable(url))
    return (
      <Text key={key} style={{ color: ctx.t.link }} onPress={() => follow(url, ctx)}>
        {name}
      </Text>
    );
  return (
    <Text key={key} style={{ color: ctx.t.dim }}>
      {name}
    </Text>
  );
}

function phrasing(nodes: PhrasingContent[], ctx: Ctx): ReactNode[] {
  return nodes.map((n, k) => {
    switch (n.type) {
      case 'text':
        return n.value;
      case 'strong':
        return (
          <Text key={k} style={s.strong}>
            {phrasing(n.children, ctx)}
          </Text>
        );
      case 'emphasis':
        return (
          <Text key={k} style={s.em}>
            {phrasing(n.children, ctx)}
          </Text>
        );
      case 'delete':
        return (
          <Text key={k} style={s.del}>
            {phrasing(n.children, ctx)}
          </Text>
        );
      case 'inlineCode':
        return (
          <Text key={k} style={[s.code, { backgroundColor: ctx.t.well }]}>
            {n.value}
          </Text>
        );
      case 'link':
        return (
          <Text key={k} style={{ color: ctx.t.link }} onPress={() => follow(n.url, ctx)}>
            {phrasing(n.children, ctx)}
          </Text>
        );
      case 'linkReference': {
        const url = ctx.defs.get(n.identifier);
        if (!url) return `[${n.children.map(flat).join('')}]`;
        return (
          <Text key={k} style={{ color: ctx.t.link }} onPress={() => follow(url, ctx)}>
            {phrasing(n.children, ctx)}
          </Text>
        );
      }
      case 'image':
        return picture(n.alt, n.url, k, ctx);
      case 'imageReference':
        return picture(n.alt, ctx.defs.get(n.identifier) ?? '', k, ctx);
      case 'break':
        return '\n';
      case 'html':
        return /^<br\s*\/?>$/i.test(n.value.trim()) ? '\n' : n.value;
      case 'footnoteReference':
        return `[${n.label ?? n.identifier}]`;
      default:
        return null;
    }
  });
}

function CodeBlock({ node, ctx }: { node: Code; ctx: Ctx }) {
  const { t, color, width } = ctx;
  const [copied, setCopied] = useState(false);
  const [open, setOpen] = useState(!ctx.fold);
  const lines = useMemo(() => node.value.split('\n'), [node.value]);
  const folded = !open && lines.length > FOLD_LINES + 6;
  const shown = folded ? lines.slice(0, FOLD_LINES).join('\n') : node.value;
  const fits = useMemo(() => monoWidth(shown, 12) <= width - 2 * CODE_PAD - 2, [shown, width]);
  const body = (
    <Text style={[s.pre, { color }]}>{shown}</Text>
  );
  return (
    <View style={[s.codeCard, { backgroundColor: t.well, borderColor: t.line }]}>
      <View style={s.codeHead}>
        <Text testID={CHROME} numberOfLines={1} style={[s.lang, { color: t.dim }]}>
          {node.lang ?? tr('代码')}
        </Text>
        <Pressable
          hitSlop={10}
          accessibilityRole="button"
          accessibilityLabel={tr('复制代码')}
          onPress={() => {
            Clipboard.setStringAsync(node.value).catch(() => undefined);
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          }}
        >
          <Text testID={CHROME} style={[s.lang, { color: copied ? t.ok : t.dim }]}>
            {copied ? tr('已复制') : tr('复制')}
          </Text>
        </Pressable>
      </View>
      {fits ? (
        <View style={s.codeBody}>{body}</View>
      ) : (
        <ScrollView horizontal showsHorizontalScrollIndicator={false} nestedScrollEnabled contentContainerStyle={s.codeBody}>
          <View onStartShouldSetResponder={claim}>{body}</View>
        </ScrollView>
      )}
      {folded && (
        <Pressable onPress={() => setOpen(true)} style={[s.more, { borderColor: t.line }]}>
          <Text testID={CHROME} style={[s.lang, { color: t.link }]}>{tr('展开其余 {n} 行', { n: lines.length - FOLD_LINES })}</Text>
        </Pressable>
      )}
    </View>
  );
}

/**
 * What scrolls sideways takes the touch itself. The message around it copies on long press,
 * and on Android inside the inverted chat list that held the swipe: a wide table or code
 * block did not move (2026-11-12, real phone). Long press on it no longer copies the message.
 */
const claim = () => true;

function TableBlock({ node, ctx }: { node: TableNode; ctx: Ctx }) {
  const { t, color, width } = ctx;
  const widths = useMemo(
    () => columnWidths(node.children.map((r) => r.children.map(flat)), CELL_SIZE, width - 2),
    [node, width],
  );
  const total = widths.reduce((a, b) => a + b, 0);
  const grid = node.children.map((row, r) => (
    <View key={r} style={[s.row, r > 0 && { borderTopWidth: StyleSheet.hairlineWidth, borderColor: t.line }, r === 0 && { backgroundColor: t.well }]}>
      {widths.map((w, c) => {
        const cell = row.children[c];
        return (
          <Text
            key={c}
            style={[s.cell, { width: w, color, textAlign: node.align?.[c] ?? 'left' }, r === 0 && s.strong]}
          >
            {cell ? phrasing(cell.children, ctx) : ''}
          </Text>
        );
      })}
    </View>
  ));
  return (
    <View style={[s.tableCard, { borderColor: t.line }]}>
      {total <= width - 2 ? (
        grid
      ) : (
        <ScrollView horizontal persistentScrollbar nestedScrollEnabled>
          <View onStartShouldSetResponder={claim}>{grid}</View>
        </ScrollView>
      )}
    </View>
  );
}

function ListBlock({ node, ctx }: { node: List; ctx: Ctx }) {
  const start = node.start ?? 1;
  // Marks line up down the list: as wide as the widest number needs.
  const markWidth = node.ordered ? (String(start + node.children.length - 1).length > 1 ? 26 : 20) : 14;
  const inner = { ...ctx, width: ctx.width - markWidth - 2 };
  return (
    <View style={s.list}>
      {node.children.map((item, i) => (
        <View key={i} style={s.item}>
          <Text testID={CHROME} style={[s.body, { color: ctx.color, width: markWidth + 2 }]}>
            {typeof item.checked === 'boolean' ? (item.checked ? '☑' : '☐') : node.ordered ? `${start + i}.` : '•'}
          </Text>
          <View style={s.itemBody}>{flow(item.children as RootContent[], inner)}</View>
        </View>
      ))}
    </View>
  );
}

function block(n: RootContent, i: number, ctx: Ctx): ReactNode {
  const { t, color } = ctx;
  switch (n.type) {
    case 'paragraph':
      return (
        <Text key={i} style={[s.body, { color }]}>
          {phrasing(n.children, ctx)}
        </Text>
      );
    case 'heading':
      return (
        <Text key={i} style={[s.heading, n.depth <= 2 && s.headingBig, { color }]}>
          {phrasing(n.children, ctx)}
        </Text>
      );
    case 'list':
      return <ListBlock key={i} node={n} ctx={ctx} />;
    case 'code':
      return <CodeBlock key={i} node={n} ctx={ctx} />;
    case 'table':
      return <TableBlock key={i} node={n} ctx={ctx} />;
    case 'blockquote':
      return (
        <View key={i} style={[s.quote, { borderColor: t.line }]}>
          {flow(n.children, { ...ctx, color: t.dim, width: ctx.width - QUOTE_INDENT })}
        </View>
      );
    case 'thematicBreak':
      return <View key={i} style={[s.rule, { backgroundColor: t.line }]} />;
    case 'html':
      // Not drawn as HTML, and not hidden either: what the Agent wrote stays readable.
      return (
        <Text key={i} style={[s.body, { color: t.dim }]}>
          {n.value}
        </Text>
      );
    case 'footnoteDefinition':
      return (
        <View key={i} style={s.item}>
          <Text style={[s.body, { color: t.dim }]}>{`[${n.label ?? n.identifier}] `}</Text>
          <View style={s.itemBody}>{flow(n.children, { ...ctx, color: t.dim, width: ctx.width - LIST_INDENT })}</View>
        </View>
      );
    default:
      return null;
  }
}

function flow(nodes: RootContent[], ctx: Ctx): ReactNode[] {
  return nodes.map((n, i) => block(n, i, ctx));
}

function definitions(root: Parents, into = new Map<string, string>()) {
  for (const c of root.children) {
    if (c.type === 'definition') into.set(c.identifier, c.url);
    else if ('children' in c && c.type !== 'paragraph' && c.type !== 'heading' && c.type !== 'table') definitions(c, into);
  }
  return into;
}

const NO_DEFS = new Map<string, string>();
const hasDefinition = (root: Root) => root.children.some((c) => c.type === 'definition' || c.type === 'blockquote' || c.type === 'list');

export const Markdown = memo(function Markdown({
  text,
  t,
  color,
  onNote,
  fold = true,
}: {
  text: string;
  t: Theme;
  color: string;
  /** Says what a tap on a file link just did. */
  onNote?: (note: string) => void;
  /** Long code blocks start folded. */
  fold?: boolean;
}) {
  const { width } = useWindowDimensions();
  const root = parsed(text);
  const ctx: Ctx = { t, color, width: width - SIDE_PADDING, fold, onNote, defs: hasDefinition(root) ? definitions(root) : NO_DEFS };
  return <View style={s.stack}>{flow(root.children, ctx)}</View>;
});

const s = StyleSheet.create({
  stack: { gap: 8 },
  body: { fontSize: 15, lineHeight: 24 },
  // 600 draws as regular in Android's CJK font; 700 is the first weight it shows.
  strong: { fontWeight: '700' },
  em: { fontStyle: 'italic' },
  del: { textDecorationLine: 'line-through' },
  heading: { fontSize: 15, lineHeight: 24, fontWeight: '700', marginTop: 4 },
  headingBig: { fontSize: 17, lineHeight: 26 },
  // Weight resets: Android drops a bundled face asked to be bold, even when inherited.
  code: { fontFamily: mono, fontSize: 13, fontWeight: '400', fontStyle: 'normal' },
  list: { gap: 4 },
  item: { flexDirection: 'row' },
  itemBody: { flex: 1, gap: 6 },
  quote: { borderLeftWidth: 3, paddingLeft: QUOTE_INDENT - 3, gap: 6 },
  rule: { height: StyleSheet.hairlineWidth, marginVertical: 4 },
  codeCard: { borderRadius: 10, borderWidth: StyleSheet.hairlineWidth, overflow: 'hidden' },
  codeHead: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', gap: 12, paddingHorizontal: CODE_PAD, paddingTop: 7 },
  lang: { fontSize: 12, lineHeight: 16 },
  codeBody: { paddingHorizontal: CODE_PAD, paddingTop: 5, paddingBottom: 9 },
  pre: { fontFamily: mono, fontSize: 12, lineHeight: 18 },
  more: { borderTopWidth: StyleSheet.hairlineWidth, paddingVertical: 9, alignItems: 'center' },
  tableCard: { borderRadius: 10, borderWidth: StyleSheet.hairlineWidth, overflow: 'hidden' },
  row: { flexDirection: 'row' },
  cell: { fontSize: CELL_SIZE, lineHeight: 19, paddingHorizontal: 10, paddingVertical: 7 },
});
