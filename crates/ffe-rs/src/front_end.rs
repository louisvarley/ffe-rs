//! The original's front end: the logo/intro screens and the start choice.
//!
//! The screens are data here, not drawing: F961 (the model and the primary
//! bitmap) and F962 (the credit lines) are driven by `GameTics`, and the start
//! choice follows the intro. `docs/notes/intro.md` records the derivations.
//!
//! The JJFFE acceptance splash (`ffe.asm:68301-68348`, the "This is a
//! replacement executable..." legal page) is deliberately **not** here: it is
//! the mod's, not the DOS release's, and the owner has ruled the DOS game the
//! source of truth. The front end starts at the original's logo screens.
//!
//! GameTics runs at 49,700 per second (`ffe_core`'s timer), so the original's
//! thresholds are 10.0 s (the primary bitmap), 10.75 s (the model switch and the
//! first credit) and 166.8 s (the intro loop restarts).

/// F961 draws model 177 while `GameTics < 0x8276e`; from there it draws 176.
pub const MODEL_FIRST: u32 = 177;
pub const MODEL_SECOND: u32 = 176;
/// F961's `F1575` background arguments: `(0,0,0)` for model 177
/// (`ffe.asm:71660-71668`) and `(0,0,4)` for model 176 (`71689-71695`).
pub const MODEL_FIRST_BACKGROUND: [u8; 3] = [0, 0, 0];
pub const MODEL_SECOND_BACKGROUND: [u8; 3] = [0, 0, 4];
/// The tics at which the second model takes over (`F961`'s `0x8276e`).
pub const MODEL_SWITCH: u32 = 0x8276e;
/// F962 blits the primary bitmap 0x42 at y = 0x9e once `GameTics > 0x795cc`.
pub const PRIMARY_BITMAP: u32 = 0x42;
pub const PRIMARY_BITMAP_Y: i32 = 0x9e;
pub const PRIMARY_BITMAP_FROM: u32 = 0x795cc;
/// F961 clears the buffer and restarts the clock at `0x7e7b76` tics.
pub const INTRO_END: u32 = 0x7e7b76;
/// F962 reads the input ("V" for the version) from `0x719668` tics on (DOS
/// `FIRSTENC.EXE` 0x2f6a1). The version itself is the DOS line.
pub const INTRO_INPUT_FROM: u32 = 0x719668;
/// The boot clock's first value (DOS `0x43c499`, `[ebx] = 0x400`).
pub const BOOT_TICS: u32 = 0x400;
/// Tics the original's timer adds per 20 ms frame at 1x (`DATA_007706` 0x3e2).
pub const TICS_PER_FRAME: u32 = 0x3e2;

/// One credit line: the `GameTics` window F962 draws it in, and its text call's
/// arguments. `first` is inclusive and `ends` (when present) exclusive; the
/// Journal Entries line's original test is `> 0x6d0954`, so its `first` is the
/// next tic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntroLine {
    /// The original's string code (`0x941b..0x942c`, or `0x942d` for the version).
    pub code: u32,
    pub first: u32,
    pub ends: Option<u32>,
    pub colour: i32,
    pub x: i32,
    pub y: i32,
    pub wrap: i32,
    /// The raw string's index in `base:text/intro-credits` (F1397's SysMenu
    /// table order; entry 18 is the version line).
    pub string: usize,
}

impl IntroLine {
    pub fn visible(&self, tics: u32) -> bool {
        tics >= self.first
            && tics >= MODEL_SWITCH
            && self.ends.is_none_or(|ends| tics < ends)
            && tics < INTRO_END
    }
}

