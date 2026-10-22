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

/// Entries in the original model name table (`DATA_004682`, "Ship string table").
pub const MODEL_NAME_COUNT: usize = 209;

/// The model name table: 209 pointers to NUL-terminated Latin-1 strings, entry n
/// naming model table slot n. Stored as the strings joined by NUL bytes (each one
/// terminated), in table order.
pub fn decode_model_names(bytes: &[u8], rva: u32) -> Result<EmbeddedTable, String> {
    let section = DataSection::parse(bytes)?;
    let source_offset = section.offset(rva, MODEL_NAME_COUNT * 4)?;
    let mut joined = Vec::new();
    for entry in 0..MODEL_NAME_COUNT {
        let pointer = dword(bytes, source_offset + 4 * entry)?;
        let string_rva = pointer
            .checked_sub(section.image_base)
            .ok_or("Model name pointer precedes the image base")?;
        let start = section.offset(string_rva, 1)?;
        let text = bytes
            .get(start..start.saturating_add(256).min(bytes.len()))
            .ok_or("Model name outside the executable")?;
        let end = text
            .iter()
            .position(|b| *b == 0)
            .ok_or("Model name longer than 255 bytes")?;
        joined.extend_from_slice(&text[..end]);
        joined.push(0);
    }
    Ok(EmbeddedTable {
        source_offset,
        bytes: joined,
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
