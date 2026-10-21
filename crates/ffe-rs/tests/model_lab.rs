use ffe_content::{Asset, AssetKind, Manifest, SourceFile};
use ffe_rs::model_lab::{LabView, ModelLab, StationVertices};

#[test]
fn surface_lab_cycles_verified_topology_wraps_selection_and_keeps_old_vertex_mode() {
    let [a, b] = models();
    let commands = [7, 0, 0, 0, 0, 2, 4, 3, 0, 0];
    let a = a.with_commands(&commands).unwrap();
    let b = b.with_commands(&commands).unwrap();
    let mut lab = ModelLab::from_models([a, b]);
    assert_eq!(lab.view(), LabView::Wire);
    let before = lab.points().unwrap();
    lab.cycle_view();
    assert_eq!(lab.view(), LabView::Face);
    assert_eq!(lab.selected_face().unwrap().0, 0);
    assert_eq!(lab.selected_face().unwrap().1.vertices, [3, 1, 2]);
    lab.step_face(-1).unwrap();
    assert_eq!(lab.selected_face().unwrap().0, 1);
    lab.step_face(1).unwrap();
    assert_eq!(lab.selected_face().unwrap().0, 0);
    assert!(lab.step_face(i32::MAX).is_err());
    assert_eq!(lab.points().unwrap(), before);
    lab.cycle_view();
    assert_eq!(lab.view(), LabView::Points);
    lab.cycle_view();
    assert_eq!(lab.view(), LabView::Wire);
    lab.select(1).unwrap();
    assert_eq!(lab.selected_face().unwrap().0, 0);
    let mut old = ModelLab::from_models(models());
    old.cycle_view();
    old.step_face(1).unwrap();
    assert_eq!(old.view(), LabView::Points);
    assert!(old.selected_face().is_none());
}

fn models() -> [StationVertices; 2] {
    let records = [
        1, 0, 50, 10, 20, 0, 1, 0, 20, 30, 50, 0, 19, 0, 0x81, 0, 2, 0,
    ];
    [
        StationVertices::new(74, 6, 13, &records).unwrap(),
        StationVertices::new(75, 6, 14, &records).unwrap(),
    ]
}

#[test]
fn model_lab_is_repeatable_rotates_and_animates_without_mutating_inputs() {
    let mut lab = ModelLab::from_models(models());
    let before = lab.points().unwrap();
    assert_eq!(before.len(), 6);
    assert_eq!(before, lab.points().unwrap());
    lab.turn(1, -1).unwrap();
    assert_ne!(before, lab.points().unwrap());
    lab.turn(-1, 1).unwrap();
    assert_eq!(before, lab.points().unwrap());
    lab.set_animation(65535);
    assert_ne!(before, lab.points().unwrap());
    lab.set_animation(0);
    assert_eq!(before, lab.points().unwrap());
    for _ in 0..72 {
        lab.turn(1, 1).unwrap();
    }
    assert_eq!(before, lab.points().unwrap());
    let old = lab.points().unwrap();
    assert!(lab.select(2).is_err());
    assert!(lab.turn(i32::MAX, 0).is_err());
    assert_eq!(old, lab.points().unwrap());
    lab.select(1).unwrap();
    assert_eq!(lab.model().index, 75);
    assert!(lab
        .points()
        .unwrap()
        .iter()
        .all(|(x, y, _)| (0..320).contains(x) && (32..125).contains(y)));
    assert!(StationVertices::new(0, 2, 0, &[1; 6]).is_err());
}

