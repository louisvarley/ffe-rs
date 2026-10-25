//! Read verified executable asset tables without loading or executing them.

#[derive(Debug)]
pub struct EmbeddedModelVertices {
    pub index: u32,
    pub descriptor_offset: usize,
    pub source_offset: usize,
    pub vertex_count: u32,
    pub scale: u32,
    pub bytes: Vec<u8>,
}

fn model_descriptor(
    bytes: &[u8],
    table_rva: u32,
    index: u32,
) -> Result<(DataSection, usize), String> {
    if index > 511 {
        return Err("Model table index exceeds the bounded importer.".to_owned());
    }
    let section = DataSection::parse(bytes)?;
    let entry_rva = table_rva
        .checked_add(index * 4)
        .ok_or("Model table RVA overflow")?;
    let entry = section.offset(entry_rva, 4)?;
    let descriptor_rva = dword(bytes, entry)?
        .checked_sub(section.image_base)
        .ok_or("Model pointer precedes image base")?;
    let descriptor_offset = section.offset(descriptor_rva, 24)?;
    Ok((section, descriptor_offset))
}

pub fn decode_model_vertices(
    bytes: &[u8],
    table_rva: u32,
    index: u32,
) -> Result<EmbeddedModelVertices, String> {
    let (section, descriptor_offset) = model_descriptor(bytes, table_rva, index)?;
    let vertex_count = dword(bytes, descriptor_offset + 8)?;
    let scale = dword(bytes, descriptor_offset + 20)?;
    if vertex_count == 0 || vertex_count > 128 || vertex_count % 2 != 0 || scale > 31 {
        return Err("Invalid bounded model vertex count/scale.".to_owned());
    }
    let vertex_rva = dword(bytes, descriptor_offset + 4)?
        .checked_sub(section.image_base)
        .ok_or("Vertex pointer precedes image base")?;
    let source_offset = section.offset(vertex_rva, vertex_count as usize / 2 * 6)?;
    let records = range(bytes, source_offset, vertex_count as usize / 2 * 6)?;
    ffe_core::model::validate_records(records, vertex_count)?;
    // Child models reference parent (external) vertices, which only a child instance
    // resolves; their programs validate structurally and evaluate in that context.
    let external = records.chunks_exact(6).any(|r| {
        matches!(r[0], 3 | 4 | 5..=8 | 11 | 12 | 13 | 14 | 15..=20)
            && r[2..5].iter().any(|b| *b >= 0x80)
    });
    if let Err(error) = ffe_core::model::evaluate(
        records,
        vertex_count,
        &ffe_core::model::VertexContext {
            scale,
            ..Default::default()
        },
    ) {
        if !(external && error.contains("External vertex")) {
            return Err(error);
        }
    }
    Ok(EmbeddedModelVertices {
        index,
        descriptor_offset,
        source_offset,
        vertex_count,
        scale,
        bytes: records.to_vec(),
    })
}

/// The descriptor dwords a child-model call (F1750) reads besides the three blocks:
/// +0x14 and +0x18 are both added to the child scale, +0x1c is the F1687 cull radius
/// (shifted left by that scale).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedModelDescriptor {
    pub index: u32,
    pub vertex_count: u32,
    pub normal_count: u32,
    pub scale: u32,
    pub scale_offset: u32,
    pub radius: u32,
}

pub fn decode_model_descriptor(
    bytes: &[u8],
    table_rva: u32,
    index: u32,
) -> Result<EmbeddedModelDescriptor, String> {
    let (section, descriptor_offset) = model_descriptor(bytes, table_rva, index)?;
    // model_descriptor maps 24 bytes; the cull radius is at +0x1c.
    let descriptor_rva = u32::try_from(
        descriptor_offset
            .checked_sub(section.raw)
            .ok_or("Descriptor precedes its section")?,
    )
    .map_err(|_| "Descriptor offset overflow")?
        + section.rva;
    section.offset(descriptor_rva, 32)?;
    Ok(EmbeddedModelDescriptor {
        index,
        vertex_count: dword(bytes, descriptor_offset + 8)?,
        normal_count: dword(bytes, descriptor_offset + 16)?,
        scale: dword(bytes, descriptor_offset + 20)?,
        scale_offset: dword(bytes, descriptor_offset + 24)?,
        radius: dword(bytes, descriptor_offset + 28)?,
    })
}

/// Most characters a font set can define (`0x20..=0x7e`, 95 entries). Several
/// entries alias one program (lowercase letters share the uppercase glyph), and
/// a set's table may end earlier: font 0 defines all 95, font 1 only its first
/// 52. Beyond the defined entries the bytes are not pointers.
pub use ffe_core::model_commands::GLYPH_CHAR_LIMIT;
/// First character the font models place (code 0x20 is space).
pub use ffe_core::model_commands::GLYPH_FIRST_CHAR;

/// The per-character command programs of a font model (model table slots 0, 1
/// and 2). F1747 executes the program for each character through F1772 in a
/// nested frame and then reads its final return word: bits 6.. name the font
/// vertex that is the next pen position. The programs are contiguous slices of
/// the font model's command block; the descriptor's table at +0x40 gives each
/// entry's start (the table at +0x44 holds the same anchors for culled glyphs).
#[derive(Debug)]
pub struct EmbeddedGlyphPrograms {
    pub source_offset: usize,
    pub vertex_count: u32,
    pub scale: u32,
    /// The descriptor's byte `+0x3c`: the font vertex a control code moves the pen to.
    pub newline_vertex: u32,
    /// Start offset of each character's program in `bytes`, in code order from
    /// [`GLYPH_FIRST_CHAR`]; the last program runs to the end of `bytes`.
    pub starts: Vec<u32>,
    pub bytes: Vec<u8>,
}

