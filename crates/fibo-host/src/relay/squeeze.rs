//! The native protocol through the relay, compressed as `ssh -o Compression=yes` compresses
//! it: a workspace is JSON and goes whole on each change (2 MB is usual), and a relay's
//! bandwidth is what a cloud server gives. One zstd stream each way for the session's
//! life, flushed after every piece so the other side can act on it at once; its window
//! holds the last workspace, so the next one mostly refers back to it.
use std::io;
use zstd::stream::raw::{CParameter, Decoder, Encoder, InBuffer, Operation, OutBuffer};

/// Large enough for one workspace and the next.
const WINDOW_LOG: u32 = 23;

pub(super) struct Squeeze(Encoder<'static>);

impl Squeeze {
    pub(super) fn new() -> io::Result<Self> {
        let mut e = Encoder::new(3)?;
        e.set_parameter(CParameter::WindowLog(WINDOW_LOG))?;
        Ok(Self(e))
    }
    /// `bytes` compressed and flushed: everything the other side needs to decode them.
    pub(super) fn pack(&mut self, bytes: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(bytes.len() / 2 + 64);
        let mut chunk = vec![0; zstd::zstd_safe::CCtx::out_size().max(bytes.len() + 64)];
        let mut input = InBuffer::around(bytes);
        while input.pos() < bytes.len() {
            let mut o = OutBuffer::around(&mut chunk[..]);
            self.0.run(&mut input, &mut o)?;
            let n = o.pos();
            out.extend_from_slice(&chunk[..n]);
        }
        loop {
            let mut o = OutBuffer::around(&mut chunk[..]);
            let left = self.0.flush(&mut o)?;
            let n = o.pos();
            out.extend_from_slice(&chunk[..n]);
            if left == 0 {
                return Ok(out);
            }
        }
    }
}

pub(super) struct Unsqueeze(Decoder<'static>);

impl Unsqueeze {
    pub(super) fn new() -> io::Result<Self> {
        let mut d = Decoder::new()?;
        d.set_parameter(zstd::stream::raw::DParameter::WindowLogMax(WINDOW_LOG))?;
        Ok(Self(d))
    }
    /// What `packed` decodes to; a piece the other side flushed decodes whole.
    pub(super) fn unpack(&mut self, packed: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(packed.len() * 4);
        let mut chunk = vec![0; zstd::zstd_safe::DCtx::out_size()];
        let mut input = InBuffer::around(packed);
        loop {
            let mut o = OutBuffer::around(&mut chunk[..]);
            self.0.run(&mut input, &mut o)?;
            let n = o.pos();
            out.extend_from_slice(&chunk[..n]);
            // Done once every byte went in and the last pass did not fill its buffer.
            if input.pos() == packed.len() && n < chunk.len() {
                return Ok(out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_piece_decodes_as_it_arrives_and_a_repeat_costs_little() {
        let (mut s, mut u) = (Squeeze::new().unwrap(), Unsqueeze::new().unwrap());
        let workspace: Vec<u8> = (0..300_000)
            .flat_map(|i: u32| format!("{{\"id\":{i},\"t\":\"x{}\"}}", i % 97).into_bytes())
            .collect();
        let first = s.pack(&workspace).unwrap();
        assert_eq!(u.unpack(&first).unwrap(), workspace);
        // The next workspace, one field changed: within the window it refers back.
        let mut next = workspace.clone();
        next[100] = b'9';
        let second = s.pack(&next).unwrap();
        assert!(
            second.len() * 20 < first.len(),
            "{} vs {}",
            second.len(),
            first.len()
        );
        assert_eq!(u.unpack(&second).unwrap(), next);
        // Small pieces, one at a time, as a terminal sends them.
        for piece in [&b"a"[..], b"", b"hello"] {
            assert_eq!(u.unpack(&s.pack(piece).unwrap()).unwrap(), piece);
        }
    }
}
