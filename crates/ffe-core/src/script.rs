//! The original's mission scripts (cache v32, `base:script/missions`): the
//! data graph under `DATA_001697` ("Mission struct table") that the
//! interpreter F41 and the evaluator F31 run. See `docs/notes/mission-scripts.md`.
//!
//! The import copies the executable's script region as it is and rewrites
//! every relocated pointer in it: a pointer into the region becomes
//! `POINTER_BLOB | offset`, a pointer to the per-mission static words in BSS
//! becomes `POINTER_STATIC | word`, and a null pointer stays 0.
//!
//! Layouts (from F51-F54 and F33/F47):
//!
//! - **Mission struct** (`DATA_001697[n]`): `+0x00` the parameter target
//!   stream (F52 assigns each parameter through F37 from it), `+0x08` the
//!   parameter count, `+0x0c` the kill state (-1: none), `+0x10` the number
//!   of local words, `+0x14` the static words pointer, `+0x18` their count,
//!   `+0x1c..` the state pointers (state 0 first).
//! - **State**: `+0x00` the entry stream, `+0x04` the number of conditions,
//!   `+0x08..` one stream per condition: an F31 expression followed by the
//!   F41 statements it runs.

use std::collections::BTreeSet;

pub mod brackets;
pub mod engine;
pub mod menu;
pub mod run;

/// Asset magic.
pub const MAGIC: &[u8; 4] = b"FFSC";
/// Tag of a rewritten pointer into the region.
pub const POINTER_BLOB: u32 = 0x4000_0000;
/// Tag of a rewritten pointer to the static words.
pub const POINTER_STATIC: u32 = 0x8000_0000;
/// Missions in `DATA_001697` (floppy 1.02).
pub const MISSIONS: usize = 105;

/// `DATA_001698`'s 68 names the `[Mi...(name)` macro asks the scripts for
/// (F64): each name's id goes to global 6 (cache v34, `base:script/queries`).
pub const QUERIES: usize = 0x44;

/// The query names: `u16` count, then per name an `i32` id and the NUL-ended
/// name.
pub fn encode_queries(queries: &[(i32, Vec<u8>)]) -> Vec<u8> {
    let mut out = (queries.len() as u16).to_le_bytes().to_vec();
    for (id, name) in queries {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(name);
        out.push(0);
    }
    out
}

/// Read the query names.
pub fn decode_queries(bytes: &[u8]) -> Result<Vec<(i32, String)>, String> {
    let count = bytes
        .get(..2)
        .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
        .ok_or("script queries cut short")?;
    let mut at = 2;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let id = bytes
            .get(at..at + 4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or("script queries cut short")?;
        at += 4;
        let rest = bytes.get(at..).ok_or("script queries cut short")?;
        let end = rest
            .iter()
            .position(|b| *b == 0)
            .ok_or("unterminated script query")?;
        out.push((id, String::from_utf8_lossy(&rest[..end]).into_owned()));
        at += end + 1;
    }
    if at != bytes.len() {
        return Err("trailing bytes after the script queries".to_owned());
    }
    Ok(out)
}

/// F63/F64: the id of query `name` (case-insensitive), if any.
pub fn query_id(queries: &[(i32, String)], name: &str) -> Option<i32> {
    queries
        .iter()
        .find(|(_, query)| query.eq_ignore_ascii_case(name))
        .map(|(id, _)| *id)
}

/// A decoded pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pointer {
    Null,
    /// An offset into the script region.
    Blob(u32),
    /// A word index into the static words.
    Static(u32),
}

/// The imported script region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Tokens and opcodes the original has no case for, met by the skip
    /// walkers since the last [`Image::take_quirks`].
    quirks: std::cell::Cell<u32>,
    blob: Vec<u8>,
    table: u32,
    missions: u32,
    static_words: u32,
    pointers: BTreeSet<u32>,
}

