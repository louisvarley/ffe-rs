//! The original's message strings (cache v30, `base:text/messages`): the
//! string arrays behind F1343's `0x8000..0xffff` codes. F1343 sends a code to
//! `DATA_007649[(code >> 9) & 0x3f]` with `code & 0x1ff` as the index; the
//! handlers listed here pass that index to F1347 with a plain pointer array
//! (`0x98be` is F319's `DATA_005299` entry 0xbe, "You do not have clearance").
//! Handler 18 (`0xa4xx`) is F319 again, so it shares handler 12's array.
//! The other handlers (F831's names, F613's cargo, F801, ...) generate their
//! text and are not here.
//!
//! The asset is a sequence of tables: the handler byte, a little-endian `u16`
//! count, then that many NUL-terminated strings. The strings keep the
//! original's bytes, including its formatting codes (`0x01` colour, `0xfe`
//! position, `0xff` variable expansion); [`plain`] removes them.

use std::collections::BTreeMap;

/// Most strings a table can index (`code & 0x1ff`).
pub const MAX_STRINGS: usize = 0x200;

/// Handler 18 is F319 again.
const ALIASES: [(u8, u8); 1] = [(18, 12)];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Messages<'a> {
    tables: BTreeMap<u8, Vec<&'a [u8]>>,
}

impl<'a> Messages<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        let mut tables = BTreeMap::new();
        let mut at = 0;
        while at < bytes.len() {
            let header = bytes
                .get(at..at + 3)
                .ok_or("message table header cut short")?;
            let handler = header[0];
            let count = usize::from(u16::from_le_bytes([header[1], header[2]]));
            if handler >= 0x40 || count == 0 || count > MAX_STRINGS {
                return Err(format!("bad message table {handler:#x} of {count}"));
            }
            at += 3;
            let mut strings = Vec::with_capacity(count);
            for _ in 0..count {
                let rest = bytes.get(at..).ok_or("message string cut short")?;
                let end = rest
                    .iter()
                    .position(|b| *b == 0)
                    .ok_or("unterminated message string")?;
                strings.push(&rest[..end]);
                at += end + 1;
            }
            if tables.insert(handler, strings).is_some() {
                return Err(format!("message table {handler:#x} repeated"));
            }
        }
        if tables.is_empty() {
            return Err("no message tables".to_owned());
        }
        Ok(Self { tables })
    }

    /// The raw bytes of message `code` (`0x8000..`), when its handler is one
    /// of the imported arrays and the index is in range.
    pub fn get(&self, code: u16) -> Option<&'a [u8]> {
        if code < 0x8000 {
            return None;
        }
        let mut handler = ((code >> 9) & 0x3f) as u8;
        if let Some((_, to)) = ALIASES.iter().find(|(from, _)| *from == handler) {
            handler = *to;
        }
        self.tables
            .get(&handler)?
            .get(usize::from(code & 0x1ff))
            .copied()
    }

    /// Message `code` as text with the formatting codes removed.
    pub fn text(&self, code: u16) -> Option<String> {
        self.get(code).map(plain)
    }

    /// How many strings each handler's table holds.
    pub fn counts(&self) -> Vec<(u8, usize)> {
        self.tables.iter().map(|(h, s)| (*h, s.len())).collect()
    }
}

/// Encode `tables` (`(handler, strings)`) in the asset's layout.
pub fn encode(tables: &[(u8, Vec<Vec<u8>>)]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for (handler, strings) in tables {
        if *handler >= 0x40 || strings.is_empty() || strings.len() > MAX_STRINGS {
            return Err(format!("bad message table {handler:#x}"));
        }
        out.push(*handler);
        out.extend_from_slice(&(strings.len() as u16).to_le_bytes());
        for string in strings {
            if string.contains(&0) {
                return Err("message string holds a NUL".to_owned());
            }
            out.extend_from_slice(string);
            out.push(0);
        }
    }
    Ok(out)
}

/// The visible text of an original string: `0x01 n` (colour) and `0xfe x y`
/// (position) are dropped, `0xff 0x30+v n` (a variable) becomes `…`, a
/// carriage return becomes a newline and other control bytes are dropped.
pub fn plain(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x01 => i += 2,
            0xfe => i += 3,
            0xfd => i += 2,
            0xff => {
                out.push('…');
                i += if bytes.get(i + 1) == Some(&b'0') {
                    3
                } else {
                    2
                };
            }
            b'\r' => {
                out.push('\n');
                i += 1;
            }
            b if b < 0x20 => i += 1,
            b => {
                out.push(char::from(b));
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_find_their_handler_and_index() {
        let bytes = encode(&[
            (
                12,
                vec![
                    b"first".to_vec(),
                    b"\x01\x02WARNING: \x01\x03Mis-Jump".to_vec(),
                ],
            ),
            (0, vec![b"zero".to_vec()]),
        ])
        .unwrap();
        let messages = Messages::new(&bytes).unwrap();
        // 0x9800 | 1: handler (0x98 >> 1) & 0x3f = 12, index 1.
        assert_eq!(
            messages.get(0x9801),
            Some(&b"\x01\x02WARNING: \x01\x03Mis-Jump"[..])
        );
        assert_eq!(messages.text(0x9801).as_deref(), Some("WARNING: Mis-Jump"));
        // Handler 18 (0xa4xx) is F319's array again.
        assert_eq!(messages.text(0xa400).as_deref(), Some("first"));
        assert_eq!(messages.text(0x8000).as_deref(), Some("zero"));
        assert_eq!(messages.get(0x9802), None);
        assert_eq!(messages.get(0x7fff), None);
        assert_eq!(messages.counts(), vec![(0, 1), (12, 2)]);
    }

    #[test]
    fn malformed_assets_are_refused() {
        assert!(Messages::new(&[]).is_err());
        assert!(Messages::new(&[12, 1, 0, b'a']).is_err());
        assert!(Messages::new(&[0x40, 1, 0, 0]).is_err());
        assert!(Messages::new(&[1, 1, 0, 0, 1, 1, 0, 0]).is_err());
        assert!(encode(&[(1, vec![b"a\0b".to_vec()])]).is_err());
    }
}