impl EmbeddedGlyphPrograms {
    /// The program starting at `start`: to the next distinct start, or the end
    /// of the block for the last one.
    pub fn slice(&self, start: usize) -> Option<&[u8]> {
        let end = self
            .starts
            .iter()
            .map(|offset| *offset as usize)
            .filter(|offset| *offset > start)
            .min()
            .unwrap_or(self.bytes.len());
        self.bytes.get(start..end)
    }

    /// The program for character `code`, when the set defines it and it is not a
    /// control code. Lowercase letters may share the uppercase program.
    pub fn program(&self, code: u8) -> Option<&[u8]> {
        let index = usize::from(code.checked_sub(GLYPH_FIRST_CHAR)?);
        let start = *self.starts.get(index)? as usize;
        self.slice(start)
    }
}

/// Import a font model's per-character programs. Each slice must decode as a
/// command program over the font model's vertices and end in a return record
/// whose bits 6.. are an in-range vertex (the next pen); a font set whose
/// layout differs fails explicitly rather than being guessed.
pub fn glyph_programs(
    bytes: &[u8],
    table_rva: u32,
    index: u32,
) -> Result<EmbeddedGlyphPrograms, String> {
    if index > 2 {
        return Err("Only model slots 0, 1 and 2 are font glyph sets.".to_owned());
    }
    let (section, descriptor_offset) = model_descriptor(bytes, table_rva, index)?;
    let vertex_count = dword(bytes, descriptor_offset + 8)?;
    let scale = dword(bytes, descriptor_offset + 20)?;
    if vertex_count == 0 || vertex_count > 128 || vertex_count % 2 != 0 {
        return Err("Invalid font model vertex count.".to_owned());
    }
    let descriptor_rva = u32::try_from(
        descriptor_offset
            .checked_sub(section.raw)
            .ok_or("Descriptor precedes its section")?,
    )
    .map_err(|_| "Descriptor offset overflow")?
        + section.rva;
    let mut absolute = Vec::with_capacity(GLYPH_CHAR_LIMIT);
    for entry in 0..GLYPH_CHAR_LIMIT {
        let pointer = dword(
            bytes,
            section.offset(descriptor_rva + 0x40 + 4 * entry as u32, 4)?,
        )?;
        let Some(rva) = pointer.checked_sub(section.image_base) else {
            break;
        };
        let Ok(offset) = section.offset(rva, 2) else {
            break;
        };
        absolute.push(u32::try_from(offset).map_err(|_| "Glyph offset overflow")?);
    }
    if absolute.is_empty() {
        return Err("Font set defines no glyph programs.".to_owned());
    }
    let first = *absolute.first().ok_or("No glyph programs")?;
    // Several characters share one program (lowercase aliases uppercase), so the
    // distinct starts, in file order, delimit the programs.
    let mut distinct = absolute.clone();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.first() != Some(&first) {
        return Err("The first glyph program is not the block start.".to_owned());
    }
    let mut end = 0;
    for (index, start) in distinct.iter().enumerate() {
        end = match distinct.get(index + 1) {
            Some(next) => *next,
            // The last program is ended by its own return record.
            None => {
                // A glyph program is small; the decoder caps streams at 16 KiB.
                let tail = bytes
                    .get(*start as usize..)
                    .and_then(|rest| rest.get(..rest.len().min(4096)))
                    .ok_or("Glyph program outside file")?;
                let decoded = ffe_core::model_commands::decode(tail, vertex_count)
                    .map_err(|error| format!("Last glyph program: {error}"))?;
                let last = decoded
                    .commands
                    .last()
                    .ok_or("Last glyph program is empty.")?;
                *start + u32::try_from(last.offset + last.size).map_err(|_| "Glyph size")?
            }
        };
        let program = bytes
            .get(*start as usize..end as usize)
            .ok_or("Glyph program slice outside the file")?;
        validate_glyph(program, vertex_count, *start)?;
    }
    let starts = absolute
        .iter()
        .map(|offset| {
            offset
                .checked_sub(first)
                .ok_or("Glyph program precedes the font block")
        })
        .collect::<Result<Vec<u32>, _>>()?;
    let newline_vertex = u32::from(
        *bytes
            .get(section.offset(descriptor_rva + 0x3c, 1)?)
            .ok_or("Font descriptor outside the executable")?,
    );
    if newline_vertex >= vertex_count {
        return Err("The font's newline vertex is out of range.".to_owned());
    }
    Ok(EmbeddedGlyphPrograms {
        source_offset: first as usize,
        vertex_count,
        scale,
        newline_vertex,
        starts,
        bytes: bytes
            .get(first as usize..end as usize)
            .ok_or("Glyph program block outside the executable")?
            .to_vec(),
    })
}

fn validate_glyph(program: &[u8], vertex_count: u32, start: u32) -> Result<(), String> {
    let decoded = ffe_core::model_commands::decode(program, vertex_count)
        .map_err(|error| format!("Glyph program at {start:#x}: {error}"))?;
    // Any opcode-zero word is the return; its bits 6.. are the next pen vertex.
    let last = decoded
        .commands
        .last()
        .ok_or_else(|| format!("Glyph program at {start:#x} is empty."))?;
    if last.word & 31 != 0 {
        return Err(format!(
            "Glyph program at {start:#x} does not end in a return record."
        ));
    }
    if u32::from((last.word >> 6) as u8) >= vertex_count {
        return Err(format!(
            "Glyph program at {start:#x} names an out-of-range pen vertex."
        ));
    }
    Ok(())
}

/// Bytes in one ship statistics block (every block the floppy 1.02 table points to).
pub const SHIP_STATS_BYTES: usize = 48;

/// Bytes of one model record (see [`decode_model_record`]).
pub const MODEL_RECORD_BYTES: usize = 20;

