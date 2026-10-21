use ffe_core::model_commands::{visibility, visibility_branch, VisibilityContext};
use ffe_core::model_normals::Normal;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    case: usize,
    operand: i16,
    depth: i32,
    scale: i32,
    detail: [i32; 2],
    predicate: u8,
    branches: [Option<i32>; 6],
}

#[test]
#[ignore = "Local-only executed JJFFE visibility branches; use tools/reference/verify-visibility.py"]
fn executed_reference_visibility_predicates_and_branch_outcomes() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_VISIBILITY_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 512 * 1024);
    let normals: Vec<_> = (0..6)
        .map(|i| Normal {
            culled: i % 2 != 0,
            colour: [0; 3],
        })
        .collect();
    let depths = [i32::MIN, -1, 0, 1, 100, i32::MAX];
    let scales = [-33, -1, 0, 1, 15, 31, 32, 33, i32::MAX, i32::MIN];
    let details = [[0, 0], [2, 999], [1, -2], [3, 31]];
    let codes = [11, 12, 0x8b, 0x8c, 0xff8b, 0xff8c];
    let mut count = 0;
    for (i, line) in input.lines().enumerate() {
        let v: Vector = serde_json::from_str(line).unwrap();
        assert_eq!(v.case, i);
        assert!(i < 966);
        if i < 960 {
            assert_eq!(v.operand, [0, 1, 31, 32767][i / 240]);
            assert_eq!(v.depth, depths[i / 40 % 6]);
            assert_eq!(v.scale, scales[i / 4 % 10]);
            assert_eq!(v.detail, details[i % 4]);
        } else {
            assert_eq!(v.operand, i16::MIN + (i - 960) as i16);
            assert_eq!(
                (v.depth, v.scale, v.detail),
                (i32::MAX, i32::MIN, details[3])
            );
        }
        let context = VisibilityContext {
            depth: v.depth,
            scale: v.scale,
            detail: v.detail,
            normals: &normals,
        };
        assert_eq!(
            u8::from(visibility(v.operand, &context).unwrap()),
            v.predicate,
            "predicate case {i}"
        );
        for (j, code) in codes.into_iter().enumerate() {
            assert_eq!(
                visibility_branch(code, v.operand, &context).unwrap(),
                v.branches[j],
                "branch case {i} code {code:#06x}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 966);
}
