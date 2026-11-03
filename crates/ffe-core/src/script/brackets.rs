//! F77/F76: the `[` macros F1343 expands in every message string after its
//! FF codes (`docs/notes/journal.md` lists them). A macro is `[`, two
//! letters, then its arguments; F75 reads a seed (`G n` a global random word
//! F106, `L n` the article's slot word F95, `R` a fresh draw from the shared
//! pair), F74 a signed number.
//!
//! Ported: `Gr` (a `0x02 n` colour change), `Mi` (F64's query to the
//! scripts), `Ca` (the player's cash), `Fm`/`Ff`/`Cr`/`Co` (F791 names of
//! kinds 1, 2 and 12), `Dr` (a date between two day offsets), `Nu` (a
//! number between two bounds) and `Pn` (the commander's name). The other
//! macros (the journals' systems, ranks and alternatives) are kept as they
//! are until they are ported; an unknown code drops its two letters and
//! keeps the `[`, as F76 does.

/// What the macros read from the game and the scripts.
pub trait Macros {
    /// F75: seed `kind` (`b'G'`, `b'L'`, `b'R'`) with number `n`.
    fn seed(&mut self, kind: u8, n: i32) -> i32;
    /// F64: the scripts' text for query `name` with `word`.
    fn query(&mut self, word: i32, name: &str) -> Vec<u8>;
    /// `DATA_008919`.
    fn commander(&mut self) -> Vec<u8>;
    /// `DATA_008888`, tenths of a credit.
    fn cash(&mut self) -> i64;
    /// `DATA_008807`.
    fn today(&mut self) -> u32;
    /// F791: a generated name of `kind` from `seed`.
    fn name(&mut self, kind: u32, seed: i32) -> Vec<u8>;
    /// F1358's date text for absolute day `day`.
    fn date(&mut self, day: u32) -> Vec<u8>;
}

/// F921 on two seed words: a number below `range` (the words advance).
fn f921(range: u32, a: &mut u32, b: &mut u32) -> u32 {
    *a = a.wrapping_add(*b);
    *b = b.rotate_left(5);
    *a = a.wrapping_add(*b).rotate_left(16);
    (*a & 0xffff).wrapping_mul(range) >> 16
}

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> u8 {
        self.text.get(self.at).copied().unwrap_or(0)
    }

    fn take(&mut self) -> u8 {
        let byte = self.peek();
        if self.at < self.text.len() {
            self.at += 1;
        }
        byte
    }

    /// F74: an optional `-` and decimal digits.
    fn number(&mut self) -> i32 {
        let sign = if self.peek() == b'-' {
            self.at += 1;
            -1
        } else {
            1
        };
        let mut value: i32 = 0;
        while self.peek().is_ascii_digit() {
            value = value
                .wrapping_mul(10)
                .wrapping_add(i32::from(self.take() - b'0'));
        }
        value.wrapping_mul(sign)
    }

    /// F75.
    fn seed(&mut self, macros: &mut dyn Macros) -> i32 {
        match self.take() {
            kind @ (b'G' | b'L') => {
                let n = self.number();
                macros.seed(kind, n)
            }
            b'R' => macros.seed(b'R', 0),
            _ => 0,
        }
    }

    /// `(a,b)` after a seed: skip a byte, number, skip, number, skip.
    fn bounds(&mut self) -> (i32, i32) {
        self.take();
        let low = self.number();
        self.take();
        let high = self.number();
        self.take();
        (low, high)
    }
}

/// F77: expand every `[` macro in `text`; `0xd5` becomes `'`.
pub fn expand(text: &[u8], macros: &mut dyn Macros) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut reader = Reader { text, at: 0 };
    while reader.at < text.len() {
        let byte = reader.take();
        match byte {
            0xd5 => out.push(b'\''),
            b'[' => macro_(&mut reader, &mut out, macros),
            _ => out.push(byte),
        }
    }
    out
}

