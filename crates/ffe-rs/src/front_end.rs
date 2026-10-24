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
/// The tics at which the second model takes over (`F961`'s `0x8276e`).
pub const MODEL_SWITCH: u32 = 0x8276e;
/// F962 blits the primary bitmap 0x42 at y = 0x9e once `GameTics > 0x795cc`.
pub const PRIMARY_BITMAP: u32 = 0x42;
pub const PRIMARY_BITMAP_Y: i32 = 0x9e;
pub const PRIMARY_BITMAP_FROM: u32 = 0x795cc;
/// F961 clears the buffer and restarts the clock at `0x7e7b76` tics.
pub const INTRO_END: u32 = 0x7e7b76;
/// The input ("V" for the version) is read from `0x8276e` tics on.
pub const INTRO_INPUT_FROM: u32 = MODEL_SWITCH;

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
    pub text: &'static str,
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
/// at their exact thresholds, positions and wrap widths. The strings are the
/// original's, stripped to plain text: line breaks kept, the in-line font codes
/// (0xfd/0xfe) become a space.
pub static INTRO_LINES: [IntroLine; 18] = [
    IntroLine {
        code: 0x941b,
        first: MODEL_SWITCH,
        ends: Some(0xfbd3a),
        colour: 15,
        x: 90,
        y: 15,
        wrap: 0,
        text: "A FRONTIER DEVELOPMENTS PRODUCTION Copyright @ David Braben 1995, all rights reserved.",
    },
    IntroLine {
        code: 0x941b + 1,
        first: 0x181534,
        ends: Some(0x1ee8d2),
        colour: 15,
        x: 2,
        y: 68,
        wrap: 0,
        text: "Programming:\n  David Braben\n  Andrew Dunn\n  Mike Edie\n  Peter Elliot-Green\n  Peter Irvin\n  Ran Mokady\n  Jonathan Roach\n  Chris Sawyer",
    },
    IntroLine {
        code: 0x941b + 2,
        first: 0x212f5c,
        ends: Some(0x24fa42),
        colour: 15,
        x: 245,
        y: 20,
        wrap: 0,
        text: "Shapes:\n  Aggy Finn\n  David Braben\n  Jonathan Griffiths\n  Peter Irvin",
    },
    IntroLine {
        code: 0x941b + 3,
        first: 0x2375e6,
        ends: Some(0x2740cc),
        colour: 15,
        x: 10,
        y: 115,
        wrap: 0,
        text: "Story:\n  Manda Scott\n  David Braben",
    },
    IntroLine {
        code: 0x941b + 4,
        first: 0x2d523c,
        ends: Some(0x34e808),
        colour: 15,
        x: 240,
        y: 15,
        wrap: 0,
        text: "Music:\n  The Quality Quartet\n  Dave Lowe",
    },
    IntroLine {
        code: 0x941b + 5,
        first: 0x32a17e,
        ends: Some(0x372e92),
        colour: 15,
        x: 5,
        y: 125,
        wrap: 0,
        text: "Sound Drivers:\n  Human Machine Interfaces Inc\n  This product features the LoudMouth MegaPatch(tm) Sound Bank",
    },
    IntroLine {
        code: 0x941b + 6,
        first: 0x3bbba6,
        ends: Some(0x3f868c),
        colour: 15,
        x: 245,
        y: 15,
        wrap: 0,
        text: "Intro Sequence:\n  Aggy Finn",
    },
    IntroLine {
        code: 0x941b + 7,
        first: 0x48a0b4,
        ends: Some(0x4d2dc8),
        colour: 15,
        x: 20,
        y: 20,
        wrap: 0,
        text: "Ship Interiors:\n  Danny Flynn",
    },
    IntroLine {
        code: 0x941b + 8,
        first: 0x4ae73e,
        ends: Some(0x4f7452),
        colour: 15,
        x: 240,
        y: 115,
        wrap: 0,
        text: "Planet Design:\n  Jonathan Roach\n  David Braben",
    },
    IntroLine {
        code: 0x941b + 9,
        first: 0x4d2dc8,
        ends: Some(0x50f8ae),
        colour: 15,
        x: 20,
        y: 20,
        wrap: 0,
        text: "Other Graphics:\n  Aggy Finn\n  David Braben",
    },
    IntroLine {
        code: 0x941b + 10,
        first: 0x588e7a,
        ends: Some(0x5d1b8e),
        colour: 15,
        x: 5,
        y: 125,
        wrap: 0,
        text: "Video Production:\n  Dave Jordan\n  Manda Scott",
    },
    IntroLine {
        code: 0x941b + 11,
        first: 0x5ad504,
        ends: Some(0x5e9fea),
        colour: 15,
        x: 245,
        y: 10,
        wrap: 0,
        text: "Video Post Production:\n  Kathy Braben",
    },
    IntroLine {
        code: 0x9428,
        first: 0x626ad0,
        ends: Some(0x67ba12),
        colour: 15,
        x: 20,
        y: 100,
        wrap: 0,
        text: "Journal Entries:\n   Manda Scott\n   Julian Flood\n   Jude Jones\n   S.L.A.M.\n Susannah Oliver\nTony Gallucci\nMartyn Taylor",
    },
    IntroLine {
        code: 0x9427,
        first: 0x657388,
        ends: Some(0x693e6e),
        colour: 15,
        x: 140,
        y: 10,
        wrap: 0,
        text: "Representation:\n  Jacqui Lyons of Marjacq",
    },
    IntroLine {
        code: 0x941b + 14,
        first: 0x6b84f8,
        ends: Some(0x70d43a),
        colour: 15,
        x: 5,
        y: 20,
        wrap: 0,
        text: "Video Actors:\n  Elizabeth Arbaud\n  Peter Edan\n  Tony Gallucci\n  Jason Graham\n  Steve Gorman\n  Sarah Johnson\n  Dave Jordan\n  Brendan Kavanaugh\n  Clyde Kellett\n  Susan Kelly\n  Dorian McFarland",
    },
    // The original tests this one with `jna` (<=), so it starts one tic later.
    IntroLine {
        code: 0x941b + 15,
        first: 0x6d0955,
        ends: Some(0x725896),
        colour: 15,
        x: 220,
        y: 20,
        wrap: 0,
        text: "Michel Moalem\nJason Myers\nRob Patterson\nChris Paton\nPiers Pitcairn\nPooni\nPhil Purves\nMark Reading\nThomas Skinner\nRose Thomas\nKeith Vogel\nSimon Watkins",
    },
    IntroLine {
        code: 0x941b + 16,
        first: 0x77a7d8,
        ends: None,
        colour: 15,
        x: 5,
        y: 10,
        wrap: 310,
        text: "\"First Encounters\" is the sequel to \"Frontier\" - the UK's best selling computer game of 1993.\n\n       \"Frontier\" in turn was the sequel to the best seller \"Elite\", first released in 1984.",
    },
    IntroLine {
        code: 0x941b + 17,
        first: 0x77a7d8,
        ends: None,
        colour: 15,
        x: 5,
        y: 68,
        wrap: 310,
        text: " First Encounters @ David Braben 1995 Frontier @ David Braben 1993 Elite  @ David Braben and Ian Bell 1984",
    },
];

