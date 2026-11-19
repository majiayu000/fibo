//! Where typed bytes are cut into writes. The program can read between two writes, and one
//! that then finds a lone ESC at the end of what it read takes it as the Esc key and the rest
//! of the sequence as typed text: `[<35;9;21M` in Codex's input box (11-19), where Esc also
//! interrupts a running turn. So a cut never leaves an escape sequence unfinished when it can
//! help it.

/// The most one write carries.
pub const SLICE: usize = 256;

const ESC: u8 = 0x1b;

/// Where the write of `bytes[start..]` ends: at most `max` bytes on, and before an escape
/// sequence the cut would leave unfinished. A sequence that starts the slice and is longer
/// than `max` cannot be kept whole and is cut at `max`.
pub fn slice_end(bytes: &[u8], start: usize, max: usize) -> usize {
    let end = bytes.len().min(start + max);
    if end == bytes.len() {
        return end;
    }
    match bytes[start..end].iter().rposition(|&b| b == ESC) {
        Some(at) if at > 0 && !finished(&bytes[start + at..end]) => start + at,
        _ => end,
    }
}

/// Whether `seq`, which starts with ESC, is a whole sequence.
fn finished(seq: &[u8]) -> bool {
    match seq.get(1) {
        None => false,
        // X10 mouse: `ESC [ M` and three raw bytes.
        Some(b'[') if seq.get(2) == Some(&b'M') => seq.len() >= 6,
        Some(b'[') => seq[2..].iter().any(|b| (0x40..=0x7e).contains(b)),
        Some(b'O') => seq.len() > 2,
        // String sequences: one ended by `ESC \` has its last ESC there, not here.
        Some(b']' | b'P' | b'_' | b'^' | b'X') => seq[2..].contains(&0x07),
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(i: usize) -> Vec<u8> {
        format!("\x1b[<35;{};{}M", 5 + i % 60, 10 + i % 7).into_bytes()
    }

    fn slices(bytes: &[u8], max: usize) -> Vec<&[u8]> {
        let (mut out, mut at) = (vec![], 0);
        while at < bytes.len() {
            let end = slice_end(bytes, at, max);
            out.push(&bytes[at..end]);
            at = end;
        }
        out
    }

    #[test]
    fn a_burst_of_mouse_reports_is_written_in_whole_reports() {
        // Pad so a report starts at byte 255: a plain 256-byte cut leaves its ESC alone.
        let mut bytes = vec![b'a'; 255];
        for i in 0..40 {
            bytes.extend(report(i));
        }
        assert_eq!(slice_end(&bytes, 0, SLICE), 255);
        for offset in 0..20 {
            let mut bytes = vec![b'a'; offset];
            for i in 0..200 {
                bytes.extend(report(i));
            }
            let pieces = slices(&bytes, SLICE);
            assert!(pieces.iter().all(|p| p.len() <= SLICE));
            for piece in &pieces[1..] {
                assert_eq!(piece[0], ESC, "offset {offset}: a piece starts mid-report");
            }
            assert_eq!(pieces.concat(), bytes);
        }
    }

    #[test]
    fn unfinished_sequences_wait_for_the_next_write() {
        let cut = |head: &[u8], tail: &[u8]| {
            let bytes = [head, tail].concat();
            slice_end(&bytes, 0, head.len())
        };
        assert_eq!(cut(b"abc\x1b", b"[201~"), 3);
        assert_eq!(cut(b"abc\x1b[20", b"1~"), 3);
        assert_eq!(cut(b"abc\x1bO", b"A"), 3);
        assert_eq!(cut(b"abc\x1b[M !", b"!"), 3);
        assert_eq!(cut(b"abc\x1b]52;c;aGk=", b"\x07"), 3);
        // Whole sequences and plain text are cut where the slice ends.
        assert_eq!(cut(b"abc\x1b[201~", b"xyz"), 9);
        assert_eq!(cut(b"abc\x1bOA", b"xyz"), 6);
        assert_eq!(cut(b"abc\x1b[M !!", b"xyz"), 9);
        assert_eq!(cut(b"abc\x1bx", b"yz"), 5);
        assert_eq!(cut(b"abcdef", b"ghi"), 6);
        // The last piece needs no cut.
        assert_eq!(slice_end(b"ab\x1b", 0, 3), 3);
    }

    #[test]
    fn a_sequence_longer_than_a_slice_is_cut_at_the_slice() {
        let mut bytes = b"\x1b]52;c;".to_vec();
        bytes.extend(vec![b'A'; 600]);
        bytes.push(0x07);
        assert_eq!(slice_end(&bytes, 0, SLICE), SLICE);
    }
}
