import { afterEach, expect, jest, test } from 'bun:test';
import { Remote, REQUEST_TIMEOUT_MS } from '../src/remote/client';
import { Handshake, respond, type Channel } from '../src/remote/noise';
import { utf8 } from '../src/remote/bytes';

afterEach(() => { jest.useRealTimers(); });
function connected() {
  jest.useFakeTimers();
  jest.setSystemTime(1_000_000);
  const key = new Uint8Array(32);
  const hs = new Handshake(key);
  const host = respond(key, hs.first);
  const ws = { send: (_b: Uint8Array) => {}, close: () => {}, onmessage: (_e: { data: Uint8Array }) => {} };
  // Exercise the established connection with real Noise frames; the vectors cover handshake bytes.
  const r = new (Remote as unknown as new (ws: unknown, channel: Channel) => Remote)(ws, hs.finish(host.answer));
  return { r, send: (b: Uint8Array) => ws.onmessage({ data: b }), host: host.channel };
}

test('a slow upload continues across the original deadline while authenticated ACKs arrive', async () => {
  const { r, send, host } = connected();
  const pending = r.request('upload', { data: 'x'.repeat(8192) });
  for (let n = 0; n < 4; n++) {
    jest.advanceTimersByTime(8_000);
    send(host.ack(1024));
    expect(r.ended).toBeNull();
  }
  send(host.data(utf8('{"id":1,"ok":{"saved":true}}\n')));
  expect(await pending).toEqual({ saved: true });
});

test('a stalled upload ends only after a full interval without progress', async () => {
  const { r, send, host } = connected();
  const pending = r.request('upload', { data: 'x'.repeat(8192) }).catch(e => e);
  jest.advanceTimersByTime(8_000);
  send(host.ack(1024));
  jest.advanceTimersByTime(REQUEST_TIMEOUT_MS - 1);
  expect(r.ended).toBeNull();
  jest.advanceTimersByTime(1);
  expect(r.ended?.code).toBe(4408);
  expect((await pending).message).toContain('传输进展');
});

test('partial reply DATA keeps the request alive until its JSON line finishes', async () => {
  const { r, send, host } = connected();
  const pending = r.request('chat');
  jest.advanceTimersByTime(8_000);
  send(host.data(utf8('{"id":1,"ok":')));
  jest.advanceTimersByTime(8_000);
  expect(r.ended).toBeNull();
  send(host.data(utf8('42}\n')));
  expect(await pending).toBe(42);
});

test('events from another terminal cannot keep an unanswered request alive', async () => {
  const { r, send, host } = connected();
  const pending = r.request('image').catch(e => e);
  jest.advanceTimersByTime(8_000);
  send(host.data(utf8('{"stream":99,"type":"event","event":{}}\n')));
  jest.advanceTimersByTime(2_000);
  expect(r.ended?.code).toBe(4408);
  expect(await pending).toBeInstanceOf(Error);
});

test('ACKs for later requests cannot renew a fully uploaded request', async () => {
  const { r, send, host } = connected();
  const pending = r.request('image').catch(e => e);
  send(host.ack(utf8('{"id":1,"op":"image"}\n').length));
  jest.advanceTimersByTime(8_000);
  const ping = r.request('ping');
  send(host.ack(utf8('{"id":2,"op":"ping"}\n').length));
  send(host.data(utf8('{"id":2,"ok":{}}\n')));
  await ping;
  jest.advanceTimersByTime(2_000);
  expect(r.ended?.code).toBe(4408);
  expect(await pending).toBeInstanceOf(Error);
});

test('a ping queued behind a long reply does not disconnect that progressing download', async () => {
  const { r, send, host } = connected();
  const chat = r.request('chat');
  const ping = r.request('ping');
  send(host.ack(utf8('{"id":1,"op":"chat"}\n{"id":2,"op":"ping"}\n').length));
  jest.advanceTimersByTime(8_000);
  send(host.data(utf8('{"id":1,"ok":{"text":"')));
  jest.advanceTimersByTime(8_000);
  expect(r.ended).toBeNull();
  send(host.data(utf8('done"}}\n{"id":2,"ok":{}}\n')));
  expect(await chat).toEqual({ text: 'done' });
  expect(await ping).toEqual({});
});

test('a request blocked behind a reply still expires when that reply stops arriving', async () => {
  const { r, send, host } = connected();
  const chat = r.request('chat').catch(e => e);
  const ping = r.request('ping').catch(e => e);
  jest.advanceTimersByTime(8_000);
  send(host.data(utf8('{"id":1,"ok":')));
  jest.advanceTimersByTime(10_000);
  expect(r.ended?.code).toBe(4408);
  expect(await chat).toBeInstanceOf(Error);
  expect(await ping).toBeInstanceOf(Error);
});

test('a long reply final frame gives the queued ping time to arrive in a separate frame', async () => {
  const { r, send, host } = connected();
  const chat = r.request('chat');
  const ping = r.request('ping').catch(e => e);
  jest.advanceTimersByTime(8_000);
  send(host.data(utf8('{"id":1,"ok":{"text":"')));
  jest.advanceTimersByTime(8_000);
  send(host.data(utf8('done"}}\n')));
  expect(await chat).toEqual({ text: 'done' });
  jest.advanceTimersByTime(8_000);
  expect(r.ended).toBeNull();
  send(host.data(utf8('{"id":2,"ok":{}}\n')));
  expect(await ping).toEqual({});
});
