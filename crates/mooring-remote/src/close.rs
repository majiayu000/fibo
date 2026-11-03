//! Close codes a phone's WebSocket ends with, set by the machine or by the relay. Each
//! says what the phone should do next, so each case has its own code; the phone app
//! mirrors them in `mobile/src/remote/close.ts`, and `vectors/v1.json` lists them so a
//! test on each side catches a mismatch.
//!
//! Codes between the machine and the relay (4000 replaced, 4002 bad hash, 4003 bad key)
//! are another socket and not listed here.

/// The session ended on its own.
pub const NORMAL: u16 = 1000;
/// The machine left the relay: offline, redial later.
pub const MACHINE_GONE: u16 = 1012;
/// The phone fell too far behind and the relay cut it: redial.
pub const TOO_SLOW: u16 = 1013;
/// The phone sent something the protocol does not allow: redial.
pub const BAD_MESSAGE: u16 = 4400;
/// Unknown device, wrong key, or unpaired: refused; only a new pairing helps.
pub const UNPAIRED: u16 = 4401;
/// The machine has a new code (`mooring remote --new-code`) and this token is retired:
/// refused; only a new pairing helps.
pub const UNKNOWN_MACHINE: u16 = 4404;
/// The computer clicked "disconnect" on this phone; it stays paired and may come back.
pub const KICKED: u16 = 4410;
/// The device's role changed; connecting again gives it the new one.
pub const ROLE_CHANGED: u16 = 4411;

/// Every code by name, for the vectors file.
pub const ALL: [(&str, u16); 8] = [
    ("normal", NORMAL),
    ("machine_gone", MACHINE_GONE),
    ("too_slow", TOO_SLOW),
    ("bad_message", BAD_MESSAGE),
    ("unpaired", UNPAIRED),
    ("unknown_machine", UNKNOWN_MACHINE),
    ("kicked", KICKED),
    ("role_changed", ROLE_CHANGED),
];
