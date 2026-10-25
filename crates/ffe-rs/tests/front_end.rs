//! The intro's deterministic timeline and input: the data F961/F962 draw and
//! the boot loop's `GameTics`/key rules, checked against DOS `FIRSTENC.EXE`
//! (`docs/notes/intro.md`).
use ffe_rs::front_end::{
    Intro, IntroAction, IntroLine, IntroTimeline, BOOT_TICS, INTRO_END, INTRO_INPUT_FROM,
    INTRO_LINES, MODEL_FIRST, MODEL_SECOND, MODEL_SWITCH, PRIMARY_BITMAP_FROM, TICS_PER_FRAME,
    VERSION_LINE, VERSION_TEXT,
};

/// The DOS release's front end has no JJFFE legal/acceptance page. The version
/// line is the DOS one, not the mod's remastered string.
#[test]
fn the_front_end_has_no_jjffe_splash_or_version() {
    assert!(!INTRO_LINES.iter().any(|line| line.string > 18));
    assert_eq!(VERSION_TEXT, "English Release 1.02 - 21/04/95");
    assert!(!VERSION_TEXT.contains("Remastered"));
    assert!(!VERSION_TEXT.contains("1.1"));
}

#[test]
fn the_dos_version_line_and_its_input_gate() {
    assert_eq!(
        (
            VERSION_LINE.x,
            VERSION_LINE.y,
            VERSION_LINE.wrap,
            VERSION_LINE.colour,
            VERSION_LINE.string
        ),
        (10, 60, 310, 16, 18)
    );
    assert_eq!(INTRO_INPUT_FROM, 0x719668);
    assert!(!IntroTimeline::version(INTRO_INPUT_FROM - 1, Some('V')));
    assert!(IntroTimeline::version(INTRO_INPUT_FROM, Some('V')));
    assert!(!IntroTimeline::version(INTRO_INPUT_FROM, Some('v')));
}

#[test]
fn start_choices_are_read_every_frame_and_skip_maps_to_the_verified_type() {
    let intro = Intro::new();
    // The boot loop reads the action from the first frame (0x400), not only
    // after the intro's loop point (DOS `0x43c4f4`..`0x43c51b`).
    assert_eq!(
        intro.key(BOOT_TICS, '1'),
        Some(IntroAction::Start {
            requested: 1,
            chosen: 1,
            notice: None
        })
    );
    for key in ['2', '3', '4', '5'] {
        assert_eq!(
            intro.key(BOOT_TICS, key),
            Some(IntroAction::Start {
                requested: key as u8 - b'0',
                chosen: 1,
                notice: Some("start types 2-5 are unverified; starting Ross 154 (type 1)")
            })
        );
    }
    // DOS accepts 1-5 at any tics, including the last frame of the loop.
    assert!(matches!(
        intro.key(INTRO_END - 1, '1'),
        Some(IntroAction::Start { .. })
    ));
    // Any other key is F147(0); the port uses the verified type 1.
    assert_eq!(
        intro.key(BOOT_TICS, '\0'),
        Some(IntroAction::Skip {
            chosen: 1,
            notice: Some("DOS boots start type 0; starting the verified type 1 (Ross 154)")
        })
    );
    // V is a no-op before the DOS gate and latches the version after it.
    assert_eq!(intro.key(BOOT_TICS, 'V'), None);
    assert_eq!(intro.key(INTRO_INPUT_FROM, 'V'), Some(IntroAction::Version));
    assert_eq!(
        intro.key(BOOT_TICS, '0'),
        Some(IntroAction::Skip {
            chosen: 1,
            notice: Some("DOS boots start type 0; starting the verified type 1 (Ross 154)")
        })
    );
}

#[test]
fn the_model_bitmap_and_animation_follow_the_original_thresholds() {
    let start = IntroTimeline::at(BOOT_TICS);
    assert_eq!((start.model, start.animation), (MODEL_FIRST, 1));
    assert!(!start.bitmap && !start.finished && start.lines.is_empty());

    // 10.0 s: 497,000 tics, just before F962's 0x795cc bitmap. The credits do
    // not start until the model switch.
    let ten = IntroTimeline::at(497_000);
    assert_eq!(ten.animation, 485);
    assert!(!ten.bitmap);
    assert!(ten.lines.is_empty());
    let after = IntroTimeline::at(PRIMARY_BITMAP_FROM);
    assert!(
        !after.bitmap,
        "the original skips the bitmap at 0x795cc itself"
    );
    assert!(IntroTimeline::at(PRIMARY_BITMAP_FROM + 1).bitmap);

    // The model switch: 177 to 176, and the animation restarts from the switch.
    let before = IntroTimeline::at(MODEL_SWITCH - 1);
    assert_eq!(before.model, MODEL_FIRST);
    assert_eq!(before.animation, ((MODEL_SWITCH - 1) >> 10) as u16);
    let switch = IntroTimeline::at(MODEL_SWITCH);
    assert_eq!((switch.model, switch.animation), (MODEL_SECOND, 0));
    assert_eq!(
        IntroTimeline::at(MODEL_SWITCH + 0x7ff).animation,
        1,
        "0x7ff tics after the switch is still animation 1"
    );
}