/// F76 at a `[` (already read).
fn macro_(reader: &mut Reader<'_>, out: &mut Vec<u8>, macros: &mut dyn Macros) {
    let start = reader.at;
    let code = [reader.take(), reader.take()];
    match &code {
        b"Gr" => {
            let n = reader.number();
            out.push(2);
            out.push(n as u8);
        }
        b"Mi" => {
            let word = reader.seed(macros);
            reader.take();
            let mut name = Vec::new();
            while !matches!(reader.peek(), 0 | b')') {
                name.push(reader.take());
            }
            reader.take();
            let name = String::from_utf8_lossy(&name).into_owned();
            out.extend(macros.query(word, &name));
        }
        b"Ca" => {
            let cash = macros.cash();
            out.extend(format!("${}.{}", cash / 10, (cash % 10).abs()).bytes());
        }
        b"Fm" | b"Ff" | b"Cr" | b"Co" => {
            let seed = reader.seed(macros);
            let kind = match &code {
                b"Fm" => 1,
                b"Ff" => 2,
                _ => 12,
            };
            out.extend(macros.name(kind, seed));
        }
        b"Dr" | b"Nu" => {
            let seed = reader.seed(macros);
            let (low, high) = reader.bounds();
            let (mut a, mut b) = (seed as u32, seed as u32);
            let draw = f921(high.wrapping_sub(low) as u32, &mut a, &mut b) as i32;
            if &code == b"Dr" {
                let day = (macros.today() as i32).wrapping_add(low).wrapping_add(draw);
                out.extend(macros.date(day as u32));
            } else {
                out.extend(low.wrapping_add(draw).to_string().bytes());
            }
        }
        b"Pn" => out.extend(macros.commander()),
        _ => {
            // Not ported yet: keep the macro as written.
            if is_known(&code) {
                out.push(b'[');
                reader.at = start;
            } else {
                // F76's unknown code: the `[` stays, the two letters go.
                out.push(b'[');
            }
        }
    }
}

/// `DATA_102307`'s two-letter codes.
fn is_known(code: &[u8; 2]) -> bool {
    const CODES: [&[u8; 2]; 26] = [
        b"Gr", b"Mi", b"Ca", b"Fm", b"Ff", b"Cr", b"Co", b"Dr", b"Nu", b"%i", b"Ri", b"Rf", b"Re",
        b"Sh", b"Si", b"Sl", b"Oo", b"Pn", b"Zz", b"Sy", b"Ci", b"Pl", b"Sc", b"Cs", b"Sr", b"di",
    ];
    CODES.contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;
    impl Macros for Fixed {
        fn seed(&mut self, kind: u8, n: i32) -> i32 {
            i32::from(kind) * 100 + n
        }
        fn query(&mut self, word: i32, name: &str) -> Vec<u8> {
            format!("<{name}:{word}>").into_bytes()
        }
        fn commander(&mut self) -> Vec<u8> {
            b"Jameson".to_vec()
        }
        fn cash(&mut self) -> i64 {
            1234
        }
        fn today(&mut self) -> u32 {
            1000
        }
        fn name(&mut self, kind: u32, seed: i32) -> Vec<u8> {
            format!("N{kind}/{seed}").into_bytes()
        }
        fn date(&mut self, day: u32) -> Vec<u8> {
            format!("D{day}").into_bytes()
        }
    }

    #[test]
    fn the_script_texts_macros_expand() {
        let text = b"Commander [Pn. By [MiR(Finn date1), $[MiL0(cash) and [Ca.";
        let out = expand(text, &mut Fixed);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "Commander Jameson. By <Finn date1:8200>, $<cash:7600> and $123.4."
        );
        let numbers = String::from_utf8(expand(b"[NuG2(10,20) [FfG1", &mut Fixed)).unwrap();
        let (low, rest) = numbers.split_once(' ').unwrap();
        let value: i32 = low.parse().unwrap();
        assert!((10..20).contains(&value));
        assert_eq!(rest, "N2/7101");
        // A macro not ported yet stays as written; an unknown one loses its
        // letters but keeps the bracket.
        assert_eq!(
            expand(b"[SyL1(I) [Qq!", &mut Fixed),
            b"[SyL1(I) [!".to_vec()
        );
        assert_eq!(expand(&[0xd5], &mut Fixed), b"'".to_vec());
    }
}