/// F31's operand count for each expression token: `(expressions, dwords)`,
/// counted from the evaluator's cases. Tokens 0 and 1 have no case.
pub const EXPRESSION_SHAPES: [(u8, u8); 0x44] = {
    let mut shapes = [(0u8, 0u8); 0x44];
    let mut token = 2;
    while token <= 6 {
        shapes[token] = (0, 1);
        token += 1;
    }
    let mut token = 7;
    while token < 0x44 {
        shapes[token] = (TOKENS[token].0, 0);
        token += 1;
    }
    // F31 reads two expressions where the subscription table says one.
    shapes[0x28] = (2, 0);
    shapes[0x29] = (2, 0);
    shapes[0x2a] = (2, 0);
    shapes[0x2b] = (2, 0);
    shapes[0x2c] = (2, 0);
    shapes
};

/// The token records at `DATA_001701` (16 bytes each): the child count F32
/// walks, the event a condition reading the token subscribes to, and how
/// F32 forms the event's two keys (`a`: 0 none, 1 the mission, 2 the
/// instance, 3 an expression; `b`: 0 none, 1 the dword, 2 an expression).
pub const TOKENS: [(u8, u8, u8, u8); 0x44] = [
    (0, 0, 0, 0),
    (1, 0, 0, 0),
    (0, 0, 0, 0),
    (0, 1, 0, 1),
    (0, 2, 0, 1),
    (0, 3, 1, 1),
    (0, 4, 2, 1),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (2, 0, 0, 0),
    (1, 5, 0, 2),
    (1, 5, 0, 2),
    (2, 6, 3, 2),
    (1, 21, 0, 2),
    (2, 7, 3, 2),
    (1, 8, 0, 2),
    (1, 9, 0, 2),
    (1, 10, 0, 2),
    (1, 11, 0, 2),
    (2, 12, 3, 2),
    (1, 13, 0, 2),
    (1, 13, 0, 2),
    (1, 14, 0, 2),
    (1, 15, 0, 2),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 16, 0, 2),
    (1, 0, 0, 0),
    (3, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 17, 0, 2),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 22, 0, 2),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (3, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 23, 0, 2),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (1, 0, 0, 0),
    (2, 24, 3, 2),
    (1, 0, 0, 0),
];

/// One item of a statement's operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    /// An F31 expression.
    Expression,
    /// An F20 dword.
    Dword,
    /// A NUL-terminated string (opcode 10's journal name).
    String,
    /// An F37 assignment target.
    Target,
    /// Opcode 1's mission parameters (the mission's own count of
    /// expressions, then its target).
    Parameters,
}

/// F41's operands for each statement opcode (0..=0x3c).
pub fn statement_shape(opcode: u8) -> Option<&'static [Operand]> {
    use Operand::{Dword as D, Expression as E, Parameters as P, String as S, Target as L};
    const E2: &[Operand] = &[E, E];
    const E3: &[Operand] = &[E, E, E];
    Some(match opcode {
        0 | 5 | 6 | 34 => &[],
        1 => &[D, P],
        2 => &[E, D],
        3 | 9 => &[D],
        4 => &[E, E, E, E, E, E],
        7 | 19 => &[E, L],
        8 | 11 | 12 | 13 | 18 | 20 | 22 | 23 | 24 | 25 | 29 | 32 | 35 | 38 | 57 => &[E],
        10 => &[S, E, E, E, E, L],
        14 | 16 | 21 | 31 | 45 | 47 | 53 | 54 | 55 | 60 => E3,
        15 | 17 | 26 | 27 | 28 | 30 | 33 | 36 | 40 | 44 | 46 | 51 => E2,
        37 => &[L],
        39 => &[E, E, E, E, L],
        41 | 42 | 43 | 52 | 58 => &[E, E, L],
        48 => &[E, E, E, E, E, L],
        49 | 59 => &[E, E, E, E, E],
        50 => &[E, E, E, E, E, E, E, E, E, L],
        56 => &[E, E, E, E],
        _ => return None,
    })
}

/// F37's operands for each target token: `None` for a token F37 ignores
/// without reading anything.
pub fn target_shape(token: u8) -> Option<&'static [Operand]> {
    use Operand::{Dword as D, Expression as E};
    match token {
        3..=6 => Some(&[D]),
        0x1f | 0x23 | 0x24 | 0x30 | 0x38 | 0x3d | 0x3e | 0x41 => Some(&[E]),
        0x25 | 0x42 => Some(&[E, E]),
        _ => None,
    }
}