#[test]
fn the_boot_clock_starts_at_0x400_and_loops_at_the_original_point() {
    assert_eq!(BOOT_TICS, 0x400);
    assert_eq!(TICS_PER_FRAME, 0x3e2);
    // One 20 ms frame at 1x adds 994 tics (49,700/s).
    assert_eq!(IntroTimeline::advance(BOOT_TICS, 20), BOOT_TICS + 994);
    assert_eq!(IntroTimeline::advance(BOOT_TICS, 0), BOOT_TICS);
    assert_eq!(
        IntroTimeline::advance(INTRO_END - 1, 20),
        0,
        "the original restarts at 0 after the clear/flip/clear"
    );
    assert_eq!(IntroTimeline::advance(INTRO_END, 20), 0);
}

#[test]
fn every_credit_line_covers_exactly_its_threshold_window() {
    assert_eq!(INTRO_LINES.len(), 18);
    let thresholds: Vec<(u32, Option<u32>)> = INTRO_LINES
        .iter()
        .map(|line| (line.first, line.ends))
        .collect();
    assert_eq!(
        thresholds,
        vec![
            (MODEL_SWITCH, Some(0xfbd3a)),
            (0x181534, Some(0x1ee8d2)),
            (0x212f5c, Some(0x24fa42)),
            (0x2375e6, Some(0x2740cc)),
            (0x2d523c, Some(0x34e808)),
            (0x32a17e, Some(0x372e92)),
            (0x3bbba6, Some(0x3f868c)),
            (0x48a0b4, Some(0x4d2dc8)),
            (0x4ae73e, Some(0x4f7452)),
            (0x4d2dc8, Some(0x50f8ae)),
            (0x588e7a, Some(0x5d1b8e)),
            (0x5ad504, Some(0x5e9fea)),
            (0x626ad0, Some(0x67ba12)),
            (0x657388, Some(0x693e6e)),
            (0x6b84f8, Some(0x70d43a)),
            (0x6d0955, Some(0x725896)),
            (0x77a7d8, None),
            (0x77a7d8, None),
        ]
    );
    let positions: Vec<(i32, i32, i32)> = INTRO_LINES
        .iter()
        .map(|line| (line.x, line.y, line.wrap))
        .collect();
    assert_eq!(
        positions,
        vec![
            (90, 15, 0),
            (2, 68, 0),
            (245, 20, 0),
            (10, 115, 0),
            (240, 15, 0),
            (5, 125, 0),
            (245, 15, 0),
            (20, 20, 0),
            (240, 115, 0),
            (20, 20, 0),
            (5, 125, 0),
            (245, 10, 0),
            (20, 100, 0),
            (140, 10, 0),
            (5, 20, 0),
            (220, 20, 0),
            (5, 10, 310),
            (5, 68, 310),
        ]
    );
    let strings: Vec<usize> = INTRO_LINES.iter().map(|line| line.string).collect();
    assert_eq!(
        strings,
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 12, 14, 15, 16, 17]
    );
    for (index, line) in INTRO_LINES.iter().enumerate() {
        assert!(
            line.visible(line.first),
            "line {index} not at its first tic"
        );
        if line.first > 0 {
            assert!(
                !line.visible(line.first - 1),
                "line {index} visible before its first tic"
            );
        }
        if let Some(ends) = line.ends {
            assert!(
                line.visible(ends - 1),
                "line {index} missing before its end"
            );
            assert!(!line.visible(ends), "line {index} visible at its end");
        }
        assert!(
            IntroTimeline::at(line.first)
                .lines
                .iter()
                .any(|shown| std::ptr::eq(*shown, line)),
            "line {index} missing at {}",
            line.first
        );
    }
}

#[test]
fn the_intro_ends_by_clearing_and_restarting_the_clock() {
    let last = IntroTimeline::at(INTRO_END - 1);
    assert!(!last.finished);
    assert_eq!(last.model, MODEL_SECOND);
    assert!(last.bitmap);
    assert_eq!(
        last.lines,
        vec![&INTRO_LINES[16], &INTRO_LINES[17]],
        "the last two credits share the final stretch"
    );
    assert_eq!(
        (last.lines[0].x, last.lines[0].y, last.lines[0].wrap),
        (5, 10, 310)
    );
    assert_eq!(
        (last.lines[1].x, last.lines[1].y, last.lines[1].wrap),
        (5, 68, 310)
    );
    let end = IntroTimeline::at(INTRO_END);
    assert!(end.finished && end.lines.is_empty() && end.model == 0 && !end.bitmap);
}

#[test]
fn overlapping_credits_both_draw() {
    let both = IntroTimeline::at(0x657388);
    assert_eq!(both.lines.len(), 2, "{:?}", both.lines);
    assert_eq!(both.lines[0].x, 20);
    assert_eq!(both.lines[1].x, 140);
    assert_eq!(both.lines[0].code, 0x9428, "Journal Entries");
    assert_eq!(both.lines[1].code, 0x9427, "Representation");
    let both = IntroTimeline::at(0x6d0955);
    assert_eq!(both.lines.len(), 2, "{:?}", both.lines);
    assert_eq!(both.lines[0].code, 0x941b + 14);
    assert_eq!(both.lines[1].code, 0x941b + 15);
}

#[test]
fn a_tampered_threshold_changes_the_window() {
    let mut line: IntroLine = INTRO_LINES[0];
    assert!(line.visible(MODEL_SWITCH) && !line.visible(0xfbd3a));
    line.ends = Some(0xfbd3b);
    assert!(line.visible(0xfbd3a), "the tamper must move the window");
}
