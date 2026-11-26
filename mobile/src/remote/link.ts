// The pairing link the machine shows as a QR code, as `fibo-remote/src/link.rs` reads
// and writes it, and the relay URL it leads to.
import { sha256 } from '@noble/hashes/sha2.js';
import { tr } from '../i18n';
import { b64url, hex, unb64url, utf8 } from './bytes';
import { derive } from './noise';

export const LINK_VERSION = 2;

export type Link = {
  relay: string;
  /** Other relays the same machine is on (`alt=`, repeatable): every one is dialed, the first through wins. */
  alts?: string[];
  /** The machine's code, shared by every phone it paired: gives the route token. */
  code: Uint8Array;
  device: string;
  /** This device's own key: gives the Noise PSK. Keep it in secure storage. */
  key: Uint8Array;
  machine: string;
  name: string;
};

function unescape(s: string): string {
  const out: number[] = [];
  for (let i = 0; i < s.length; ) {
    const h = s[i] === '%' ? parseInt(s.slice(i + 1, i + 3), 16) : NaN;
    if (s[i] === '%' && /^[0-9a-fA-F]{2}$/.test(s.slice(i + 1, i + 3))) {
      out.push(h);
      i += 3;
    } else {
      out.push(...utf8(s[i]));
      i += 1;
    }
  }
  return new TextDecoder().decode(new Uint8Array(out));
}

export function escape(s: string): string {
  return Array.from(utf8(s), (b) => {
    const c = String.fromCharCode(b);
    return /[A-Za-z0-9\-_.~]/.test(c) ? c : '%' + b.toString(16).toUpperCase().padStart(2, '0');
  }).join('');
}

/**
 * A pairing link this phone cannot use. `message` is for the person, with what to do next;
 * `detail` is what exactly was wrong, for support, shown only under 「详情」.
 */
export class LinkError extends Error {
  constructor(
    readonly kind: 'foreign' | 'version' | 'broken',
    readonly detail: string,
  ) {
    super(
      kind === 'foreign'
        ? tr('这不是 Fibo 的配对链接。请在电脑的 Fibo 里点标题栏的手机图标，选「配对新手机」，扫屏幕上的二维码')
        : kind === 'version'
          ? tr('这个配对链接来自另一个版本的 Fibo，手机认不出。请把电脑和手机上的 Fibo 都更新到最新，再重新配对')
          : tr('配对链接不完整，可能没复制全。请重新扫码，或者重新复制整段链接'),
    );
  }
}

function code(field: string | undefined, name: string): Uint8Array {
  const b = field === undefined ? null : unb64url(field);
  if (!b || b.length !== 32) throw new LinkError('broken', `bad ${name}`);
  return b;
}

export function parseLink(text: string): Link {
  text = text.trim();
  const prefix = 'mooring://pair?';
  if (!text.startsWith(prefix)) throw new LinkError('foreign', 'not a mooring://pair link');
  const fields: Record<string, string> = {};
  const alts: string[] = [];
  for (const kv of text.slice(prefix.length).split('&')) {
    const at = kv.indexOf('=');
    if (at > 0 && kv.slice(0, at) === 'alt') alts.push(unescape(kv.slice(at + 1)));
    else if (at > 0 && !(kv.slice(0, at) in fields)) fields[kv.slice(0, at)] = unescape(kv.slice(at + 1));
  }
  if (Number(fields.v) !== LINK_VERSION) throw new LinkError('version', `unsupported link version ${fields.v}`);
  if (!fields.relay) throw new LinkError('broken', 'no relay');
  if (!fields.device) throw new LinkError('broken', 'no device');
  const others = [...new Set(alts)].filter((a) => a && a !== fields.relay);
  return {
    relay: fields.relay,
    ...(others.length && { alts: others }),
    code: code(fields.code, 'code'),
    device: fields.device,
    key: code(fields.key, 'key'),
    machine: fields.machine ?? '',
    name: fields.name ?? '',
  };
}

export function linkText(l: Link): string {
  const alts = (l.alts ?? []).map((a) => `&alt=${escape(a)}`).join('');
  return `mooring://pair?v=${LINK_VERSION}&relay=${escape(l.relay)}${alts}&code=${b64url(l.code)}&device=${escape(l.device)}&key=${b64url(l.key)}&machine=${escape(l.machine)}&name=${escape(l.name)}`;
}

/** The route token the relay knows this machine by. */
export const token = (code: Uint8Array): string => b64url(derive('keepline route 1', code));

/** What the relay stores instead of the token. */
export const tokenHash = (token: string): string => hex(sha256(utf8(token)));

export function websocketBase(relay: string): string {
  relay = relay.replace(/\/+$/, '');
  if (relay.startsWith('https://')) return 'wss://' + relay.slice(8);
  if (relay.startsWith('http://')) return 'ws://' + relay.slice(7);
  if (relay.startsWith('wss://') || relay.startsWith('ws://')) return relay;
  if (relay && !relay.includes('://')) return 'wss://' + relay;
  throw new Error(tr('看不懂的中继地址 {relay}', { relay }));
}

/** The relay's own web address, where it answers `/healthz`. */
export const relayHttpBase = (relay: string): string => websocketBase(relay).replace(/^ws(s?):\/\//, 'http$1://');

/** Every relay to dial the machine through, the link's own first. */
export const relaysOf = (l: Link): string[] => [...new Set([l.relay, ...(l.alts ?? [])])];

/** Where this phone opens its WebSocket. */
export const linkUrl = (l: Link): string =>
  `${websocketBase(l.relay)}/c/${token(l.code)}/keepline?d=${escape(l.device)}`;

/** A relay-side binary frame `[kind][id length][id][payload]`; phones never send one, the vectors check it. */
export function frame(kind: number, id: string, payload: Uint8Array): Uint8Array {
  const idb = utf8(id);
  const out = new Uint8Array(2 + idb.length + payload.length);
  out[0] = kind;
  out[1] = idb.length;
  out.set(idb, 2);
  out.set(payload, 2 + idb.length);
  return out;
}
