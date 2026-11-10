// Every Chinese literal the phone shows goes through tr() (or tk() / notUi()), and every key has
// English. Walks the TypeScript syntax tree, so comments and identifiers never count.
import { expect, test } from 'bun:test';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import ts from 'typescript';
import { EN } from '../src/locales/en';

const ROOT = join(import.meta.dir, '../src');
const CJK = /[㐀-鿿　-〿＀-￯]/;
// Screenshot data only, dropped from a normal build (EXPO_PUBLIC_FIXTURE).
const SKIP = new Set(['fixture.ts']);
const MARKERS = new Set(['tr', 'tk', 'notUi']);

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) return n === 'locales' ? [] : files(p);
    return /\.tsx?$/.test(n) && !SKIP.has(relative(ROOT, p)) ? [p] : [];
  });
}

type Found = { file: string; line: number; text: string };
const bare: Found[] = [];
const keys: Found[] = [];
const dynamic: Found[] = [];

for (const file of files(ROOT)) {
  const src = ts.createSourceFile(file, readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true, file.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const at = (n: ts.Node, text: string): Found => ({ file: relative(ROOT, file), line: src.getLineAndCharacterOfPosition(n.getStart()).line + 1, text });
  const marker = (n: ts.Node) => {
    const p = n.parent;
    return ts.isCallExpression(p) && p.arguments[0] === n && ts.isIdentifier(p.expression) && MARKERS.has(p.expression.text) ? p.expression.text : null;
  };
  const visit = (n: ts.Node) => {
    if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) {
      if (CJK.test(n.text)) {
        const m = marker(n);
        if (m === 'tr' || m === 'tk') keys.push(at(n, n.text));
        else if (!m && !ts.isImportDeclaration(n.parent) && !ts.isLiteralTypeNode(n.parent)) bare.push(at(n, n.text));
      }
    } else if (ts.isTemplateExpression(n)) {
      const text = n.head.text + n.templateSpans.map((s) => '${…}' + s.literal.text).join('');
      if (CJK.test(text)) (marker(n) ? dynamic : bare).push(at(n, text));
    } else if (ts.isJsxText(n)) {
      if (CJK.test(n.text)) bare.push(at(n, n.text.trim()));
    }
    ts.forEachChild(n, visit);
  };
  visit(src);
}

const show = (xs: Found[]) => xs.map((x) => `${x.file}:${x.line}  ${x.text}`).join('\n');

test('Chinese the phone shows goes through tr()', () => {
  expect(show(bare)).toBe('');
});
test('tr() keys are plain literals, with {name} for what varies', () => {
  expect(show(dynamic)).toBe('');
});
test('every key has English', () => {
  expect(show(keys.filter((k) => !(k.text in EN)))).toBe('');
});

test('the setting switches the words, and a missing key falls back to the Chinese', async () => {
  const { tr: t, setLanguage } = await import('../src/i18n');
  expect(t('在跑')).toBe('在跑');
  setLanguage('en');
  expect(t('在跑')).toBe('Running');
  expect(t('没有这条词')).toBe('没有这条词');
  expect(t('{n} 个', { n: 3 })).toBe('3 个');
  expect(t('电脑#list')).toBe('Computers');
  setLanguage('auto');
  expect(t('在跑')).toBe('在跑');
  expect(t('电脑#list')).toBe('电脑');
});
