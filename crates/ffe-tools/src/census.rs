//! Read-only model census: how far every model in the original table gets through
//! the native importer, decoder and executor. A diagnostic, not a converter.
use crate::external::recorded_bytes;
use crate::{inspect_game, Severity};
use ffe_convert::embedded::{
    decode_model_normals, decode_model_vertices, decode_sine_table, model_command_bytes,
};
use ffe_convert::model_extents::MODEL_COUNT;
use ffe_core::model::VertexContext;
use ffe_core::model_commands::{decode, VisibilityContext};
use ffe_core::model_execute::{execute_live, InstanceContext};
use ffe_core::model_instance::Vertices;
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::Normal;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ModelReport {
    pub index: u32,
    pub extent: usize,
    pub vertex_count: u32,
    /// First failing stage and its message; absent when the program executed.
    pub failure: Option<(String, String)>,
    pub commands: usize,
    pub faces: usize,
    pub ordered: usize,
}

#[derive(Debug, Serialize)]
pub struct Census {
    pub source_profile: String,
    pub models: usize,
    pub executed: usize,
    /// Failures grouped by stage and the blocking command word's opcode where known.
    pub blockers: BTreeMap<String, usize>,
    pub reports: Vec<ModelReport>,
    pub limitations: Vec<&'static str>,
}

/// Opcode of the command word quoted in a decoder/executor error, if any.
fn blocking_opcode(message: &str) -> Option<u16> {
    let start = message.find("0x")?;
    let digits: String = message[start + 2..]
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect();
    (digits.len() == 4).then(|| u16::from_str_radix(&digits, 16).ok().map(|w| w & 31))?
}

fn census_model(bytes: &[u8], table_rva: u32, table: &SineTable, index: u32) -> ModelReport {
    let mut report = ModelReport {
        index,
        extent: 0,
        vertex_count: 0,
        failure: None,
        commands: 0,
        faces: 0,
        ordered: 0,
    };
    let fail = |stage: &str, error: String| Some((stage.to_owned(), error));
    let commands = match model_command_bytes(bytes, table_rva, index) {
        Ok(commands) => commands,
        Err(error) => {
            report.failure = fail("import", error);
            return report;
        }
    };
    report.extent = commands.bytes.len();
    report.vertex_count = commands.vertex_count;
    let vertices = match decode_model_vertices(bytes, table_rva, index) {
        Ok(vertices) => vertices,
        Err(error) => {
            report.failure = fail("vertices", error);
            return report;
        }
    };
    let normal_records = match decode_model_normals(bytes, table_rva, index) {
        Ok(normals) => normals.bytes,
        Err(error) => {
            report.failure = fail("normals", error);
            return report;
        }
    };
    if let Err(error) = decode(&commands.bytes, commands.vertex_count) {
        report.failure = fail("decode", error);
        return report;
    }
    // Same synthetic inputs as the station oracles, moved further back so small
    // models stay in front of the near plane.
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
    let mut live = match Vertices::new(&vertices.bytes, vertices.vertex_count) {
        // Polygon circles read the normal records (+0x154).
        Ok(live) => live.with_normal_records(&normal_records),
        Err(error) => {
            report.failure = fail("vertices", error);
            return report;
        }
    };
    let mut context = VertexContext {
        scale: vertices.scale,
        ..Default::default()
    };
    let instance = InstanceContext {
        position: [0, 0, 2000],
        ..Default::default()
    };
    match execute_live(
        &commands.bytes,
        &mut live,
        &mut context,
        &visibility,
        Some(table),
        instance,
        4096,
    ) {
        Ok(result) => {
            report.commands = result.commands.len();
            report.faces = result.emissions.len();
            report.ordered = result.ordered.len();
        }
        Err(error) => report.failure = fail("execute", error),
    }
    report
}

pub fn model_census(root: &Path) -> Result<Census, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    let inspection = inspect_game(&root)?;
    if !inspection.inspection_passed {
        let errors: Vec<_> = inspection
            .diagnostics
            .iter()
            .filter(|entry| entry.severity == Severity::Error)
            .map(|entry| format!("{} [{}]: {}", entry.path, entry.code, entry.message))
            .collect();
        return Err(format!("source inspection failed:\n{}", errors.join("\n")));
    }
    let profile = inspection
        .source_profile
        .ok_or("inspection passed without a source profile")?;
    let (table_rva, sine_rva) = crate::cache::model_layout(profile.id)?;
    let record = inspection
        .files
        .iter()
        .find(|record| record.path == "FIRSTENC.EXE")
        .ok_or("FIRSTENC.EXE was not fingerprinted")?;
    let bytes = recorded_bytes(&root, record)?;
    let table = SineTable::from_bytes(&decode_sine_table(&bytes, sine_rva)?.bytes)?;
    let mut reports = Vec::with_capacity(MODEL_COUNT);
    let mut blockers = BTreeMap::new();
    for index in 0..MODEL_COUNT as u32 {
        let report = census_model(&bytes, table_rva, &table, index);
        if let Some((stage, message)) = &report.failure {
            let key = match (stage.as_str(), blocking_opcode(message)) {
                ("decode" | "execute", Some(opcode)) => format!("{stage}: opcode {opcode}"),
                // "Vertex pair N: unsupported opcode 0x..": group by the vertex opcode.
                ("vertices", _) if message.contains("opcode") => format!(
                    "vertices: {}",
                    message.rsplit(':').next().unwrap_or(message).trim()
                ),
                _ => format!("{stage}: {}", message.split(':').next().unwrap_or(message)),
            };
            *blockers.entry(key).or_insert(0) += 1;
        }
        reports.push(report);
    }
    Ok(Census {
        source_profile: profile.id.to_owned(),
        models: reports.len(),
        executed: reports.iter().filter(|r| r.failure.is_none()).count(),
        blockers,
        reports,
        limitations: vec![
            "Synthetic instance: identity frame, position z 2000, every normal visible",
            "Glyphs and child models are recorded, not executed",
            "Execution success is not verified against the original for these models",
        ],
    })
}
