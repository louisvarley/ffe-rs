//! Local-only comparison of `ffe_core::lift` and the stage machine with the executed original:
//! the lift scene stage functions F1069-F1087 and F1086 (`--lift`). Run through
//! `tools/reference/verify-lift.py`.
use ffe_core::lift::{objects, view_position, Object};
use ffe_core::model_animation::build_yzt;
use ffe_core::model_math::SineTable;
use ffe_core::traffic::{state, Stages};
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value
        .as_i64()
        .unwrap_or_else(|| panic!("not an integer: {value}"))
}

#[test]
#[ignore = "Local-only lift comparison; use tools/reference/verify-lift.py"]
fn executed_reference_lift() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_LIFT_CACHE").expect("FFE_LIFT_CACHE")).unwrap(),
    )
    .unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let eye = |ship: usize| {
        let word =
            |at: usize| i16::from_le_bytes([stats[48 * ship + at], stats[48 * ship + at + 1]]);
        [word(0x18), word(0x1a)]
    };
    let path = std::env::var_os("FFE_LIFT_VECTORS").expect("FFE_LIFT_VECTORS is not set");
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.len() < 32 << 20, "trace too large");
    let mut per_stage = [0usize; 0x24];
    let (mut expired, mut multiple) = (0, 0);
    for line in text.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let stage = int(&case["stage"]) as u8;
        let timeout = int(&case["timeout"]) as i32;
        let frame = int(&case["frame"]) as i32;
        let eye = eye(int(&case["ship"]) as usize);
        // The scene the stage draws: from the stage machine for the timed stages, and straight
        // from F1086's argument for the last launch stage.
        let (mut after_state, mut after_timeout) = (stage, timeout);
        let scene = if stage == state::LAUNCH_OUT {
            objects(stage, timeout)
        } else {
            let mut stages = Stages {
                state: stage,
                timeout,
                drawn: None,
            };
            stages.step(frame, false, 0xffff);
            after_state = stages.state;
            after_timeout = stages.timeout;
            expired += usize::from(stages.state != stage);
            let (drawn, t) = stages.drawn.expect("a timed stage draws");
            assert_eq!(drawn, stage, "{line}");
            objects(drawn, t)
        }
        .expect("a scene");
        assert_eq!(
            (
                int(&case["after_state"]) as u8,
                int(&case["after_timeout"]) as i32
            ),
            (after_state, after_timeout),
            "stage state and timeout after the frame: {line}"
        );
        let want = case["objects"].as_array().unwrap();
        assert_eq!(want.len(), scene.len(), "object count: {line}");
        assert_eq!(int(&case["count"]) as usize, scene.len(), "{line}");
        multiple += usize::from(scene.len() > 1);
        for (got, want) in scene.iter().zip(want) {
            let got: &Object = got;
            assert_eq!(int(&want["model"]) as u32, got.model, "model: {line}");
            assert_eq!(int(&want["word"]) as u16, got.word, "word: {line}");
            let position = view_position(got, eye);
            let high = |k: usize| int(&want["position"][k]);
            let low = |k: usize| int(&want["position"][k]) as u32 as i32;
            assert_eq!(
                (low(0), low(2), low(4)),
                (position[0], position[1], position[2]),
                "position: {line}"
            );
            assert_eq!(
                (high(1), high(3), high(5)),
                (0, 0, 0),
                "high dwords: {line}"
            );
            let matrix = build_yzt(&table, 0, i32::from(got.angle));
            let wanted: Vec<i32> = want["matrix"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| int(v) as i32)
                .collect();
            assert_eq!(wanted, matrix.to_vec(), "orientation: {line}");
            assert_eq!(
                int(&want["time_word"]) as u16,
                (int(&case["tics"]) as u32 >> 10) as u16,
                "global word 0: {line}"
            );
            assert_eq!(int(&want["seed"]), int(&case["seed"]), "seed: {line}");
            let outdoor = if int(&case["flag"]) & 0x20 != 0 { 0 } else { 1 };
            assert_eq!(int(&want["outdoor"]), outdoor, "{line}");
        }
        per_stage[usize::from(stage)] += 1;
    }
    eprintln!("lift cases per stage {per_stage:?}; {expired} expired, {multiple} with two objects");
    for stage in [
        state::OPEN_DOOR,
        state::INTO_LIFT,
        state::TURN,
        state::CLOSE_DOOR,
        state::WAIT_IN_LIFT,
        state::OPEN_INNER,
        state::INTO_BAY,
        state::LAUNCH_INTO_LIFT,
        state::LAUNCH_CLOSE,
        state::LAUNCH_WAIT,
        state::LAUNCH_OPEN,
        state::LAUNCH_OUT,
    ] {
        assert!(per_stage[usize::from(stage)] >= 400, "stage {stage:#x}");
    }
    assert!(expired > 300 && multiple > 300);
}