/// The model descriptor fields object set-up (F1533, F879) read, as 20 bytes: the
/// dwords at `+0x14` and `+0x18` (the scales), the byte at `+0x24`, the words at
/// `+0x2c` and `+0x30` (the two radii), a flag byte that is 1 when the pointer at
/// `+0x34` is non-null and the byte it points to is non-zero (a complex collision
/// shape) and a flag byte that is 1 when the statistics pointer at `+0x38` is
/// non-null, and (at 16) the word at `+0x2e`, which with `+0x2c` forms the dword F879
/// reads as the planet's radius.
pub fn decode_model_record(
    bytes: &[u8],
    table_rva: u32,
    index: u32,
) -> Result<[u8; MODEL_RECORD_BYTES], String> {
    let (section, descriptor_offset) = model_descriptor(bytes, table_rva, index)?;
    let record = range(bytes, descriptor_offset, 0x40)?;
    let mut out = [0u8; MODEL_RECORD_BYTES];
    out[0..4].copy_from_slice(&record[0x14..0x18]);
    out[4..8].copy_from_slice(&record[0x18..0x1c]);
    out[8] = record[0x24];
    out[10..12].copy_from_slice(&record[0x2c..0x2e]);
    out[12..14].copy_from_slice(&record[0x30..0x32]);
    let shape = dword(record, 0x34)?;
    if shape != 0 {
        let rva = shape
            .checked_sub(section.image_base)
            .ok_or("Collision pointer precedes image base")?;
        let offset = section.offset(rva, 1)?;
        out[9] = u8::from(bytes[offset] != 0);
    }
    out[14] = u8::from(dword(record, 0x38)? != 0);
    out[16..18].copy_from_slice(&record[0x2e..0x30]);
    Ok(out)
}

/// The gameplay tables `DATA_007207..7225` (hyperdrive masses, damage codes and
/// the other equipment tables), 647 bytes copied raw.
pub const GAMEPLAY_TABLES_BYTES: usize = 647;

pub fn decode_gameplay_tables(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, GAMEPLAY_TABLES_BYTES)?;
    let table = range(bytes, source_offset, GAMEPLAY_TABLES_BYTES)?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: table.to_vec(),
    })
}

/// The five open-air pad tables `DATA_006175..6179` F650 selects between:
/// 16 pads of three little-endian dwords, 192 bytes. `DATA_006176` and the later
/// labels are not at the numeric addresses their names suggest; the tables are
/// contiguous in declaration order and the next label is `DATA_006180`.
pub const PAD_TABLES_BYTES: usize = 0xc0;

/// F650 indexes the selected table by the bay number without bounds checking.
/// Bays 6 and 7 of the last table (`DATA_006179`, models that take the default
/// branch) read the first six dwords of `DATA_006180`, the ship index tables;
/// those bytes are imported with the pads so the overrun can be reproduced.
pub const PAD_OVERRUN_BYTES: usize = 0x18;

pub fn decode_pad_tables(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, PAD_TABLES_BYTES + PAD_OVERRUN_BYTES)?;
    let table = range(bytes, source_offset, PAD_TABLES_BYTES + PAD_OVERRUN_BYTES)?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: table.to_vec(),
    })
}

/// The 48-byte statistics block descriptor +0x38 points to (object set-up reads
/// forward thrust at +0x0, reverse thrust at +0x2, gun mounts at +0x4, mass at
/// +0x6), or `None` when the pointer is null.
pub fn decode_ship_stats(
    bytes: &[u8],
    table_rva: u32,
    index: u32,
) -> Result<Option<Vec<u8>>, String> {
    let (section, descriptor_offset) = model_descriptor(bytes, table_rva, index)?;
    let descriptor_rva = u32::try_from(
        descriptor_offset
            .checked_sub(section.raw)
            .ok_or("Descriptor precedes its section")?,
    )
    .map_err(|_| "Descriptor offset overflow")?
        + section.rva;
    let pointer = dword(bytes, section.offset(descriptor_rva + 0x38, 4)?)?;
    if pointer == 0 {
        return Ok(None);
    }
    let stats_rva = pointer
        .checked_sub(section.image_base)
        .ok_or("Ship stats pointer precedes image base")?;
    let offset = section.offset(stats_rva, SHIP_STATS_BYTES)?;
    Ok(Some(bytes[offset..offset + SHIP_STATS_BYTES].to_vec()))
}

#[derive(Debug)]
pub struct EmbeddedModelCommands {
    pub descriptor_offset: usize,
    pub source_offset: usize,
    pub vertex_count: u32,
    pub bytes: Vec<u8>,
}

/// A model's raw command program, bounded by its declared extent, without decoding.
/// The descriptor's vertex count, normal count and scale must equal the declarations
/// the extents came from, so a differently ordered table cannot borrow an extent.
pub fn model_command_bytes(
    bytes: &[u8],
    table_rva: u32,
    index: u32,
) -> Result<EmbeddedModelCommands, String> {
    // Descriptor programs have no length field; never infer length from a return.
    let size = *crate::model_extents::COMMAND_EXTENTS
        .get(index as usize)
        .ok_or("Model index is outside the declared model table.")? as usize;
    if size == 0 {
        return Err(format!("Model {index} is a null table entry."));
    }
    let (section, descriptor_offset) = model_descriptor(bytes, table_rva, index)?;
    let vertex_count = dword(bytes, descriptor_offset + 8)?;
    let shape = (
        vertex_count,
        dword(bytes, descriptor_offset + 16)?,
        dword(bytes, descriptor_offset + 20)?,
    );
    let (vertices, normals, scale) = crate::model_extents::DESCRIPTOR_SHAPES[index as usize];
    if shape != (u32::from(vertices), u32::from(normals), u32::from(scale)) {
        return Err(format!(
            "Model {index} descriptor {shape:?} differs from its declaration; extent not trusted."
        ));
    }
    let command_rva = dword(bytes, descriptor_offset)?
        .checked_sub(section.image_base)
        .ok_or("Command pointer precedes image base")?;
    let source_offset = section.offset(command_rva, size)?;
    let records = range(bytes, source_offset, size)?;
    Ok(EmbeddedModelCommands {
        descriptor_offset,
        source_offset,
        vertex_count,
        bytes: records.to_vec(),
    })
}