impl Image {
    pub fn new(bytes: &[u8]) -> Result<Self, String> {
        let word = |at: usize| -> Result<u32, String> {
            bytes
                .get(at..at + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or_else(|| "mission scripts cut short".to_owned())
        };
        if bytes.get(..4) != Some(MAGIC) {
            return Err("not a mission script image".to_owned());
        }
        let table = word(4)?;
        let missions = word(8)?;
        let static_words = word(12)?;
        let count = word(16)? as usize;
        if count > 0x10000 || missions == 0 || missions > 0x400 || static_words > 0x400 {
            return Err("bad mission script header".to_owned());
        }
        let mut pointers = BTreeSet::new();
        for index in 0..count {
            pointers.insert(word(20 + 4 * index)?);
        }
        let start = 20 + 4 * count;
        let length = word(start)? as usize;
        let blob = bytes
            .get(start + 4..start + 4 + length)
            .ok_or("mission script blob cut short")?
            .to_vec();
        if start + 4 + length != bytes.len() {
            return Err("trailing bytes after the mission scripts".to_owned());
        }
        let image = Self {
            quirks: std::cell::Cell::new(0),
            blob,
            table,
            missions,
            static_words,
            pointers,
        };
        for &at in &image.pointers {
            if at as usize + 4 > image.blob.len() {
                return Err(format!("pointer at {at:#x} outside the region"));
            }
            match image.decode(image.raw(at)?) {
                Some(Pointer::Blob(target)) if target as usize >= image.blob.len() => {
                    return Err(format!("pointer at {at:#x} leaves the region"));
                }
                Some(Pointer::Static(word)) if word >= image.static_words => {
                    return Err(format!("pointer at {at:#x} leaves the static words"));
                }
                None => return Err(format!("bad pointer at {at:#x}")),
                _ => {}
            }
        }
        for mission in 0..image.missions as usize {
            if !matches!(image.mission(mission), Ok(Pointer::Blob(_))) {
                return Err(format!("mission {mission} has no struct"));
            }
        }
        Ok(image)
    }

    /// Build the asset: `blob` is the region with its pointers rewritten.
    pub fn encode(
        blob: &[u8],
        table: u32,
        missions: u32,
        static_words: u32,
        pointers: &BTreeSet<u32>,
    ) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for value in [table, missions, static_words, pointers.len() as u32] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for at in pointers {
            out.extend_from_slice(&at.to_le_bytes());
        }
        out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        out.extend_from_slice(blob);
        out
    }

    pub fn len(&self) -> usize {
        self.blob.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blob.is_empty()
    }

    pub fn missions(&self) -> usize {
        self.missions as usize
    }

    pub fn static_words(&self) -> usize {
        self.static_words as usize
    }

    pub fn byte(&self, at: u32) -> Result<u8, String> {
        self.blob
            .get(at as usize)
            .copied()
            .ok_or_else(|| format!("script read past the region at {at:#x}"))
    }

