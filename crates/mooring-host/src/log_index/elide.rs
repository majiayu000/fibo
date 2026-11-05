//! Bounds each long JSON string of a buffered line.
/// The longest stretch of one JSON string a buffered line keeps.
pub(super) const STRING_KEEP: usize = 32 * 1024;
/// Shortens each long JSON string of a line as it streams in, keeping the line valid
/// JSON: a pasted image's base64 or a huge tool output would otherwise push the whole
/// line past the 256 KiB buffer and the ask on it would be lost. A string is cut only
/// between whole characters, never inside an escape or a surrogate pair.
#[derive(Default, Clone, Copy)]
pub(super) struct Elide {
    in_string: bool,
    escaped: bool,
    hex_left: u8,
    hex: u16,
    /// A `\uD8xx` just ended; its low half must follow before a cut.
    high: bool,
    run: usize,
    skipping: bool,
}
impl Elide {
    pub(super) fn over(bytes: &[u8]) -> Self {
        let mut e = Self::default();
        for &b in bytes {
            e.keep(b);
        }
        e
    }
    /// Whether `b` stays in the buffered line.
    pub(super) fn keep(&mut self, b: u8) -> bool {
        if !self.in_string {
            if b == b'"' {
                *self = Self {
                    in_string: true,
                    ..Self::default()
                };
            }
            return true;
        }
        let whole = !self.escaped && self.hex_left == 0 && !self.high && b & 0xC0 != 0x80;
        if !self.skipping && self.run >= STRING_KEEP && whole && b != b'"' {
            self.skipping = true;
        }
        if self.escaped {
            self.escaped = false;
            if b == b'u' {
                self.hex_left = 4;
                self.hex = 0;
            } else {
                self.high = false;
            }
        } else if self.hex_left > 0 {
            self.hex_left -= 1;
            self.hex = self.hex << 4 | (b as char).to_digit(16).unwrap_or(0) as u16;
            if self.hex_left == 0 {
                self.high = (0xD800..0xDC00).contains(&self.hex);
            }
        } else if b == b'\\' {
            self.escaped = true;
        } else if b == b'"' {
            self.in_string = false;
            self.skipping = false;
            return true;
        } else {
            self.high = false;
        }
        if self.skipping {
            return false;
        }
        self.run += 1;
        true
    }
}