/// [`model_command_bytes`] that also requires every physical record to decode.
pub fn decode_model_commands(
    bytes: &[u8],
    table_rva: u32,
    index: u32,
) -> Result<EmbeddedModelCommands, String> {
    let commands = model_command_bytes(bytes, table_rva, index)?;
    ffe_core::model_commands::decode(&commands.bytes, commands.vertex_count)?;
    Ok(commands)
}

#[derive(Debug, Clone, Copy)]
pub struct BitmapLayout {
    pub primary_rva: u32,
    pub secondary_rva: u32,
    pub low_palette_rva: u32,
}

#[derive(Debug)]
pub struct EmbeddedModelNormals {
    pub descriptor_offset: usize,
    pub source_offset: usize,
    pub vertex_count: u32,
    pub normal_count: u32,
    pub bytes: Vec<u8>,
}

pub fn decode_model_normals(
    bytes: &[u8],
    table_rva: u32,
    index: u32,
) -> Result<EmbeddedModelNormals, String> {
    let vertices = decode_model_vertices(bytes, table_rva, index)?;
    let (section, descriptor_offset) = model_descriptor(bytes, table_rva, index)?;
    let normal_count = dword(bytes, descriptor_offset + 16)?;
    if !(0..=256).contains(&normal_count) || normal_count % 2 != 0 {
        return Err("Invalid bounded model normal count.".to_owned());
    }
    let pointer = dword(bytes, descriptor_offset + 12)?;
    if normal_count <= 2 {
        // Only the implicit pair: F1480 writes normals 0 and 1 from the shades
        // and computes no others, so a declared count of zero or two needs no
        // records and the original leaves the pointer null.
        return Ok(EmbeddedModelNormals {
            descriptor_offset,
            source_offset: 0,
            vertex_count: vertices.vertex_count,
            normal_count,
            bytes: Vec::new(),
        });
    }
    let rva = pointer
        .checked_sub(section.image_base)
        .ok_or("Normal pointer precedes image base")?;
    let source_offset = section.offset(rva, (normal_count as usize / 2 - 1) * 6)?;
    let records = range(bytes, source_offset, (normal_count as usize / 2 - 1) * 6)?;
    ffe_core::model_normals::validate(
        records,
        normal_count,
        &vertices.bytes,
        vertices.vertex_count,
    )?;
    Ok(EmbeddedModelNormals {
        descriptor_offset,
        source_offset,
        vertex_count: vertices.vertex_count,
        normal_count,
        bytes: records.to_vec(),
    })
}

#[derive(Debug)]
pub struct EmbeddedFont {
    pub source_offset: usize,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct EmbeddedSineTable {
    pub source_offset: usize,
    pub bytes: Vec<u8>,
}

pub fn decode_sine_table(bytes: &[u8], rva: u32) -> Result<EmbeddedSineTable, String> {
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, 65536)?;
    let table = range(bytes, source_offset, 65536)?;
    ffe_core::model_math::SineTable::from_bytes(table)?;
    Ok(EmbeddedSineTable {
        source_offset,
        bytes: table.to_vec(),
    })
}

#[derive(Debug)]
pub struct EmbeddedTable {
    pub source_offset: usize,
    pub bytes: Vec<u8>,
}

/// The original texture descriptor table (`DATA_007811`, 148 x 30 bytes).
pub fn decode_texture_descriptors(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    use ffe_core::model_texture::{Descriptors, DESCRIPTOR_BYTES, DESCRIPTOR_COUNT};
    let section = DataSection::parse(bytes)?;
    let size = DESCRIPTOR_COUNT * DESCRIPTOR_BYTES;
    let source_offset = section.offset(rva, size)?;
    let table = range(bytes, source_offset, size)?;
    Descriptors::from_bytes(table)?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: table.to_vec(),
    })
}

/// Model strings the JJFFE 2.8a7 declaration of `DATA_004682` lists. The release
/// executable's table continues past them (see [`decode_model_names`]).
pub const MODEL_NAME_COUNT: usize = 209;

/// Most pointers the scanned string table may hold.
pub const MODEL_NAME_LIMIT: usize = 512;

#[derive(Debug)]
pub struct EmbeddedModelNames {
    pub source_offset: usize,
    /// Every string, each followed by a NUL byte, in table order.
    pub bytes: Vec<u8>,
    pub count: usize,
}

/// The model string table (`DATA_004682`, "Ship string table"): pointers to
/// NUL-terminated Latin-1 strings, entry n the string of ffcode `0x4000 | n`.
/// Entry n is **not** model table slot n: the commands and F437/F439/F282 look
/// the code up in a model's statistics block at `+0xe`, and the table also holds
/// UI/decal strings. The release executable's table is longer than the JJFFE
/// declaration's 209 entries (219 on floppy 1.02: the extra ones include
/// "police" and the First Encounters logo), and the model text commands index
/// the same table through `0x4000 | n`. So the table is scanned, not taken from
/// the declaration: the scan stops at the first word that is not a pointer into
/// the data section holding a terminated string.
pub fn decode_model_names(bytes: &[u8], rva: u32) -> Result<EmbeddedModelNames, String> {
    let section = DataSection::parse(bytes)?;
    let first = section.offset(rva, 4)?;
    let mut joined = Vec::new();
    let mut count = 0;
    while count < MODEL_NAME_LIMIT {
        let pointer = dword(bytes, first + 4 * count)?;
        let Some(string_rva) = pointer.checked_sub(section.image_base) else {
            break;
        };
        let Ok(start) = section.offset(string_rva, 1) else {
            break;
        };
        let text = bytes
            .get(start..start.saturating_add(256).min(bytes.len()))
            .ok_or("Model name outside the executable")?;
        let Some(end) = text.iter().position(|b| *b == 0) else {
            break;
        };
        if text[..end]
            .iter()
            .any(|b| !matches!(b, 0x0a | 0x0d | 0x20..=0x7e))
        {
            break;
        }
        joined.extend_from_slice(&text[..end]);
        joined.push(0);
        count += 1;
    }
    // A truncated or misplaced table would silently lose names the commands use.
    if count < MODEL_NAME_COUNT {
        return Err(format!(
            "Model string table at {rva:#x} holds only {count} entries."
        ));
    }
    Ok(EmbeddedModelNames {
        source_offset: first,
        bytes: joined,
        count,
    })
}