    fn raw(&self, at: u32) -> Result<u32, String> {
        let at = at as usize;
        self.blob
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| format!("script read past the region at {at:#x}"))
    }

    fn decode(&self, value: u32) -> Option<Pointer> {
        match value {
            0 => Some(Pointer::Null),
            v if v & POINTER_STATIC != 0 => Some(Pointer::Static(v & !POINTER_STATIC)),
            v if v & POINTER_BLOB != 0 => Some(Pointer::Blob(v & !POINTER_BLOB)),
            _ => None,
        }
    }

    /// F20: the little-endian dword at `at`.
    pub fn dword(&self, at: u32) -> Result<i32, String> {
        self.raw(at).map(|value| value as i32)
    }

    /// The pointer word at `at` (which must be a relocated or null word).
    pub fn pointer(&self, at: u32) -> Result<Pointer, String> {
        let value = self.raw(at)?;
        if value != 0 && !self.pointers.contains(&at) {
            return Err(format!("no pointer at {at:#x}"));
        }
        self.decode(value)
            .ok_or_else(|| format!("bad pointer at {at:#x}"))
    }

    fn blob_pointer(&self, at: u32) -> Result<u32, String> {
        match self.pointer(at)? {
            Pointer::Blob(target) => Ok(target),
            other => Err(format!(
                "expected a script pointer at {at:#x}, found {other:?}"
            )),
        }
    }

    /// `DATA_001697[index]`.
    pub fn mission(&self, index: usize) -> Result<Pointer, String> {
        if index >= self.missions as usize {
            return Err(format!("no mission {index}"));
        }
        self.pointer(self.table + 4 * index as u32)
    }

    /// The struct of mission `index`.
    pub fn mission_struct(&self, index: usize) -> Result<Mission, String> {
        let base = match self.mission(index)? {
            Pointer::Blob(base) => base,
            other => return Err(format!("mission {index} is {other:?}")),
        };
        Ok(Mission {
            base,
            parameters: self.pointer(base)?,
            parameter_count: self.dword(base + 8)?,
            kill_state: self.dword(base + 0xc)?,
            locals: self.dword(base + 0x10)?,
            statics: self.pointer(base + 0x14)?,
            static_count: self.dword(base + 0x18)?,
        })
    }

    /// State `index` of mission struct `mission` (`+0x1c + 4 * index`).
    pub fn state(&self, mission: &Mission, index: i32) -> Result<u32, String> {
        let index = u32::try_from(index).map_err(|_| format!("bad state {index}"))?;
        self.blob_pointer(mission.base + 0x1c + 4 * index)
    }

    /// A state's entry stream (`+0`), or `None` for a null pointer.
    pub fn entry(&self, state: u32) -> Result<Option<u32>, String> {
        match self.pointer(state)? {
            Pointer::Null => Ok(None),
            Pointer::Blob(stream) => Ok(Some(stream)),
            other => Err(format!("state {state:#x} entry is {other:?}")),
        }
    }

    /// A state's condition count (`+4`).
    pub fn conditions(&self, state: u32) -> Result<usize, String> {
        let count = self.dword(state + 4)?;
        usize::try_from(count).map_err(|_| format!("state {state:#x} has {count} conditions"))
    }

    /// A state's condition stream `index` (`+8 + 4 * index`).
    pub fn condition(&self, state: u32, index: usize) -> Result<u32, String> {
        self.blob_pointer(state + 8 + 4 * index as u32)
    }

    /// The states of `mission` the engine can reach: state 0, the kill
    /// state, and every state an opcode 9 in a reachable stream names.
    pub fn reachable_states(&self, mission: &Mission) -> Result<Vec<i32>, String> {
        let mut found = std::collections::BTreeSet::from([0]);
        if mission.kill_state >= 0 {
            found.insert(mission.kill_state);
        }
        let mut pending: Vec<i32> = found.iter().copied().collect();
        while let Some(index) = pending.pop() {
            let state = self.state(mission, index)?;
            let mut streams = Vec::new();
            if let Some(entry) = self.entry(state)? {
                streams.push(entry);
            }
            for condition in 0..self.conditions(state)? {
                streams.push(self.skip_expression(self.condition(state, condition)?)?);
            }
            for mut at in streams {
                loop {
                    let (opcode, next) = self.skip_statement(at)?;
                    if opcode == 9 && found.insert(self.dword(at + 1)?) {
                        pending.push(self.dword(at + 1)?);
                    }
                    at = next;
                    if opcode == 0 {
                        break;
                    }
                }
            }
        }
        Ok(found.into_iter().collect())
    }

    /// Skip one F31 expression at `at`, returning the offset after it. A
    /// token with no evaluator case (0, 1, or above 0x43) consumes nothing
    /// beyond itself, as in the original, and counts as a quirk.
    pub fn skip_expression(&self, at: u32) -> Result<u32, String> {
        let token = self.byte(at)?;
        let Some(&(expressions, dwords)) = EXPRESSION_SHAPES.get(usize::from(token)) else {
            self.quirk();
            return Ok(at + 1);
        };
        if token < 2 {
            self.quirk();
            return Ok(at + 1);
        }
        let mut next = at + 1 + 4 * u32::from(dwords);
        for _ in 0..expressions {
            next = self.skip_expression(next)?;
        }
        Ok(next)
    }

    fn quirk(&self) {
        self.quirks.set(self.quirks.get() + 1);
    }

    /// The quirk count since the last call (see [`Image::skip_expression`]).
    pub fn take_quirks(&self) -> u32 {
        self.quirks.replace(0)
    }

    /// Skip one F37 target at `at`.
    pub fn skip_target(&self, at: u32) -> Result<u32, String> {
        let token = self.byte(at)?;
        let mut next = at + 1;
        for operand in target_shape(token).unwrap_or(&[]) {
            next = match operand {
                Operand::Dword => next + 4,
                _ => self.skip_expression(next)?,
            };
        }
        Ok(next)
    }

    /// Skip one statement at `at`, returning `(opcode, next)`.
    pub fn skip_statement(&self, at: u32) -> Result<(u8, u32), String> {
        let opcode = self.byte(at)?;
        // F41 ignores an opcode above 0x3c and reads on from the next byte.
        let Some(shape) = statement_shape(opcode) else {
            self.quirk();
            return Ok((opcode, at + 1));
        };
        let mut next = at + 1;
        let mut mission = None;
        for operand in shape {
            next = match operand {
                Operand::Expression => self.skip_expression(next)?,
                Operand::Dword => {
                    mission = Some(self.dword(next)?);
                    next + 4
                }
                Operand::String => {
                    let mut end = next;
                    while self.byte(end)? != 0 {
                        end += 1;
                    }
                    end + 1
                }
                Operand::Target => self.skip_target(next)?,
                Operand::Parameters => {
                    let index = mission.ok_or("parameters without a mission")?;
                    let target = self.mission_struct(
                        usize::try_from(index).map_err(|_| format!("bad mission {index}"))?,
                    )?;
                    let mut after = next;
                    for _ in 0..target.parameter_count.max(0) {
                        after = self.skip_expression(after)?;
                    }
                    self.skip_target(after)?
                }
            };
        }
        Ok((opcode, next))
    }

    /// Walk a statement stream to its terminating opcode 0, returning the
    /// offset just after it.
    pub fn skip_statements(&self, mut at: u32) -> Result<u32, String> {
        for _ in 0..0x10000 {
            let (opcode, next) = self.skip_statement(at)?;
            at = next;
            if opcode == 0 {
                return Ok(at);
            }
        }
        Err("statement stream does not end".to_owned())
    }
}