/// The version line 'V' draws (`0x942d`): x 10, y 60, wrap 310, colour 16.
/// **JJFFE-only:** the string is the mod's, so it is available to the reference
/// oracle but is not part of the DOS front end the default boot shows.
pub const VERSION_TEXT: &str = "English Release 1.1 (Remastered Version)";
pub const VERSION_LINE: IntroLine = IntroLine {
    code: 0x942d,
    first: INTRO_INPUT_FROM,
    ends: None,
    colour: 16,
    x: 10,
    y: 60,
    wrap: 310,
    text: VERSION_TEXT,
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
        }
    }

    /// Whether the version line is drawn: F962 reads the input only from
    /// `0x8276e` and the action is 'V' (0x56).
    pub fn version(tics: u32, key: Option<char>) -> bool {
        tics >= INTRO_INPUT_FROM && key == Some('V')
    }
}

/// What an intro key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroAction {
    /// 'V': show [`VERSION_LINE`] for this frame.
    Version,
    /// A start choice. `requested` is the key's number; only 1 is implemented, so
    /// `chosen` is 1 and `notice` explains the substitution for 2..5.
    Start {
        requested: u8,
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

    /// A key press at `tics`. The version key is F962's own test (at and after
    /// `0x8276e`); the start choice belongs after the intro has restarted at
    /// `0x7e7b76`. Keys 2-5 are the JJFFE mapping, not verified against the DOS
    /// release, so they map to the verified start type 1 with a note.
    pub fn key(&self, tics: u32, key: char) -> Option<IntroAction> {
        if IntroTimeline::version(tics, Some(key)) {
            return Some(IntroAction::Version);
        }
        if tics < INTRO_END {
            return None;
        }
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
            _ => None,
        }
    }
}