/// F962's eighteen credit calls (`ffe.asm:71796-72058`, strings `0x941b..0x942c`),
/// at their exact thresholds, positions and wrap widths. `string` indexes the
/// raw DOS bytes in `base:text/intro-credits` (cache v27); the renderer applies
/// their layout codes (see [`crate::intro_text`]).
pub static INTRO_LINES: [IntroLine; 18] = [
    IntroLine {
        code: 0x941b,
        string: 0,
        first: MODEL_SWITCH,
        ends: Some(0xfbd3a),
        colour: 15,
        x: 90,
        y: 15,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 1,
        string: 1,
        first: 0x181534,
        ends: Some(0x1ee8d2),
        colour: 15,
        x: 2,
        y: 68,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 2,
        string: 2,
        first: 0x212f5c,
        ends: Some(0x24fa42),
        colour: 15,
        x: 245,
        y: 20,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 3,
        string: 3,
        first: 0x2375e6,
        ends: Some(0x2740cc),
        colour: 15,
        x: 10,
        y: 115,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 4,
        string: 4,
        first: 0x2d523c,
        ends: Some(0x34e808),
        colour: 15,
        x: 240,
        y: 15,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 5,
        string: 5,
        first: 0x32a17e,
        ends: Some(0x372e92),
        colour: 15,
        x: 5,
        y: 125,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 6,
        string: 6,
        first: 0x3bbba6,
        ends: Some(0x3f868c),
        colour: 15,
        x: 245,
        y: 15,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 7,
        string: 7,
        first: 0x48a0b4,
        ends: Some(0x4d2dc8),
        colour: 15,
        x: 20,
        y: 20,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 8,
        string: 8,
        first: 0x4ae73e,
        ends: Some(0x4f7452),
        colour: 15,
        x: 240,
        y: 115,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 9,
        string: 9,
        first: 0x4d2dc8,
        ends: Some(0x50f8ae),
        colour: 15,
        x: 20,
        y: 20,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 10,
        string: 10,
        first: 0x588e7a,
        ends: Some(0x5d1b8e),
        colour: 15,
        x: 5,
        y: 125,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 11,
        string: 11,
        first: 0x5ad504,
        ends: Some(0x5e9fea),
        colour: 15,
        x: 245,
        y: 10,
        wrap: 0,
    },
    IntroLine {
        code: 0x9428,
        string: 13,
        first: 0x626ad0,
        ends: Some(0x67ba12),
        colour: 15,
        x: 20,
        y: 100,
        wrap: 0,
    },
    IntroLine {
        code: 0x9427,
        string: 12,
        first: 0x657388,
        ends: Some(0x693e6e),
        colour: 15,
        x: 140,
        y: 10,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 14,
        string: 14,
        first: 0x6b84f8,
        ends: Some(0x70d43a),
        colour: 15,
        x: 5,
        y: 20,
        wrap: 0,
    },
    // The original tests this one with `jna` (<=), so it starts one tic later.
    IntroLine {
        code: 0x941b + 15,
        string: 15,
        first: 0x6d0955,
        ends: Some(0x725896),
        colour: 15,
        x: 220,
        y: 20,
        wrap: 0,
    },
    IntroLine {
        code: 0x941b + 16,
        string: 16,
        first: 0x77a7d8,
        ends: None,
        colour: 15,
        x: 5,
        y: 10,
        wrap: 310,
    },
    IntroLine {
        code: 0x941b + 17,
        string: 17,
        first: 0x77a7d8,
        ends: None,
        colour: 15,
        x: 5,
        y: 68,
        wrap: 310,
    },
];

/// The version line 'V' draws (`0x942d`): x 10, y 60, wrap 310, colour 16.
/// **JJFFE-only:** the string is the mod's, so it is available to the reference
/// oracle but is not part of the DOS front end the default boot shows.
pub const VERSION_TEXT: &str = "English Release 1.02 - 21/04/95";
pub const VERSION_LINE: IntroLine = IntroLine {
    code: 0x942d,
    string: 18,
    first: INTRO_INPUT_FROM,
    ends: None,
    colour: 16,
    x: 10,
    y: 60,
    wrap: 310,
};

/// The intro screen's draw state at one `GameTics`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntroFrame {
    pub tics: u32,
    /// 177 before `0x8276e`, 176 from there; 0 once the intro restarts.
    pub model: u32,
    /// F961's global 0: `tics >> 10` (less the switch for model 176).
    pub animation: u16,
    /// F962's primary bitmap 0x42 at y 158.
    pub bitmap: bool,
    /// F961's clear at `0x7e7b76`: the loop restarts.
    pub finished: bool,
    /// The credit lines this tic draws, in F962's order. F962 returns early
    /// (before any credit) while `GameTics < 0x8276e`, so the first credit's
    /// window starts at the model switch, not at 0.
    pub lines: Vec<&'static IntroLine>,
    /// F961's F1575 argument (nibble RGB): `(0,0,0)` for model 177 and
    /// `(0,0,4)` for model 176 (`ffe.asm:71660-71695`). The runtime palette
    /// resolves it to the clear colour.
    pub background: [u8; 3],
}