/// A mission struct's fixed fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mission {
    /// The struct's offset in the region (its identity as an event key).
    pub base: u32,
    pub parameters: Pointer,
    pub parameter_count: i32,
    pub kill_state: i32,
    pub locals: i32,
    pub statics: Pointer,
    pub static_count: i32,
}

impl Image {
    /// One expression as text: `(token child ...)`, constants in hex, `P n`
    /// player value, `G n` global, `S n` static, `L n` local.
    pub fn expression_text(&self, at: u32) -> Result<(String, u32), String> {
        let token = self.byte(at)?;
        if (2..=6).contains(&token) {
            let value = self.dword(at + 1)?;
            let text = match token {
                2 => format!("{value:#x}"),
                3 => format!("P{value}"),
                4 => format!("G{value}"),
                5 => format!("S{value}"),
                _ => format!("L{value}"),
            };
            return Ok((text, at + 5));
        }
        let Some(&(expressions, _)) = EXPRESSION_SHAPES.get(usize::from(token)) else {
            return Ok((format!("?{token:#x}"), at + 1));
        };
        if token < 2 {
            return Ok((format!("?{token:#x}"), at + 1));
        }
        let mut next = at + 1;
        let mut parts = vec![format!("{token:#x}")];
        for _ in 0..expressions {
            let (part, after) = self.expression_text(next)?;
            parts.push(part);
            next = after;
        }
        Ok((format!("({})", parts.join(" ")), next))
    }

