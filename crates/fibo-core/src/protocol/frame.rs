//! Length-prefixed native frames and their read/write operations.
use super::{Reply, Request};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const MAJOR: u32 = 3;
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
pub const CONTROL: u8 = 1;
pub const SNAPSHOT: u8 = 2;
pub const READY: u8 = 3;
pub const OUTPUT: u8 = 4;
pub const RESIZED: u8 = 5;
pub const STATUS: u8 = 6;
pub const ACK: u8 = 7;
pub const WRITABLE: u8 = 8;

pub fn write_frame(w: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    if payload.len() >= MAX_FRAME {
        return Err(io::Error::other("frame too large"));
    }
    w.write_all(&((payload.len() + 1) as u32).to_be_bytes())?;
    w.write_all(&[kind])?;
    w.write_all(payload)
}
pub fn read_frame(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut size = [0; 4];
    r.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err(io::Error::other("invalid frame size"));
    }
    let mut bytes = vec![0; size];
    r.read_exact(&mut bytes)?;
    Ok((bytes[0], bytes[1..].to_vec()))
}
/// Serialize one queued user input as ordered bounded packets. UTF-8 and bracketed
/// paste delimiters may span packets: the PTY remains a byte stream. Never retry.
pub fn send_input(w: &mut impl Write, bytes: &[u8], lossy: bool) -> io::Result<()> {
    for chunk in bytes.chunks(16 * 1024) {
        send(
            w,
            CONTROL,
            &Request::Input {
                bytes: chunk.to_vec(),
                lossy,
            },
        )?;
    }
    Ok(())
}

pub fn send<T: Serialize>(w: &mut impl Write, kind: u8, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    write_frame(w, kind, &bytes)
}
pub fn receive<T: for<'de> Deserialize<'de>>(r: &mut impl Read) -> io::Result<T> {
    let (kind, bytes) = read_frame(r)?;
    if kind != CONTROL {
        return Err(io::Error::other("expected control frame"));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

/// One try, without resending: `delivery::deliver` waits out a dropped link.
pub fn rpc(socket: &std::path::Path, request: &Request) -> io::Result<Reply> {
    crate::delivery::attempt(socket, request).into_result()
}