/// The default light and shade table (`DATA_007231..7234`, 36 bytes).
pub fn decode_default_lighting(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    use ffe_core::model_lighting::{default_lighting, DEFAULT_LIGHTING_BYTES};
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, DEFAULT_LIGHTING_BYTES)?;
    let table = range(bytes, source_offset, DEFAULT_LIGHTING_BYTES)?;
    default_lighting(table)?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: table.to_vec(),
    })
}

/// The star shade tables (`DATA_007759`, six of 24 bytes).
pub fn decode_star_shades(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    use ffe_core::model_lighting::{star_shades, STAR_SHADE_TABLES};
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, STAR_SHADE_TABLES * 24)?;
    let table = range(bytes, source_offset, STAR_SHADE_TABLES * 24)?;
    star_shades(table)?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: table.to_vec(),
    })
}

/// The galaxy table and system-generation tables (`DATA_007106..7138`, 20,984 bytes).
pub fn decode_system_tables(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    use ffe_core::galaxy::{Tables, TABLES_BYTES};
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, TABLES_BYTES)?;
    let table = range(bytes, source_offset, TABLES_BYTES)?;
    Tables::new(table)?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: table.to_vec(),
    })
}

/// The market tables (`DATA_006164..6169`, 704 bytes) and the 38 commodity names that
/// `DATA_006162` points at (`DATA_006216..6253`, contiguous in the executable), as one
/// block: the tables, then each name with its zero.
pub fn decode_market_tables(
    bytes: &[u8],
    tables_rva: u32,
    names_rva: u32,
) -> Result<EmbeddedTable, String> {
    use ffe_core::market::{Tables, NAMES, TABLE_BYTES};
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(tables_rva, TABLE_BYTES)?;
    let mut block = range(bytes, source_offset, TABLE_BYTES)?.to_vec();
    let names_offset = section.offset(names_rva, 1)?;
    let mut at = names_offset;
    for _ in 0..NAMES {
        let rest = bytes.get(at..).ok_or("market names run past the file")?;
        let length = rest
            .iter()
            .position(|b| *b == 0)
            .ok_or("a market name is not terminated")?;
        block.extend_from_slice(&rest[..=length]);
        at += length + 1;
    }
    Tables::new(&block)?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: block,
    })
}

/// The galaxy generator's tables: the hand-made star vectors and type words
/// (`DATA_006793..6795`), the special sector keys, index, density grid, hash table and star
/// type tables (`DATA_006799..6811`) and the 15 hyperdrive records (`DATA_007187..`), as one
/// block in that order.
pub fn decode_galaxy_tables(
    bytes: &[u8],
    vectors_rva: u32,
    sector_rva: u32,
    drives_rva: u32,
) -> Result<EmbeddedTable, String> {
    use ffe_core::sector::{Tables, DRIVE_BYTES, SECTOR_BYTES, TABLE_BYTES, VECTORS_BYTES};
    let section = DataSection::parse(bytes)?;
    let first = section.offset(vectors_rva, VECTORS_BYTES)?;
    let mut block = Vec::with_capacity(TABLE_BYTES);
    block.extend_from_slice(range(bytes, first, VECTORS_BYTES)?);
    let second = section.offset(sector_rva, SECTOR_BYTES)?;
    block.extend_from_slice(range(bytes, second, SECTOR_BYTES)?);
    let third = section.offset(drives_rva, DRIVE_BYTES)?;
    block.extend_from_slice(range(bytes, third, DRIVE_BYTES)?);
    Tables::new(&block)?;
    Ok(EmbeddedTable {
        source_offset: first,
        bytes: block,
    })
}

/// The small-dot size tables (`DATA_007835` then `DATA_007836`, nine bytes each, contiguous).
pub fn decode_dot_tables(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    use ffe_core::model_dot::{DotTables, TABLE_BYTES};
    let section = DataSection::parse(bytes)?;
    let offset = section.offset(rva, TABLE_BYTES)?;
    let block = range(bytes, offset, TABLE_BYTES)?.to_vec();
    DotTables::new(&block)?;
    Ok(EmbeddedTable {
        source_offset: offset,
        bytes: block,
    })
}

/// The four star light-point tables (`DATA_007820`, `7821`, `7822`, `7828`; RVAs), 14 dwords
/// each, concatenated.
pub fn decode_star_point_tables(bytes: &[u8], rvas: [u32; 4]) -> Result<EmbeddedTable, String> {
    use ffe_core::star_points::{StarTables, TABLE_BYTES, TYPES};
    let section = DataSection::parse(bytes)?;
    let mut block = Vec::with_capacity(TABLE_BYTES);
    let mut first = 0;
    for (index, rva) in rvas.iter().enumerate() {
        let offset = section.offset(*rva, TYPES * 4)?;
        if index == 0 {
            first = offset;
        }
        block.extend_from_slice(range(bytes, offset, TYPES * 4)?);
    }
    StarTables::new(&block)?;
    Ok(EmbeddedTable {
        source_offset: first,
        bytes: block,
    })
}

