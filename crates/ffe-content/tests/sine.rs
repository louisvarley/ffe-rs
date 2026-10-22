use ffe_content::{Asset, AssetKind, Manifest, SourceFile};

#[test]
fn sine_table_requires_v6_and_validated_quarter_wave_payload() {
    let manifest = |version| Manifest {
        format: "ffe-content-cache".to_owned(),
        format_version: version,
        tool_version: "test".to_owned(),
        source_profile: "synthetic".to_owned(),
        source_files: vec![SourceFile {
            path: "FIRSTENC.EXE".to_owned(),
            size: 1,
            sha256: "a".repeat(64),
        }],
        complete_game_assets: false,
        unresolved: vec!["synthetic".to_owned()],
        files: vec![],
    };
    let asset = |bytes| Asset {
        id: "base:math/sine".to_owned(),
        path: "math/sine.i32".to_owned(),
        kind: AssetKind::SineTable { source_offset: 100 },
        bytes,
    };
    let good: Vec<_> = (0..16384i32)
        .flat_map(|i| (i * 65536).to_le_bytes())
        .collect();
    let bytes = ffe_content::build(manifest(6), vec![asset(good.clone())]).unwrap();
    assert_eq!(
        ffe_content::read(&bytes).unwrap().asset("base:math/sine"),
        Some(good.as_slice())
    );
    for version in [1, 2, 3, 4, 5, 10] {
        assert!(ffe_content::build(manifest(version), vec![asset(good.clone())]).is_err());
    }
    assert!(ffe_content::build(manifest(7), vec![asset(good.clone())]).is_ok());
    for bad in [vec![0; 65535], vec![0xff; 65536]] {
        assert!(ffe_content::build(manifest(6), vec![asset(bad)]).is_err());
    }
}