    fn target_text(&self, at: u32) -> Result<(String, u32), String> {
        let token = self.byte(at)?;
        if (3..=6).contains(&token) {
            let value = self.dword(at + 1)?;
            let kind = ["P", "G", "S", "L"][usize::from(token - 3)];
            return Ok((format!("{kind}{value}"), at + 5));
        }
        let mut next = at + 1;
        let mut parts = vec![format!("T{token:#x}")];
        for _ in target_shape(token).unwrap_or(&[]) {
            let (part, after) = self.expression_text(next)?;
            parts.push(part);
            next = after;
        }
        Ok((parts.join(" "), next))
    }

    /// The statements at `at` up to opcode 0, one line each, prefixed with
    /// their offsets.
    pub fn listing(&self, mut at: u32) -> Result<Vec<String>, String> {
        let mut lines = Vec::new();
        for _ in 0..4096 {
            let opcode = self.byte(at)?;
            let start = at;
            let mut parts = vec![format!("{start:#06x}: {opcode}")];
            at += 1;
            let mut mission = None;
            for operand in statement_shape(opcode).unwrap_or(&[]) {
                match operand {
                    Operand::Expression => {
                        let (text, next) = self.expression_text(at)?;
                        parts.push(text);
                        at = next;
                    }
                    Operand::Dword => {
                        let value = self.dword(at)?;
                        mission = Some(value);
                        let shown = if opcode == 2 || opcode == 3 {
                            format!("->{:#06x}", (at as i64 + i64::from(value)) as u32)
                        } else {
                            format!("{value}")
                        };
                        parts.push(shown);
                        at += 4;
                    }
                    Operand::String => {
                        let mut text = Vec::new();
                        while self.byte(at)? != 0 {
                            text.push(self.byte(at)?);
                            at += 1;
                        }
                        at += 1;
                        parts.push(format!("{:?}", String::from_utf8_lossy(&text)));
                    }
                    Operand::Target => {
                        let (text, next) = self.target_text(at)?;
                        parts.push(format!("=> {text}"));
                        at = next;
                    }
                    Operand::Parameters => {
                        let index = mission.unwrap_or(0).max(0) as usize;
                        let count = self.mission_struct(index)?.parameter_count.max(0);
                        for _ in 0..count {
                            let (text, next) = self.expression_text(at)?;
                            parts.push(text);
                            at = next;
                        }
                        let (text, next) = self.target_text(at)?;
                        parts.push(format!("=> {text}"));
                        at = next;
                    }
                }
            }
            lines.push(parts.join(" "));
            if opcode == 0 {
                break;
            }
        }
        Ok(lines)
    }

    /// Every reachable state of `mission` as text.
    pub fn mission_listing(&self, mission: usize) -> Result<Vec<String>, String> {
        let record = self.mission_struct(mission)?;
        let mut out = vec![format!("mission {mission}: {record:?}")];
        for index in self.reachable_states(&record)? {
            let state = self.state(&record, index)?;
            out.push(format!("state {index}:"));
            if let Some(entry) = self.entry(state)? {
                out.push("  entry:".to_owned());
                out.extend(self.listing(entry)?.into_iter().map(|l| format!("    {l}")));
            }
            for condition in 0..self.conditions(state)? {
                let at = self.condition(state, condition)?;
                let (text, body) = self.expression_text(at)?;
                out.push(format!("  when {text}:"));
                out.extend(self.listing(body)?.into_iter().map(|l| format!("    {l}")));
            }
        }
        Ok(out)
    }
}

