//! Local-only census of the animation inputs each cached model reads: which
//! global words change its executed output, and which top-level locals it reads
//! before writing them (stale stack data in the original, see
//! `docs/notes/animation.md`). Indicative only: a perturbation shows an input
//! matters on the branches the tried settings reach.
//!
//! `FFE_CENSUS_CACHE=<data.bin> cargo test --release -p ffe-rs --test
//! animation_census -- --ignored --nocapture`
use ffe_content::AssetKind;
use ffe_core::model::VertexContext;
use ffe_core::model_animation::top_level_locals;
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::{execute_live, InstanceContext};
use ffe_core::model_instance::Vertices;
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::Normal;
use std::collections::BTreeMap;

struct Model {
    index: u32,
    vertex_count: u32,
    scale: u32,
    vertices: Vec<u8>,
    commands: Vec<u8>,
    normals: Vec<u8>,
}

fn run(
    model: &Model,
    table: &SineTable,
    globals: [u16; 64],
    strict: bool,
) -> Result<String, String> {
    let normals = vec![
        Normal {
            culled: false,
            colour: [0; 3],
        };
        32768
    ];
    let visibility = VisibilityContext {
        depth: 2000,
        scale: 0,
        detail: [0, 0],
        normals: &normals,
    };
    let mut live =
        Vertices::new(&model.vertices, model.vertex_count)?.with_normal_records(&model.normals);
    let (locals, undefined) = top_level_locals(&[[1; 3]; 8]);
    let mut context = VertexContext {
        scale: model.scale,
        globals,
        locals,
        undefined_locals: if strict { undefined } else { 0 },
        colour: [1; 3],
        ..Default::default()
    };
    let instance = InstanceContext {
        position: [0, 0, 2000],
        ..Default::default()
    };
    let result = execute_live(
        &model.commands,
        &mut live,
        &mut context,
        &visibility,
        Some(table),
        instance,
        4096,
    )?;
    // The emissions carry the whole context: drop the globals themselves.
    let text = format!("{:?}", (&result.commands, &result.ordered));
    let mut kept = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(start) = rest.find("globals: [") {
        kept.push_str(&rest[..start]);
        let end = rest[start..].find(']').expect("closing bracket");
        rest = &rest[start + end + 1..];
    }
    kept.push_str(rest);
    Ok(kept)
}

#[test]
#[ignore = "Local-only census over the cached models"]
fn animation_input_census() {
    let path = std::env::var_os("FFE_CENSUS_CACHE").expect("FFE_CENSUS_CACHE");
    let cache = ffe_content::read(&std::fs::read(path).unwrap()).unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let mut models = Vec::new();
    for record in &cache.manifest().files {
        let AssetKind::ModelCommands {
            index,
            vertex_count,
            ..
        } = record.asset
        else {
            continue;
        };
        let find = |kind: &str| {
            let id = format!("base:model-{kind}/{index:03}");
            let record = cache.manifest().files.iter().find(|r| r.id == id)?;
            Some((record.asset.clone(), cache.asset(&id)?.to_vec()))
        };
        let (Some((AssetKind::ModelVertices { scale, .. }, vertices)), Some((_, normals))) =
            (find("vertices"), find("normals"))
        else {
            continue;
        };
        models.push(Model {
            index,
            vertex_count,
            scale,
            vertices,
            commands: cache.asset(&record.id).unwrap().to_vec(),
            normals,
        });
    }
    // Settings: all words at a value, and global 0 swept over a cycle.
    let mut settings: Vec<[u16; 64]> = [0u16, 0x8000, 0xffff, 0x4000, 0x1234]
        .iter()
        .map(|v| {
            let mut g = [*v; 64];
            g[4] = 0; // opcode 30 light points: no point tables imported
            g
        })
        .collect();
    for step in 0..16u16 {
        let mut g = [0u16; 64];
        g[0] = step.wrapping_mul(0x1000).wrapping_add(0x123);
        settings.push(g);
    }
    let mut reads: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    let mut stale: BTreeMap<u32, String> = BTreeMap::new();
    let mut executed = 0;
    for model in &models {
        let mut affected = [false; 64];
        let mut ran = false;
        for globals in &settings {
            let Ok(base) = run(model, &table, *globals, false) else {
                continue;
            };
            ran = true;
            if let Err(error) = run(model, &table, *globals, true) {
                stale.entry(model.index).or_insert(error);
            }
            for (index, flag) in affected.iter_mut().enumerate() {
                if *flag || index == 4 {
                    continue;
                }
                for value in [0u16, 0xffff, 0x8000, 0x2a5b] {
                    let mut changed = *globals;
                    changed[index] = value;
                    if let Ok(other) = run(model, &table, changed, false) {
                        if other != base {
                            *flag = true;
                            break;
                        }
                    }
                }
            }
        }
        if ran {
            executed += 1;
        }
        for (index, flag) in affected.iter().enumerate() {
            if *flag {
                reads.entry(index).or_default().push(model.index);
            }
        }
    }
    println!("models executed: {executed} of {}", models.len());
    for (index, list) in &reads {
        println!("global {index}: {} models: {list:?}", list.len());
    }
    // Which models call which as children (decoded opcode 14/27 records).
    let mut parents: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for model in &models {
        if let Ok(program) = ffe_core::model_commands::decode(&model.commands, model.vertex_count) {
            for call in &program.submodels {
                let list = parents.entry(u32::from(call.model)).or_default();
                if !list.contains(&model.index) {
                    list.push(model.index);
                }
            }
        }
    }
    println!(
        "models reading a stale local (1..11) before writing it: {}",
        stale.len()
    );
    let mut top_level_only = Vec::new();
    for (index, error) in &stale {
        let callers = parents.get(index).cloned().unwrap_or_default();
        if callers.is_empty() {
            top_level_only.push(*index);
        }
        println!("  model {index}: {error} called by {callers:?}");
    }
    println!("of which never called as a child: {top_level_only:?}");
}