/// The 267 strings the pointer table `DATA_006796` names (RVA of the table), as NUL-terminated
/// strings in order.
pub fn decode_system_strings(bytes: &[u8], table_rva: u32) -> Result<EmbeddedTable, String> {
    use ffe_core::sector::{Strings, STRING_COUNT};
    let section = DataSection::parse(bytes)?;
    let table = section.offset(table_rva, 4 * STRING_COUNT)?;
    let pointers = range(bytes, table, 4 * STRING_COUNT)?;
    let mut block = Vec::new();
    for pointer in pointers.chunks_exact(4) {
        let address = u32::from_le_bytes(pointer.try_into().unwrap());
        let rva = address
            .checked_sub(section.image_base)
            .ok_or("a system string pointer is below the image base")?;
        let start = section.offset(rva, 1)?;
        let rest = bytes
            .get(start..)
            .ok_or("a system string runs past the file")?;
        let length = rest
            .iter()
            .position(|b| *b == 0)
            .ok_or("a system string is not terminated")?;
        block.extend_from_slice(&rest[..=length]);
    }
    Strings::new(&block)?;
    Ok(EmbeddedTable {
        source_offset: table,
        bytes: block,
    })
}

/// Bytes of the planet engine's tables (`DATA_007836..7899`).
pub const TERRAIN_TABLE_BYTES: usize = 7539;
/// Bytes of the two terrain height maps (`DATA_007637`, `DATA_007638`).
pub const TERRAIN_MAP_BYTES: usize = 32768;

/// Offsets of the code pointers in the terrain table block and the rank each
/// must decode to (ranks count distinct targets in order of first appearance within
/// a class: the two columns of the type table, then the midpoint table).
const TERRAIN_FIRST_COLUMN: [u8; 14] = [1, 1, 1, 2, 1, 2, 2, 1, 3, 4, 1, 1, 1, 1];
const TERRAIN_SECOND_COLUMN: [u8; 14] = [1, 1, 1, 2, 1, 2, 2, 1, 3, 3, 1, 1, 1, 1];
const TERRAIN_MIDPOINTS: [u8; 32] = [
    1, 1, 1, 1, 1, 1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 18, 18, 18,
    19, 19, 19, 19,
];

/// The planet engine's tables, `DATA_007836..7899`. The code pointers in the type
/// and midpoint tables become the ranks above (1 upward, as the routines are
/// ordered) and the two height-map pointers become 1 and 2, so the block holds no
/// addresses. The map RVAs are those of floppy 1.02 (`maps_rva`, `DATA_007637`).
pub fn decode_terrain_tables(
    bytes: &[u8],
    rva: u32,
    maps_rva: u32,
) -> Result<EmbeddedTable, String> {
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, TERRAIN_TABLE_BYTES)?;
    let mut table = range(bytes, source_offset, TERRAIN_TABLE_BYTES)?.to_vec();
    let word = |table: &[u8], at: usize| u32::from_le_bytes(table[at..at + 4].try_into().unwrap());
    let put = |table: &mut Vec<u8>, at: usize, value: u32| {
        table[at..at + 4].copy_from_slice(&value.to_le_bytes());
    };
    for (at, map) in [(471, 1u32), (479, 0), (483, 1), (495, 1)] {
        let target = word(&table, at).checked_sub(section.image_base);
        if target != Some(maps_rva + 0x4000 * map) {
            return Err(
                "Terrain table height map pointer is not at its expected address".to_owned(),
            );
        }
        put(&mut table, at, map + 1);
    }
    let rank = |table: &mut Vec<u8>,
                slots: &mut dyn Iterator<Item = usize>,
                expected: &[u8]|
     -> Result<(), String> {
        let mut seen: Vec<u32> = Vec::new();
        for (slot, want) in slots.zip(expected) {
            let value = word(table, slot);
            let index = match seen.iter().position(|v| *v == value) {
                Some(index) => index,
                None => {
                    seen.push(value);
                    seen.len() - 1
                }
            };
            if index as u8 + 1 != *want || value < section.image_base {
                return Err(
                    "Terrain table code pointers do not match the original layout".to_owned(),
                );
            }
            put(table, slot, u32::from(*want));
        }
        Ok(())
    };
    rank(
        &mut table,
        &mut (0..14).map(|k| 503 + 48 * k),
        &TERRAIN_FIRST_COLUMN,
    )?;
    rank(
        &mut table,
        &mut (0..14).map(|k| 507 + 48 * k),
        &TERRAIN_SECOND_COLUMN,
    )?;
    rank(
        &mut table,
        &mut (0..32).map(|k| 1175 + 4 * k),
        &TERRAIN_MIDPOINTS,
    )?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: table,
    })
}

/// The two 128 by 128 terrain height maps `DATA_007637` and `DATA_007638`, which
/// are adjacent.
pub fn decode_terrain_maps(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, TERRAIN_MAP_BYTES)?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: range(bytes, source_offset, TERRAIN_MAP_BYTES)?.to_vec(),
    })
}

/// A model's collision stream (descriptor `+0x34`), or `None` for a null pointer,
/// with the number of bytes of backward region words before it. F576 reads those
/// words backwards from the stream start in pairs (a distance word, which is zero
/// at the end of the chain, then a direction and radius word), so the bytes from the
/// zero word up to the stream start come first in the returned bytes. The stream has
/// no length field, so it is walked as F582 reads it: entries of a type byte and a
/// flag byte (two more bytes when the type has `0x20`), then by `type & 0xc0` a
/// surface list, a terrain record (the `0xff` marker, six bytes, 16-byte records
/// to a zero word and its two bytes), a hierarchical reference (four bytes) or a
/// list, to a zero type byte.
pub fn decode_model_collision(
    bytes: &[u8],
    table_rva: u32,
    index: u32,
) -> Result<Option<(usize, Vec<u8>)>, String> {
    let (section, descriptor_offset) = model_descriptor(bytes, table_rva, index)?;
    let pointer = dword(bytes, descriptor_offset + 0x34)?;
    if pointer == 0 {
        return Ok(None);
    }
    let rva = pointer
        .checked_sub(section.image_base)
        .ok_or("Collision pointer precedes the image base")?;
    let start = section.offset(rva, 1)?;
    let at = |i: usize| -> Result<u8, String> {
        bytes
            .get(i)
            .copied()
            .ok_or_else(|| "Collision stream runs off the file".to_owned())
    };
    let list = |mut i: usize| -> Result<usize, String> {
        while at(i)? != 0 {
            i += 1;
        }
        Ok(i + 1)
    };
    let mut i = start;
    loop {
        let kind = at(i)?;
        i += 1;
        if kind == 0 {
            break;
        }
        i += 1;
        if kind & 0x20 != 0 {
            i += 2;
        }
        match kind & 0xc0 {
            0 => {
                if at(i)? == 0xff {
                    i += 1 + 6;
                    while at(i)? != 0 || at(i + 1)? != 0 {
                        i += 16;
                    }
                    i += 2;
                } else {
                    i = list(i)?;
                }
            }
            0x80 => i += 4,
            _ => i = list(i)?,
        }
        if i - start > 0x10000 {
            return Err("Collision stream is implausibly long".to_owned());
        }
    }
    // The backward chain: pairs of words before the stream until a zero distance.
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let mut back = 2;
    while word(start - back) != 0 {
        back += 4;
        if back > 0x100 || start < back {
            return Err("Collision region chain is implausibly long".to_owned());
        }
    }
    Ok(Some((back, bytes[start - back..i].to_vec())))
}