impl Image {
    /// How often each statement opcode and each expression token appears in
    /// every reachable stream (a static census for the port's notes).
    pub fn census(&self) -> Result<([u32; 256], [u32; 256]), String> {
        let mut statements = [0u32; 256];
        let mut tokens = [0u32; 256];
        fn expression(image: &Image, at: u32, tokens: &mut [u32; 256]) -> Result<u32, String> {
            let token = image.byte(at)?;
            tokens[usize::from(token)] += 1;
            let Some(&(expressions, dwords)) = EXPRESSION_SHAPES.get(usize::from(token)) else {
                return Ok(at + 1);
            };
            if token < 2 {
                return Ok(at + 1);
            }
            let mut next = at + 1 + 4 * u32::from(dwords);
            for _ in 0..expressions {
                next = expression(image, next, tokens)?;
            }
            Ok(next)
        }
        for mission in 0..self.missions() {
            let record = self.mission_struct(mission)?;
            for index in self.reachable_states(&record)? {
                let state = self.state(&record, index)?;
                let mut streams = Vec::new();
                if let Some(entry) = self.entry(state)? {
                    streams.push(entry);
                }
                for condition in 0..self.conditions(state)? {
                    let at = self.condition(state, condition)?;
                    streams.push(expression(self, at, &mut tokens)?);
                }
                for mut at in streams {
                    loop {
                        let opcode = self.byte(at)?;
                        statements[usize::from(opcode)] += 1;
                        // Count the operand expressions' tokens too.
                        if let Some(shape) = statement_shape(opcode) {
                            let mut next = at + 1;
                            for operand in shape {
                                next = match operand {
                                    Operand::Expression => expression(self, next, &mut tokens)?,
                                    _ => break,
                                };
                            }
                        }
                        let (_, next) = self.skip_statement(at)?;
                        at = next;
                        if opcode == 0 {
                            break;
                        }
                    }
                }
            }
        }
        Ok((statements, tokens))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A one-mission image: struct at 0, state at 0x20, entry stream at
    /// 0x2c, one condition at 0x40.
    pub(crate) fn tiny(entry: &[u8], condition: &[u8]) -> Vec<u8> {
        let mut blob = vec![0u8; 0x80];
        let mut pointers = BTreeSet::new();
        let put = |blob: &mut Vec<u8>, at: usize, value: u32| {
            blob[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        put(&mut blob, 0x0c, -1i32 as u32);
        put(&mut blob, 0x10, 2);
        put(&mut blob, 0x1c, POINTER_BLOB | 0x20);
        pointers.insert(0x1c);
        put(&mut blob, 0x20, POINTER_BLOB | 0x2c);
        pointers.insert(0x20);
        put(&mut blob, 0x24, 1);
        put(&mut blob, 0x28, POINTER_BLOB | 0x40);
        pointers.insert(0x28);
        blob[0x2c..0x2c + entry.len()].copy_from_slice(entry);
        blob[0x40..0x40 + condition.len()].copy_from_slice(condition);
        put(&mut blob, 0x7c, POINTER_BLOB);
        pointers.insert(0x7c);
        Image::encode(&blob, 0x7c, 1, 2, &pointers)
    }

    #[test]
    fn statements_and_expressions_skip_as_f41_and_f31_read_them() {
        // Entry: global 3 = 5 + local 1; stop.
        let entry = [7, 0x0d, 2, 5, 0, 0, 0, 6, 1, 0, 0, 0, 4, 3, 0, 0, 0, 0];
        // Condition: global 3 > 6, then goto state 0, stop.
        let condition = [0x13, 4, 3, 0, 0, 0, 2, 6, 0, 0, 0, 9, 0, 0, 0, 0, 0];
        let image = Image::new(&tiny(&entry, &condition)).unwrap();
        let mission = image.mission_struct(0).unwrap();
        assert_eq!(mission.locals, 2);
        let state = image.state(&mission, 0).unwrap();
        assert_eq!(
            image.skip_statements(image.entry(state).unwrap().unwrap()),
            Ok(0x2c + 18)
        );
        let body = image
            .skip_expression(image.condition(state, 0).unwrap())
            .unwrap();
        assert_eq!(body, 0x40 + 11);
        assert_eq!(image.reachable_states(&mission), Ok(vec![0]));
        assert_eq!(image.take_quirks(), 0);
        // A token with no case consumes only itself.
        assert_eq!(image.skip_expression(0x3f), Ok(0x40));
        assert_eq!(image.take_quirks(), 1);
    }

    #[test]
    fn bad_images_are_rejected() {
        let bytes = tiny(&[0], &[2, 1, 0, 0, 0, 0]);
        assert!(Image::new(&bytes).is_ok());
        assert!(Image::new(&bytes[..bytes.len() - 1]).is_err());
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert!(Image::new(&bad).is_err());
    }
}
