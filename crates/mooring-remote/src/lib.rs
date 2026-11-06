//! Mooring's remote protocol, shared by everything that speaks it: the machine
//! (`mooring remote`), the relay (`mooring-relay`), the desktop workbench, and as a
//! reference for phone apps in other languages.
//!
//! - [`protocol`]: the JSON lines a client and a machine exchange.
//! - [`tools`]: the replies to `toolcalls` and `toolcall`.
//! - [`client`]: a client of one machine over a pipe (`mooring remote --stdio`, often via ssh).
//! - [`close`]: the close codes a phone's socket ends with, one per case.
//! - [`role`]: what a paired device may do.
//! - [`link`]: the pairing link a QR code carries.
//! - [`secure`]: the Noise channel a phone and a machine run, so a relay sees ciphertext.
//! - [`wire`]: the frames between a machine and a relay (the alma-relay protocol).
//!
//! `tests/vectors.rs` checks `vectors/v1.json`: fixed inputs and the exact bytes they
//! must produce, for an implementation in another language to check itself against.
//! Shapes and rules are in the Keepline repo's `specs/extensions/remote-protocol.md` and
//! `relay.md` (not moved here).
pub mod asks;
pub mod cards;
pub mod chat;
pub mod client;
pub mod close;
pub mod link;
pub mod protocol;
pub mod role;
pub mod secure;
pub mod tools;
pub mod usage;
pub mod wire;