/// Entries in the name string table (`DATA_006272`).
pub const NAME_STRINGS: usize = 510;
/// Bytes of the name range tables (`DATA_006274..6279`) that lead the name asset.
pub const NAME_RANGE_BYTES: usize = 120;
/// Bytes of the hand-coded systems' text (`DATA_007139` to the end of the last name).
pub const HANDCODED_TEXT_BYTES: usize = 7706;

/// The name generator's data (v19): the range tables `DATA_006274..6279` (120 bytes,
/// which follow the pointer table and 24 zero bytes) and then the 510 strings the
/// pointer table `DATA_006272` names, each NUL-terminated, in table order, followed by the hand-coded systems' text (`HANDCODED_TEXT_BYTES`
/// bytes from `text_rva`).
pub fn decode_name_text(
    bytes: &[u8],
    table_rva: u32,
    text_rva: u32,
) -> Result<EmbeddedTable, String> {
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(table_rva, NAME_STRINGS * 4 + 24 + NAME_RANGE_BYTES)?;
    let mut out = range(
        bytes,
        source_offset + NAME_STRINGS * 4 + 24,
        NAME_RANGE_BYTES,
    )?
    .to_vec();
    for k in 0..NAME_STRINGS {
        let pointer = dword(bytes, source_offset + 4 * k)?
            .checked_sub(section.image_base)
            .ok_or("Name string pointer precedes the image base")?;
        let at = section.offset(pointer, 1)?;
        let length = bytes[at..]
            .iter()
            .take(80)
            .position(|b| *b == 0)
            .ok_or("Name string is not terminated")?;
        out.extend_from_slice(&bytes[at..=at + length]);
    }
    // The hand-coded systems' text, which F873 and F874 index from `DATA_007139`.
    let text = range(
        bytes,
        section.offset(text_rva, HANDCODED_TEXT_BYTES)?,
        HANDCODED_TEXT_BYTES,
    )?;
    if &text[0x18bd..0x18c1] != b"Sol\0" || text[HANDCODED_TEXT_BYTES - 1] != 0 {
        return Err("Hand-coded system text is not where expected".to_owned());
    }
    out.extend_from_slice(text);
    Ok(EmbeddedTable {
        source_offset,
        bytes: out,
    })
}

/// The original 12-bit colour to fixed low-palette map (`DATA_007770`, 4096 bytes).
pub fn decode_colour_map(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    use ffe_core::model_texture::{validate_colour_map, COLOUR_MAP_ENTRIES};
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, COLOUR_MAP_ENTRIES)?;
    let table = range(bytes, source_offset, COLOUR_MAP_ENTRIES)?;
    validate_colour_map(table)?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: table.to_vec(),
    })
}

/// The 18 intro credit strings plus the version line (`ffedat.asm`'s SysMenu entries `0x941b..0x942c`)
/// as raw NUL-terminated bytes, with their `0x0d`/`0xfd`/`0xfe` layout codes
/// intact. The 19th table entry (`0x942d`) is JJFFE's version line and is not
/// imported here.
pub const INTRO_CREDIT_COUNT: usize = 19;
/// Bytes of `DATA_007648`, the signed character-width table F1398 wraps with.
pub const CHARACTER_WIDTHS_BYTES: usize = 256;

/// The intro credits: 18 consecutive NUL-terminated strings. The block is read
/// from the first string to its terminator rather than from a fixed size, so a
/// release whose strings differ still fails or imports consistently.
pub fn decode_intro_credits(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, 1)?;
    let mut out = Vec::new();
    let mut start = source_offset;
    for index in 0..INTRO_CREDIT_COUNT {
        let rest = bytes
            .get(start..)
            .ok_or("an intro credit runs past the file")?;
        let length = rest
            .iter()
            .take(4096)
            .position(|byte| *byte == 0)
            .ok_or("an intro credit is not terminated")?;
        if length == 0 {
            return Err(format!("intro credit {index} is empty"));
        }
        out.extend_from_slice(&rest[..=length]);
        start += length + 1;
    }
    Ok(EmbeddedTable {
        source_offset,
        bytes: out,
    })
}

/// The 256-byte signed character-width table `DATA_007648`. The intro's wrap
/// (F1398) and the `0x3022` model-text path read it; the original keeps it in
/// the data section as signed bytes.
pub fn decode_character_widths(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, CHARACTER_WIDTHS_BYTES)?;
    let table = range(bytes, source_offset, CHARACTER_WIDTHS_BYTES)?;
    Ok(EmbeddedTable {
        source_offset,
        bytes: table.to_vec(),
    })
}

