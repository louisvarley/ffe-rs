//! Local-only comparison of the intro timeline with the executed original:
//! F961/F962 at the credit thresholds (`--intro`). Run through
//! `tools/reference/verify-intro.py`.
//!
//! The DOS release and the JJFFE data agree on the credit strings, their
//! `GameTics` windows, positions and wrap widths, and on the two logo models
//! (177 then 176); this test checks those. The JJFFE-only version string and
//! legal splash are excluded from the boot (see `docs/notes/intro.md`).
use ffe_rs::front_end::{
    IntroTimeline, INTRO_LINES, MODEL_SWITCH, PRIMARY_BITMAP_FROM, VERSION_LINE,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    tics: u32,
    model: u32,
    animation: u32,
    version: u8,
    texts: Vec<Text>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Text {
    code: u32,
    colour: i32,
    x: i32,
    y: i32,
    wrap: i32,
    /// The JJFFE trace's plain text; the port's DOS bytes come from the cache,
    /// so the oracle compares geometry and codes only.
    #[serde(rename = "text")]
    _text: String,
}

#[test]
#[ignore = "Local-only original intro; use tools/reference/verify-intro.py"]
fn executed_reference_intro() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_INTRO_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4 << 20, "trace too large");
    let mut cases = 0;
    let mut covered = Vec::new();
    let mut version_cases = 0;
    for line in input.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        let frame = IntroTimeline::at(case.tics);
        assert_eq!(frame.model, case.model, "model at {}", case.tics);
        assert_eq!(
            u32::from(frame.animation),
            case.animation,
            "animation at {}",
            case.tics
        );
        let version = case.version != 0;
        // JJFFE latches V at the model switch; the DOS gate is 0x719668. The
        // oracle's one version row is at 0x8276e, so it is JJFFE-only and its
        // text is not compared against the DOS screen.
        let jjffe_only_version = version && case.tics < ffe_rs::front_end::INTRO_INPUT_FROM;
        if !jjffe_only_version {
            assert_eq!(
                IntroTimeline::version(case.tics, version.then_some('V')),
                version,
                "version flag at {}",
                case.tics
            );
        }
        if jjffe_only_version {
            version_cases += 1;
            covered.push(case.tics);
            cases += 1;
            continue;
        }
        let mut expected: Vec<(u32, i32, i32, i32, i32)> = Vec::new();
        // F962 draws the 'V' line before the credits.
        if version {
            expected.push((
                VERSION_LINE.code,
                VERSION_LINE.colour,
                VERSION_LINE.x,
                VERSION_LINE.y,
                VERSION_LINE.wrap,
            ));
            version_cases += 1;
        }
        expected.extend(
            frame
                .lines
                .iter()
                .map(|line| (line.code, line.colour, line.x, line.y, line.wrap)),
        );
        let actual: Vec<(u32, i32, i32, i32, i32)> = case
            .texts
            .iter()
            .map(|text| (text.code, text.colour, text.x, text.y, text.wrap))
            .collect();
        assert_eq!(actual, expected, "texts at {}", case.tics);
        covered.push(case.tics);
        cases += 1;
    }
    assert_eq!(cases, 76, "case count");
    // Every credit's first tic, its last tic and its end are in the trace.
    for line in INTRO_LINES.iter() {
        assert!(
            covered.contains(&line.first),
            "no case at the first tic {} of {:#x}",
            line.first,
            line.code
        );
        if let Some(ends) = line.ends {
            assert!(
                covered.contains(&(ends - 1)) && covered.contains(&ends),
                "no case at the end of {:#x}",
                line.code
            );
        }
    }
    assert!(covered.contains(&0), "no case at 0");
    assert!(covered.contains(&497_000), "no case at 10.0 s");
    assert!(
        covered.contains(&PRIMARY_BITMAP_FROM) && covered.contains(&(PRIMARY_BITMAP_FROM + 1)),
        "no bitmap boundary case"
    );
    assert!(
        covered.contains(&(MODEL_SWITCH - 1)) && covered.contains(&MODEL_SWITCH),
        "no model switch case"
    );
    assert_eq!(version_cases, 1, "version case");
    eprintln!(
        "compared {} intro cases: {} credits, {} version case, 10.0 s and the end",
        cases,
        INTRO_LINES.len(),
        version_cases
    );
}
