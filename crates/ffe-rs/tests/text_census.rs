//! Local-only census of opcode 10 text and opcode 18 labels in every cached
//! model. This identifies the F1344 codes and F1539 glyph sets the text port
//! must support before adding cache assets or reference hooks.
//!
//! `FFE_TEXT_CACHE=data.bin cargo test -p ffe-rs --test text_census -- --ignored --nocapture`
use ffe_content::AssetKind;
use std::collections::{BTreeMap, BTreeSet};

#[test]
#[ignore = "Local-only census over the cached models"]
fn model_text_and_label_census() {
    let path = std::env::var_os("FFE_TEXT_CACHE").expect("FFE_TEXT_CACHE");
    let cache = ffe_content::read(&std::fs::read(path).unwrap()).unwrap();
    let mut text_codes: BTreeMap<u16, BTreeSet<u32>> = BTreeMap::new();
    let mut label_codes: BTreeMap<u16, BTreeSet<u32>> = BTreeMap::new();
    let mut glyph_sets: BTreeMap<u8, BTreeSet<u32>> = BTreeMap::new();
    let mut texts = 0;
    let mut labels = 0;
    for record in &cache.manifest().files {
        let AssetKind::ModelCommands {
            index,
            vertex_count,
            ..
        } = record.asset
        else {
            continue;
        };
        let program =
            ffe_core::model_commands::decode(cache.asset(&record.id).unwrap(), vertex_count)
                .unwrap_or_else(|error| panic!("model {index}: {error}"));
        for text in program.texts {
            println!(
                "text model={index} offset={:#x} code={:#06x} glyphs={} size={} vertex={} frame={:#06x}",
                text.offset, text.string, text.glyphs, text.size, text.vertex, text.frame
            );
            texts += 1;
            text_codes.entry(text.string).or_default().insert(index);
            glyph_sets.entry(text.glyphs).or_default().insert(index);
        }
        for label in program.labels {
            println!(
                "label model={index} offset={:#x} code={:#06x} vertex={}",
                label.offset, label.string, label.vertex
            );
            labels += 1;
            label_codes.entry(label.string).or_default().insert(index);
        }
    }
    println!("texts={texts} codes={text_codes:?}");
    println!("glyph_sets={glyph_sets:?}");
    println!("labels={labels} codes={label_codes:?}");

    // The label path is complete: every label string must be a model string
    // (`0x4000 | index`) present in the imported table.
    let table = cache
        .asset("base:text/model-names")
        .expect("cache v11 or later");
    let strings: Vec<&[u8]> = table.split(|byte| *byte == 0).collect();
    let count = strings.len() - 1;
    println!("model strings: {count}");
    for (code, models) in &label_codes {
        assert!(
            (0x4000..0x8000).contains(code),
            "label {code:#06x} is not a model string: {models:?}"
        );
        assert!(
            usize::from(code & 0x3fff) < count,
            "label {code:#06x} is outside the {count} imported strings"
        );
    }
    // Of the text codes, the ones the label resolver also handles.
    let resolvable = text_codes
        .keys()
        .filter(|code| (0x4000..0x8000).contains(*code))
        .filter(|code| usize::from(*code & 0x3fff) < count)
        .count();
    let outside: Vec<u16> = text_codes
        .keys()
        .filter(|code| (0x4000..0x8000).contains(*code))
        .filter(|code| usize::from(*code & 0x3fff) >= count)
        .copied()
        .collect();
    let families: BTreeMap<u16, Vec<u32>> = text_codes
        .iter()
        .filter(|(code, _)| !(0x4000..0x8000).contains(*code))
        .map(|(code, models)| (*code, models.iter().copied().collect()))
        .collect();
    println!("text codes needing a handler: {families:x?}");
    println!(
        "text codes resolvable as model strings: {resolvable} of {}; outside the table: {outside:x?}",
        text_codes.len()
    );

    // End to end: the viewer draws the live labels of the two labelled models.
    let viewer = ffe_rs::model_viewer::ModelViewer::new(&cache).unwrap();
    let mut drawn = 0;
    for index in [66, 453] {
        if viewer.find(index).is_none() {
            continue;
        }
        let mut viewer = ffe_rs::model_viewer::ModelViewer::new(&cache).unwrap();
        while viewer.model().index != index {
            viewer.step(1);
        }
        let frame = viewer.render().unwrap();
        for shape in &frame.shapes {
            if let ffe_rs::model_viewer::Shape::Label(position, text) = shape {
                drawn += 1;
                println!(
                    "label drawn model {index} at {position:?}: {:?}",
                    String::from_utf8_lossy(text)
                );
            }
        }
        println!(
            "model {index}: {} shapes, {} text not drawn",
            frame.shapes.len(),
            frame.texts
        );
    }
    assert!(
        drawn >= 6,
        "expected the six station/ship labels, drew {drawn}"
    );
}