#[test]
fn older_caches_keep_game_content_readable_but_lab_reports_required_upgrade() {
    let manifest = Manifest {
        format: "ffe-content-cache".to_owned(),
        format_version: 2,
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
    let bytes = ffe_content::build(
        manifest,
        vec![Asset {
            id: "base:palette/low".to_owned(),
            path: "palettes/low.rgb6".to_owned(),
            kind: AssetKind::LowPaletteRgb6,
            bytes: vec![0; 384],
        }],
    )
    .unwrap();
    let cache = ffe_content::read(&bytes).unwrap();
    assert!(ModelLab::new(&cache).err().unwrap().contains("cache v3"));
    assert_eq!(cache.asset("base:palette/low"), Some([0; 384].as_slice()));
}

#[test]
fn diagnostic_projection_bounds_wrapped_extreme_vertex_programs() {
    let records = [
        1, 0, 127, 127, 127, 0, 1, 0, 128, 128, 128, 0, 19, 0, 0x81, 0, 2, 0,
    ];
    let mut lab = ModelLab::from_models([
        StationVertices::new(74, 6, 31, &records).unwrap(),
        StationVertices::new(75, 6, 31, &records).unwrap(),
    ]);
    for animation in [0, 1, 16384, 32768, 65535] {
        lab.set_animation(animation);
        for _ in 0..72 {
            lab.turn(1, 1).unwrap();
            assert!(lab
                .points()
                .unwrap()
                .iter()
                .all(|(x, y, _)| (0..320).contains(x) && (32..125).contains(y)));
        }
    }
}

#[test]
fn cache_lab_preserves_v3_points_and_requires_matching_v4_command_descriptors() {
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
    let assets = || {
        [74, 75]
            .into_iter()
            .flat_map(|index| {
                [
                    Asset {
                        id: format!("base:model-vertices/{index:03}"),
                        path: format!("models/{index:03}.vtx"),
                        kind: AssetKind::ModelVertices {
                            index,
                            vertex_count: 2,
                            scale: 13,
                            descriptor_offset: 1000 + u64::from(index) * 4,
                            source_offset: 2000,
                        },
                        bytes: vec![1, 0, 1, 2, 3, 0],
                    },
                    Asset {
                        id: format!("base:model-commands/{index:03}"),
                        path: format!("models/{index:03}.cmd"),
                        kind: AssetKind::ModelCommands {
                            index,
                            vertex_count: 2,
                            descriptor_offset: 1000 + u64::from(index) * 4,
                            source_offset: 3000,
                        },
                        bytes: vec![3, 0, 0, 0, 0, 1, 0, 0, 0, 0],
                    },
                ]
            })
            .collect::<Vec<_>>()
    };
    let load = |version, assets| {
        let bytes = ffe_content::build(manifest(version), assets).unwrap();
        ModelLab::new(&ffe_content::read(&bytes).unwrap())
    };
    assert_eq!(load(4, assets()).unwrap().view(), LabView::Wire);
    let normal_assets = || {
        let mut all = assets();
        for index in [74, 75] {
            all.push(Asset {
                id: format!("base:model-normals/{index:03}"),
                path: format!("models/{index:03}.nrm"),
                kind: AssetKind::ModelNormals {
                    index,
                    vertex_count: 2,
                    normal_count: 4,
                    descriptor_offset: 1000 + u64::from(index) * 4,
                    source_offset: 4000,
                },
                bytes: vec![0, 0, 127, 0, 0, 0],
            });
        }
        all
    };
    let normal_lab = load(5, normal_assets()).unwrap();
    assert!(normal_lab.normals().unwrap().is_some());
    assert!(load(5, assets()).err().unwrap().contains("missing"));
    for mismatch in 0..3 {
        let mut bad = normal_assets();
        if let AssetKind::ModelNormals {
            index,
            vertex_count,
            descriptor_offset,
            ..
        } = &mut bad[4].kind
        {
            match mismatch {
                0 => *index = 75,
                1 => *vertex_count = 4,
                _ => *descriptor_offset += 4,
            }
        }
        assert!(load(5, bad).err().unwrap().contains("mismatch"));
    }
    let mut old = assets();
    old.retain(|a| matches!(a.kind, AssetKind::ModelVertices { .. }));
    assert_eq!(load(3, old).unwrap().view(), LabView::Points);
    let mut missing = assets();
    missing.retain(|a| a.id != "base:model-commands/075");
    assert!(load(4, missing).err().unwrap().contains("missing"));
    for mismatch in 0..3 {
        let mut bad = assets();
        if let AssetKind::ModelCommands {
            index,
            vertex_count,
            descriptor_offset,
            ..
        } = &mut bad[1].kind
        {
            match mismatch {
                0 => *index = 75,
                1 => *vertex_count = 4,
                _ => *descriptor_offset += 4,
            }
        }
        assert!(load(4, bad).err().unwrap().contains("mismatch"));
    }
}

#[test]
fn normal_filter_has_exact_bounded_results_and_camera_changes_do_not_change_points() {
    let [a, b] = models();
    let commands = [3, 0, 0, 0, 0, 2, 2, 3, 0, 0];
    let normals = [0, 0, 127, 0, 0, 0];
    let a = a
        .with_commands(&commands)
        .unwrap()
        .with_normals(&normals, 4)
        .unwrap();
    let b = b
        .with_normals(&normals, 4)
        .unwrap()
        .with_commands(&commands)
        .unwrap();
    let mut lab = ModelLab::from_models([a, b]);
    let points = lab.points().unwrap();
    let before = lab.normals().unwrap().unwrap();
    lab.cycle_view();
    lab.cycle_view();
    lab.cycle_view();
    assert_eq!(lab.view(), LabView::Normals);
    assert_eq!(lab.points().unwrap(), points);
    assert_eq!(lab.normals().unwrap().unwrap(), before);
    for _ in 0..36 {
        lab.turn(1, 0).unwrap();
    }
    assert_ne!(lab.normals().unwrap().unwrap(), before);
    lab.cycle_view();
    assert_eq!(lab.view(), LabView::Wire);
    let [bad, _] = models();
    assert!(bad
        .with_commands(&[3, 0, 0, 0, 0, 2, 8, 3, 0, 0])
        .unwrap()
        .with_normals(&normals, 4)
        .is_err());
}