pub fn decode_font(bytes: &[u8], rva: u32) -> Result<EmbeddedFont, String> {
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, 1160)?;
    let font = range(bytes, source_offset, 1160)?;
    if font
        .chunks_exact(10)
        .any(|glyph| !(1..=9).contains(&glyph[9]))
    {
        return Err("font has an invalid advance width".to_owned());
    }
    Ok(EmbeddedFont {
        source_offset,
        bytes: font.to_vec(),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub struct EmbeddedBitmap {
    pub table: &'static str,
    pub index: u32,
    pub source_offset: usize,
    pub width: u32,
    /// Zero for the two special texture blocks, not a display height.
    pub height: u32,
    pub offset_x: i16,
    pub offset_y: i16,
    pub masked_zero: bool,
    pub indices: Vec<u8>,
}

#[derive(Debug)]
pub struct EmbeddedAssets {
    /// The fixed low 128 colours, original six-bit components. Upper colours
    /// are assigned at runtime and must not be guessed from a cabin palette.
    pub low_palette_rgb6: Vec<u8>,
    pub bitmaps: Vec<EmbeddedBitmap>,
}

fn range(bytes: &[u8], offset: usize, size: usize) -> Result<&[u8], String> {
    let end = offset.checked_add(size).ok_or("PE range overflow")?;
    bytes
        .get(offset..end)
        .ok_or_else(|| format!("truncated PE range at {offset:#x}, length {size}"))
}

fn word(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = range(bytes, offset, 2)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn dword(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = range(bytes, offset, 4)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

struct DataSection {
    image_base: u32,
    rva: u32,
    raw: usize,
    size: usize,
}

impl DataSection {
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 8 * 1024 * 1024 || range(bytes, 0, 2)? != b"MZ" {
            return Err("expected a bounded MZ/PE executable".to_owned());
        }
        let pe = dword(bytes, 60)? as usize;
        if range(bytes, pe, 4)? != b"PE\0\0" || word(bytes, pe + 4)? != 0x14c {
            return Err("expected an i386 PE executable".to_owned());
        }
        let count = usize::from(word(bytes, pe + 6)?);
        let optional_size = usize::from(word(bytes, pe + 20)?);
        if count == 0 || count > 96 || optional_size < 96 {
            return Err("invalid PE section count or optional header size".to_owned());
        }
        let optional = range(bytes, pe + 24, optional_size)?;
        if word(optional, 0)? != 0x10b {
            return Err("expected a PE32 optional header".to_owned());
        }
        let image_base = dword(optional, 28)?;
        let table = pe + 24 + optional_size;
        range(bytes, table, count * 40)?;
        let mut found = None;
        for index in 0..count {
            let section = table + index * 40;
            if range(bytes, section, 8)? == b"DATA\0\0\0\0" {
                if found.is_some() {
                    return Err("ambiguous PE DATA sections".to_owned());
                }
                let rva = dword(bytes, section + 12)?;
                let size = dword(bytes, section + 16)? as usize;
                let raw = dword(bytes, section + 20)? as usize;
                if raw < table + count * 40 || size == 0 {
                    return Err("PE DATA overlaps headers or has no file data".to_owned());
                }
                range(bytes, raw, size)?;
                rva.checked_add(u32::try_from(size).map_err(|_| "PE section size overflow")?)
                    .ok_or("PE DATA RVA overflow")?;
                found = Some(Self {
                    image_base,
                    rva,
                    raw,
                    size,
                });
            }
        }
        found.ok_or_else(|| "PE DATA section is missing".to_owned())
    }

    fn offset(&self, rva: u32, size: usize) -> Result<usize, String> {
        let relative = rva.checked_sub(self.rva).ok_or("RVA precedes PE DATA")? as usize;
        if relative.checked_add(size).ok_or("PE DATA range overflow")? > self.size {
            return Err("RVA refers outside file-backed PE DATA".to_owned());
        }
        Ok(self.raw + relative)
    }
}

pub fn decode_bitmaps(bytes: &[u8], layout: BitmapLayout) -> Result<EmbeddedAssets, String> {
    let section = DataSection::parse(bytes)?;
    let palette_offset = section.offset(layout.low_palette_rva, 384)?;
    let low_palette_rgb6 = range(bytes, palette_offset, 384)?.to_vec();
    if low_palette_rgb6.iter().any(|component| *component > 63) {
        return Err("fixed low palette contains a non-six-bit component".to_owned());
    }
    let mut bitmaps = Vec::with_capacity(347);
    for (table, rva, count) in [
        ("primary", layout.primary_rva, 96),
        ("secondary", layout.secondary_rva, 251),
    ] {
        let table_offset = section.offset(rva, count * 4)?;
        for index in 0..count {
            let pointer = dword(bytes, table_offset + index * 4)?;
            let rva = pointer
                .checked_sub(section.image_base)
                .ok_or("bitmap pointer precedes image base")?;
            let source_offset = section.offset(rva, 8)?;
            let header = range(bytes, source_offset, 8)?;
            let flags = header[0];
            let height = u32::from(header[1]);
            let width = u32::from(word(header, 2)?);
            let texture_block = table == "primary"
                && matches!(index, 94 | 95)
                && flags == 0
                && width == 64
                && height == 0
                && header[4..] == [0; 4];
            if flags > 1 || width == 0 || width > 320 || (height == 0 && !texture_block) {
                return Err(format!(
                    "{table}/{index}: unsupported bitmap header at {source_offset:#x}"
                ));
            }
            // These exact table slots have 32768 payload bytes in both verified
            // releases. Preserve them as raw blocks, not guessed 64x512 images.
            let size = if texture_block {
                32768
            } else {
                (width * height) as usize
            };
            section.offset(rva, 8 + size)?;
            bitmaps.push(EmbeddedBitmap {
                table,
                index: index as u32,
                source_offset,
                width,
                height,
                offset_x: word(header, 4)? as i16,
                offset_y: word(header, 6)? as i16,
                masked_zero: flags & 1 != 0,
                indices: range(bytes, source_offset + 8, size)?.to_vec(),
            });
        }
    }
    Ok(EmbeddedAssets {
        low_palette_rgb6,
        bitmaps,
    })
}
