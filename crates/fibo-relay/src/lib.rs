//! The relay: a machine dials out and registers, a phone opens `/c/<token>/…`, and each
//! phone socket is bridged over its machine's socket. It speaks the WebSocket part of the
//! relay tunnel protocol ([`fibo_remote::wire`]) and never reads a message's contents:
//! phone and machine encrypt end to end ([`fibo_remote::secure`]).
//! Per-address limits are in [`limit`], bytes per machine in [`usage`]; running one is in
//! `docs/relay/self-host.md`.
pub mod limit;
pub mod server;
pub mod usage;
