// Close codes the machine or the relay ends this phone's socket with, one per case, and
// what each means here. The same numbers live in `crates/mooring-remote/src/close.rs`;
// `vectors/v1.json` lists them and `test/close.test.ts` checks the two agree.

export const Close = {
  /** The session ended on its own. */
  NORMAL: 1000,
  /** The machine left the relay. */
  MACHINE_GONE: 1012,
  /** This phone fell too far behind and the relay cut it. */
  TOO_SLOW: 1013,
  /** This phone sent something the protocol does not allow. */
  BAD_MESSAGE: 4400,
  /** Unknown device, wrong key, or unpaired. */
  UNPAIRED: 4401,
  /** The machine has a new code; this phone's token is retired. */
  UNKNOWN_MACHINE: 4404,
  /** The computer clicked "disconnect" on this phone; it is still paired. */
  KICKED: 4410,
  /** This phone's role changed; connecting again gives it the new one. */
  ROLE_CHANGED: 4411,
} as const;

/** What to do after a close: how the machine shows, and when to try again. */
export type Outcome = {
  status: 'offline' | 'refused';
  /** `never`: only a new pairing helps; `now`: at once; `later`: the usual backoff. */
  retry: 'never' | 'now' | 'later';
  /** For the person holding the phone; the code stays in it for support. */
  text: string;
};

/** A relay or machine that names an outdated relay in its close reason. */
const OLD_RELAY = /太旧|too old|upgrade the relay|升级中继/i;

/**
 * `code` is absent when the socket never closed with one (a timeout, a bad handshake).
 * `reason` is the close frame's text: a relay too old for this machine says so there,
 * which beats the code's usual meaning.
 */
export function closeOutcome(code?: number, reason?: string): Outcome {
  if (reason && OLD_RELAY.test(reason)) {
    return { status: 'offline', retry: 'later', text: '中继版本太旧：请升级中继，或者换一个新的中继' };
  }
  switch (code) {
    case Close.UNPAIRED:
      return { status: 'refused', retry: 'never', text: '这台手机没有配对，或者已经被取消；重新扫码配对' };
    case Close.UNKNOWN_MACHINE:
      return { status: 'refused', retry: 'never', text: '电脑换了配对码，需要重新配对' };
    case Close.KICKED:
      return { status: 'offline', retry: 'later', text: '电脑那边断开了这台手机，5 分钟后才能再连' };
    case Close.ROLE_CHANGED:
      return { status: 'offline', retry: 'now', text: '这台手机的权限改了，正在按新权限重连' };
    case Close.MACHINE_GONE:
      return { status: 'offline', retry: 'later', text: '电脑上的 Mooring 不在线' };
    case Close.TOO_SLOW:
      return { status: 'offline', retry: 'later', text: '网络太慢，中继断开了连接' };
    case undefined:
      return { status: 'offline', retry: 'later', text: '连不上：电脑或中继不在线，或者网络不通' };
    default:
      return { status: 'offline', retry: 'later', text: `连不上：电脑或中继不在线，或者网络不通（${code}）` };
  }
}
