//! The ship library's journal archive (cache v31, `base:text/journal`): the
//! 671 articles of `MISSION.DAT` that F78 reads for the "SHIP LIBRARY
//! ARCHIVES" viewer. Each article is one entry of `DATA_002292`'s file-offset
//! table; F78 reads it and unscrambles every big-endian 32-bit group by moving
//! the top four bits to bit 7 of each byte (bits 31/30/29/28 to 31/23/15/7)
//! and the four 7-bit fields (21..27, 14..20, 7..13, 0..6) to the bytes'
//! low bits, then copies up to the NUL.
//!
//! An article is the publication line, the headline line and the body, each
//! ended by `\n`. The text keeps F78's macros (`[SyL1(I)`, `[SlL1("his","her")`
//! and others from `DATA_102307`), which it expands when it shows the
//! article; [`Article`] gives them unexpanded.
//!
//! The asset is a little-endian `u16` count, then that many NUL-terminated
//! articles.

/// `DATA_002292`'s 672 offsets bound 671 articles.
pub const ARTICLES: usize = 671;

/// F78's unscrambling of one big-endian group.
pub fn unscramble(word: u32) -> u32 {
    ((word >> 31) & 1) << 31
        | ((word >> 30) & 1) << 23
        | ((word >> 29) & 1) << 15
        | ((word >> 28) & 1) << 7
        | ((word >> 21) & 0x7f) << 24
        | ((word >> 14) & 0x7f) << 16
        | ((word >> 7) & 0x7f) << 8
        | (word & 0x7f)
}

/// F78 on one article's bytes: every whole 4-byte group, up to the first NUL.
pub fn decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for group in bytes.chunks_exact(4) {
        let word = u32::from_be_bytes([group[0], group[1], group[2], group[3]]);
        out.extend_from_slice(&unscramble(word).to_be_bytes());
    }
    if let Some(end) = out.iter().position(|byte| *byte == 0) {
        out.truncate(end);
    }
    out
}

/// The archive as imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Journal<'a> {
    articles: Vec<&'a [u8]>,
}

/// One article's lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Article<'a> {
    pub publication: &'a [u8],
    pub headline: &'a [u8],
    pub body: &'a [u8],
}

impl<'a> Journal<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        let count = bytes
            .get(..2)
            .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
            .ok_or("journal count cut short")?;
        if count == 0 || count > 0x1000 {
            return Err(format!("bad journal article count {count}"));
        }
        let mut articles = Vec::with_capacity(count);
        let mut at = 2;
        for index in 0..count {
            let rest = bytes.get(at..).ok_or("journal cut short")?;
            let end = rest
                .iter()
                .position(|byte| *byte == 0)
                .ok_or_else(|| format!("journal article {index} unterminated"))?;
            articles.push(&rest[..end]);
            at += end + 1;
        }
        if at != bytes.len() {
            return Err("trailing bytes after the journal".to_owned());
        }
        Ok(Self { articles })
    }

    pub fn len(&self) -> usize {
        self.articles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.articles.is_empty()
    }

    /// Article `index`'s raw text.
    pub fn raw(&self, index: usize) -> Option<&'a [u8]> {
        self.articles.get(index).copied()
    }

    /// Article `index` split into its publication, headline and body.
    pub fn article(&self, index: usize) -> Option<Article<'a>> {
        let raw = self.raw(index)?;
        let mut lines = raw.splitn(3, |byte| *byte == b'\n');
        Some(Article {
            publication: lines.next().unwrap_or(&[]),
            headline: lines.next().unwrap_or(&[]),
            body: lines.next().unwrap_or(&[]),
        })
    }
}

/// The asset bytes for `articles`.
pub fn encode(articles: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let count = u16::try_from(articles.len()).map_err(|_| "too many journal articles")?;
    let mut out = count.to_le_bytes().to_vec();
    for (index, article) in articles.iter().enumerate() {
        if article.contains(&0) {
            return Err(format!("journal article {index} holds a NUL"));
        }
        out.extend_from_slice(article);
        out.push(0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The inverse of [`unscramble`], to build test input.
    fn scramble(word: u32) -> u32 {
        ((word >> 31) & 1) << 31
            | ((word >> 23) & 1) << 30
            | ((word >> 15) & 1) << 29
            | ((word >> 7) & 1) << 28
            | ((word >> 24) & 0x7f) << 21
            | ((word >> 16) & 0x7f) << 14
            | ((word >> 8) & 0x7f) << 7
            | (word & 0x7f)
    }

    #[test]
    fn f78_unscrambles_each_group() {
        let text = b"S.L.A.M\nHEAD\nBody.\0\0\0";
        let scrambled: Vec<u8> = text
            .chunks_exact(4)
            .flat_map(|g| scramble(u32::from_be_bytes([g[0], g[1], g[2], g[3]])).to_be_bytes())
            .collect();
        assert_eq!(decode(&scrambled), b"S.L.A.M\nHEAD\nBody.");
        for word in [0u32, 0xffff_ffff, 0x8000_0080, 0x1234_5678] {
            assert_eq!(unscramble(scramble(word)), word);
        }
    }

    #[test]
    fn articles_split_into_their_lines() {
        let bytes = encode(&[b"S.L.A.M\nHEAD\nBody.".to_vec(), b"J.F.\nTWO\n".to_vec()]).unwrap();
        let journal = Journal::new(&bytes).unwrap();
        assert_eq!(journal.len(), 2);
        let first = journal.article(0).unwrap();
        assert_eq!(
            (first.publication, first.headline, first.body),
            (&b"S.L.A.M"[..], &b"HEAD"[..], &b"Body."[..])
        );
        assert!(Journal::new(&bytes[..bytes.len() - 1]).is_err());
    }
}
