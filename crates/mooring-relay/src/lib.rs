//! The relay: a machine dials out and registers, a phone opens `/c/<token>/…`, and each
//! phone socket is bridged over its machine's socket. It speaks the WebSocket part of the
//! alma-relay protocol ([`mooring_remote::wire`]) and never reads a message's contents:
//! phone and machine encrypt end to end ([`mooring_remote::secure`]).
//! Per-address limits are in [`limit`]; running one is in `docs/relay/self-host.md`.
pub mod limit;
pub mod server;