/// The intro's deterministic timeline.
pub struct IntroTimeline;

impl IntroTimeline {
    /// What F961/F962 draw at `tics`.
    pub fn at(tics: u32) -> IntroFrame {
        if tics >= INTRO_END {
            // F961 clears the buffer and sets the clock back to 0 here.
            return IntroFrame {
                tics,
                model: 0,
                animation: 0,
                bitmap: false,
                finished: true,
                lines: Vec::new(),
                // The last F1575 colour (model 176's) is still in the palette
                // when F961 clears and flips at the loop point.
                background: MODEL_SECOND_BACKGROUND,
            };
        }
        let (model, animation) = if tics < MODEL_SWITCH {
            (MODEL_FIRST, (tics >> 10) as u16)
        } else {
            (MODEL_SECOND, ((tics - MODEL_SWITCH) >> 10) as u16)
        };
        IntroFrame {
            tics,
            model,
            animation,
            bitmap: tics > PRIMARY_BITMAP_FROM,
            finished: false,
            lines: INTRO_LINES
                .iter()
                .filter(|line| line.visible(tics))
                .collect(),
            background: if tics < MODEL_SWITCH {
                MODEL_FIRST_BACKGROUND
            } else {
                MODEL_SECOND_BACKGROUND
            },
        }
    }

    /// Whether the version line is drawn: F962 reads the input only from
    /// `0x719668` and the action is 'V' (0x56). The string is the DOS line
    /// (`0x942d` at file `0xbd951`).
    pub fn version(tics: u32, key: Option<char>) -> bool {
        tics >= INTRO_INPUT_FROM && key == Some('V')
    }

    /// Advance `GameTics` by a frame's elapsed milliseconds (F961's caller adds
    /// `elapsed_ms * 0x3e2 / 20`). At `INTRO_END` the original clears, flips,
    /// clears again and starts the next pass at 0.
    pub fn advance(tics: u32, elapsed_ms: u32) -> u32 {
        let delta = (u64::from(elapsed_ms) * u64::from(TICS_PER_FRAME) / 20) as u32;
        let next = tics.saturating_add(delta);
        if next >= INTRO_END {
            0
        } else {
            next
        }
    }
}

/// What an intro key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroAction {
    /// 'V': show [`VERSION_LINE`] for this frame (DOS gate `0x719668`).
    Version,
    /// A start choice. `requested` is the key's number; only 1 is implemented, so
    /// `chosen` is 1 and `notice` explains the substitution for 2..5.
    Start {
        requested: u8,
        chosen: u8,
        notice: Option<&'static str>,
    },
    /// The DOS boot loop's "any other key" arm: it leaves with the default start
    /// (`F147(0)`), F998's type-0 Saker Mk III at Hope in Gateway.
    Skip {
        chosen: u8,
        notice: Option<&'static str>,
    },
}

/// The front end's input after the logos: 'V' during the intro, then the start
/// choice. (The original's logo sequence draws itself; this is the input side.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Intro;

impl Intro {
    pub fn new() -> Self {
        Self
    }

    /// A key press at `tics`. The start choice is read every frame from
    /// `0x400` on (`ffe.asm:68443-68455`); it is not gated on the intro's loop
    /// point. 'V' latches the version from `0x719668` and is otherwise a no-op.
    /// Any other key is the DOS default (`F147(0)`): the Saker Mk III on Hope in
    /// Gateway. '1' keeps the verified Ross 154 start; 2-5 are unverified and
    /// map to the checked type 1.
    pub fn key(&self, tics: u32, key: char) -> Option<IntroAction> {
        match key {
            '1'..='5' => {
                let requested = key as u8 - b'0';
                Some(IntroAction::Start {
                    requested,
                    chosen: 1,
                    notice: (requested != 1)
                        .then_some("start types 2-5 are unverified; starting Ross 154 (type 1)"),
                })
            }
            'V' => IntroTimeline::version(tics, Some('V')).then_some(IntroAction::Version),
            _ => Some(IntroAction::Skip {
                chosen: crate::navigation::DEFAULT_START_TYPE,
                notice: None,
            }),
        }
    }
}
