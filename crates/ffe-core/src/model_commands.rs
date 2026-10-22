//! Structural station-program decoding and verified command helpers, not a full executor.
use crate::model::{variable, VertexContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    pub offset: usize,
    pub material: u16,
    pub normal: u16,
    pub vertices: Vec<u8>,
    pub reflected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    pub offset: usize,
    pub material: u16,
    /// Raw F1474 packed-angle operand.
    pub angle: i16,
    /// `payload[4] & 0x80`; the remaining bits are not used by the handler.
    pub flag: bool,
    pub vertex: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub offset: usize,
    pub material: u16,
    pub vertices: [u8; 2],
}

/// F1747 text operands. The glyph programs and expanded string are not executed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub offset: usize,
    pub material: u16,
    /// Normal entry whose culled flag skips the whole text.
    pub normal: u8,
    /// Low nibble of byte 3; stored by the handler, not read before glyph execution.
    pub size: u8,
    /// High nibble of byte 3, passed to F1539 to select the glyph model set.
    pub glyphs: u8,
    /// Position vertex, resolved through F1470.
    pub vertex: u8,
    /// Word at +4 (position vertex plus orientation byte), passed to F1740.
    pub frame: u16,
    /// String identifier passed to F1344 string expansion.
    pub string: u16,
}

/// The scale adjustment F1750 adds to a child's model scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildScale {
    /// A signed literal (opcode 27 byte 3 high nibble, or 0 for opcode 14).
    Literal(i8),
    /// Local variable `0..16` (opcode 27 with byte 3 bit 3 set).
    Local(u8),
}

/// F1749/F1748 -> F1750 child-model call operands. The child program is not
/// executed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submodel {
    pub offset: usize,
    /// `code >> 5`, passed to F1538.
    pub model: u16,
    /// Position vertex, projected through F1471.
    pub vertex: u8,
    /// Word at +0, passed to F1740.
    pub frame: u16,
    /// Four extra vertex bytes (after the fixed operands), present when bit 7 of
    /// byte 1 is set.
    pub extra: Option<[u8; 4]>,
    /// Opcode 27's own child material (word +4, resolved through F1694); opcode 14
    /// passes the instance colour instead.
    pub material: Option<u16>,
    pub scale: ChildScale,
}

impl Submodel {
    /// Extra vertices in the original F1750 resolution order (+4, +2, +3, +5),
    /// ending at the first 0x7f. The original resolves them only for a visible child.
    pub fn extra_vertices(&self) -> Vec<u8> {
        let Some(extra) = self.extra else {
            return Vec::new();
        };
        [extra[2], extra[0], extra[1], extra[3]]
            .into_iter()
            .take_while(|index| *index != 0x7f)
            .collect()
    }
}

/// F1721 complex-polygon operands. The outline segments (DATA_007817 handlers) are
/// kept raw; they build the outline and are not executed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Polygon {
    pub offset: usize,
    pub material: u16,
    /// Low 7 bits of byte 2: the normal whose culled flag skips the polygon.
    pub normal: u8,
    /// Bit 7 of byte 2.
    pub flag: bool,
    /// Low 7 bits of the first segment byte: the vertex that keys the polygon.
    pub vertex: u8,
    /// Segment bytes (byte 3 gives their length).
    pub segments: Vec<u8>,
}

/// F1738 thruster operands: an exhaust glow between two vertices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thruster {
    pub offset: usize,
    pub material: u16,
    /// Vertices A (byte 2) and B (byte 3); A keys the glow.
    pub vertices: [u8; 2],
    /// Signed word +4, scaled by the instance scale and A's depth.
    pub width: i16,
}

/// F1874 curved-surface operands, rendered by F1876. The entries (parsed by F1823)
/// are kept raw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    pub offset: usize,
    /// Word +0, passed to F1876.
    pub word: i16,
    /// Origin vertex (byte +2, view space through F1470).
    pub origin: u8,
    /// Axis vertices (bytes +3..+5, object space through F1472).
    pub axes: [u8; 3],
    /// Words +6, +8 and +10, passed to F1876 (+8 shifted left by 16).
    pub parameters: [i16; 3],
    /// 16-byte entries up to the zero terminator word.
    pub entries: Vec<[u8; 16]>,
}

/// F1760/F1761 operands: a shape between two projected vertices. Only operands
/// are decoded; F1761's drawing is not ported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Beam {
    pub offset: usize,
    pub material: u16,
    /// Low and high bytes of word +2, each projected (F1471) with a near test.
    pub vertices: [u8; 2],
    /// Words +4..+10 passed to F1761.
    pub parameters: [i16; 4],
}

/// F1766 operands: a material, an F1474 size operand and vertex pairs up to the
/// first pair holding 0x7f (inclusive). Drawing is not ported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    pub offset: usize,
    pub material: u16,
    pub size: u16,
    pub pairs: Vec<[u8; 2]>,
}

/// F1737 operands (opcode 22): a curve through four projected vertices, drawn by
/// F1811 from its queue entry. Ten bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spline {
    pub offset: usize,
    pub material: u16,
    /// Bytes +2..+5. F1737 projects them in reverse order (+5 first) and stores
    /// screen points +4, +2, +5, +3; the entry is keyed by +4's view.
    pub vertices: [u8; 4],
    /// Byte +6: the normal whose cull flag skips the curve and whose colour lights it.
    pub normal: u8,
}

/// F1763 operands (opcode 18): string `string` drawn by F1812 at vertex
/// `code >> 5`'s screen point. Four bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub offset: usize,
    pub vertex: u8,
    pub string: u16,
}

/// F1762 colour operands: 6 bytes, or 20 when byte 3 selects an alternative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Colour {
    pub offset: usize,
    /// Word +0, then the seven alternative words at +4..+17 when a selector is present.
    pub materials: Vec<u16>,
    /// Low 7 bits of byte 2: the normal whose colour lights an unlit-clear material.
    pub normal: u8,
    /// Bit 7 of byte 2: write normal entry 0's colour instead of the instance colour.
    pub to_normal_zero: bool,
    /// Byte 3: F1473 variable whose low 3 bits pick `materials[k]` (0 keeps word +0).
    pub selector: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub offset: usize,
    pub word: u16,
    pub size: usize,
}

#[derive(Debug)]
pub struct Program {
    pub commands: Vec<Command>,
    pub faces: Vec<Face>,
    pub points: Vec<Point>,
    pub lines: Vec<Line>,
    pub texts: Vec<Text>,
    pub submodels: Vec<Submodel>,
    pub colours: Vec<Colour>,
    pub polygons: Vec<Polygon>,
    pub thrusters: Vec<Thruster>,
    pub surfaces: Vec<Surface>,
    pub beams: Vec<Beam>,
    pub chains: Vec<Chain>,
    pub splines: Vec<Spline>,
    pub labels: Vec<Label>,
    /// Bytes after the program's final terminator that belong to the model's
    /// declaration block but are not commands.
    pub trailing: usize,
}

fn word(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let data = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("Truncated model command at {offset:#x}."))?;
    Ok(u16::from_le_bytes([data[0], data[1]]))
}

/// A command vertex byte the original resolves through F1470/F1471, which sign-extend
/// it: 0..count-1 are the model's own vertices; 0x80..0xff (-128..-1) are external
/// vertices that F1478 copies from the parent instance's table at +0x6c (child models
/// only, resolved at execution). Only the face, line, point, text and child-call operands use this;
/// other command forms still reject bytes outside the model.
pub fn vertex_reference(index: u8, vertex_count: u32) -> bool {
    index >= 0x80 || u32::from(index) < vertex_count
}

/// Decode every physical record, including records after conditional/block terminators.
/// Only known station-stream forms are accepted; no byte scanning for "likely" polygons.
pub fn decode(bytes: &[u8], vertex_count: u32) -> Result<Program, String> {
    if bytes.is_empty()
        || bytes.len() > 16384
        || bytes.len() % 2 != 0
        || vertex_count == 0
        || vertex_count > 128
        || vertex_count % 2 != 0
    {
        return Err("Invalid bounded model command stream/count.".to_owned());
    }
    let error = match decode_prefix(bytes, vertex_count) {
        Ok(program) => return Ok(program),
        Err(error) => error,
    };
    // Some declaration blocks carry data after the final terminator. The program is
    // the longest prefix that ends in a terminator and decodes with every branch
    // inside it; a zero word that is not a record boundary cannot satisfy that.
    for end in (2..bytes.len()).step_by(2).rev() {
        if bytes[end - 2..end] != [0, 0] {
            continue;
        }
        if let Ok(mut program) = decode_prefix(&bytes[..end], vertex_count) {
            program.trailing = bytes.len() - end;
            return Ok(program);
        }
    }
    Err(error)
}

fn decode_prefix(bytes: &[u8], vertex_count: u32) -> Result<Program, String> {
    if bytes.is_empty()
        || bytes.len() > 16384
        || bytes.len() % 2 != 0
        || vertex_count == 0
        || vertex_count > 128
        || vertex_count % 2 != 0
    {
        return Err("Invalid bounded model command stream/count.".to_owned());
    }
    let mut commands = Vec::new();
    let mut faces = Vec::new();
    let mut points = Vec::new();
    let mut lines = Vec::new();
    let mut texts = Vec::new();
    let mut submodels = Vec::new();
    let mut colours = Vec::new();
    let mut polygons = Vec::new();
    let mut thrusters = Vec::new();
    let mut surfaces = Vec::new();
    let mut beams = Vec::new();
    let mut chains = Vec::new();
    let mut splines = Vec::new();
    let mut labels = Vec::new();
    let mut branches = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let code = word(bytes, offset)?;
        let opcode = code & 31;
        let size = match opcode {
            0 if code == 0 => 2,
            1 => 8,
            // Word, normal byte, segment length L, then L segment bytes.
            5 => 6 + usize::from(*bytes.get(offset + 5).ok_or("Truncated polygon command.")?),
            2 => 6,
            3 | 7 => 8,
            4 | 8 => 10,
            6 => crate::model_depth::decode(&bytes[offset..])?.1,
            9 => 8,
            15 => 2,
            17 => 14,
            18 => 4,
            22 => 10,
            25 | 30 => 2,
            // Material, size, then 2-byte pairs through the first one holding 0x7f.
            24 => {
                let mut end = offset + 6;
                loop {
                    let pair = bytes
                        .get(end..end + 2)
                        .ok_or("Truncated vertex-pair command.")?;
                    end += 2;
                    if pair.contains(&0x7f) {
                        break;
                    }
                }
                end - offset
            }
            // F1697: start a nested depth block keyed by vertex `code >> 5`, or (bit 15)
            // end it. Two bytes either way.
            21 => {
                if code & 0x8000 == 0 && u32::from((code >> 5) as u8) >= vertex_count {
                    return Err(format!(
                        "Nested block references external/out-of-range vertex at {offset:#x}."
                    ));
                }
                2
            }
            // Code, 12 header bytes, 16-byte entries while the first word is non-zero,
            // then the zero word (F1823).
            31 => {
                let mut end = offset + 14;
                while word(bytes, end)? != 0 {
                    end += 16;
                }
                end + 2 - offset
            }
            10 => 10,
            27 => {
                if word(bytes, offset + 2)? & 0x8000 != 0 {
                    12
                } else {
                    8
                }
            }
            11..=14 | 19 | 20 | 29 => {
                if matches!(opcode, 11 | 12 | 19 | 20) && code >> 5 != 0 {
                    let target = (offset as i32 + 4)
                        .checked_add(((code as i16) >> 5) as i32 * 2)
                        .ok_or("Model branch overflow.")?;
                    branches.push((offset, target));
                }
                if opcode == 14 && word(bytes, offset + 2)? & 0x8000 != 0 {
                    8
                } else {
                    4
                }
            }
            26 => {
                if *bytes.get(offset + 5).ok_or("Truncated colour command.")? == 0 {
                    6
                } else {
                    20
                }
            }
            // F1741 0xc000: basis from vertex bytes +2, +3, +4 (object space); six bytes.
            28 if code & 0xc000 == 0xc000 => {
                let vertices = bytes
                    .get(offset + 2..offset + 5)
                    .ok_or("Truncated vertex basis command.")?;
                if vertices.iter().any(|v| u32::from(*v) >= vertex_count) {
                    return Err(format!(
                        "Vertex basis references external/out-of-range vertex at {offset:#x}."
                    ));
                }
                6
            }
            28 => 4,
            _ => return Err(format!("Unsupported command {code:#06x} at {offset:#x}.")),
        };
        if offset + size > bytes.len() {
            return Err(format!("Truncated command {code:#06x} at {offset:#x}."));
        }
        if opcode == 1 {
            let payload = &bytes[offset + 2..offset + 8];
            let point = Point {
                offset,
                material: word(payload, 0)?,
                angle: word(payload, 2)? as i16,
                flag: payload[4] & 0x80 != 0,
                vertex: payload[5],
            };
            if !vertex_reference(point.vertex, vertex_count) {
                return Err(format!(
                    "Point source references external/out-of-range vertex at {offset:#x}."
                ));
            }
            points.push(point);
        }
        if opcode == 2 {
            let payload = &bytes[offset + 2..offset + 6];
            let line = Line {
                offset,
                material: word(payload, 0)?,
                vertices: [payload[2], payload[3]],
            };
            if line
                .vertices
                .iter()
                .any(|index| !vertex_reference(*index, vertex_count))
            {
                return Err(format!(
                    "Line references external/out-of-range vertex at {offset:#x}."
                ));
            }
            lines.push(line);
        }
        if opcode == 10 {
            let payload = &bytes[offset + 2..offset + 10];
            let text = Text {
                offset,
                material: word(payload, 0)?,
                normal: payload[2],
                size: payload[3] & 15,
                glyphs: payload[3] >> 4,
                vertex: payload[4],
                frame: word(payload, 4)?,
                string: word(payload, 6)?,
            };
            if !vertex_reference(text.vertex, vertex_count) {
                return Err(format!(
                    "Text references external/out-of-range vertex at {offset:#x}."
                ));
            }
            texts.push(text);
        }
        if matches!(opcode, 14 | 27) {
            let payload = &bytes[offset + 2..offset + size];
            // Opcode 14: vertex/frame word then extras at +2; opcode 27 adds byte 3
            // (scale), word +4 (material) and puts its extras at +6.
            let fixed = if opcode == 14 { 2 } else { 6 };
            let submodel = Submodel {
                offset,
                model: code >> 5,
                vertex: payload[0],
                frame: word(payload, 0)?,
                extra: (size == fixed + 6).then(|| {
                    [
                        payload[fixed],
                        payload[fixed + 1],
                        payload[fixed + 2],
                        payload[fixed + 3],
                    ]
                }),
                material: (opcode == 27).then(|| word(payload, 4)).transpose()?,
                scale: match (opcode, payload.get(3)) {
                    (27, Some(byte)) if byte & 8 != 0 => ChildScale::Local(byte >> 4),
                    (27, Some(byte)) => ChildScale::Literal((*byte as i8) >> 4),
                    _ => ChildScale::Literal(0),
                },
            };
            if std::iter::once(submodel.vertex)
                .chain(submodel.extra_vertices())
                .any(|index| !vertex_reference(index, vertex_count))
            {
                return Err(format!(
                    "Submodel references external/out-of-range vertex at {offset:#x}."
                ));
            }
            submodels.push(submodel);
        }
        if opcode == 5 {
            let payload = &bytes[offset + 2..offset + size];
            if size < 8 {
                return Err(format!("Polygon at {offset:#x} has no segments."));
            }
            let polygon = Polygon {
                offset,
                material: word(payload, 0)?,
                normal: payload[2] & 0x7f,
                flag: payload[2] & 0x80 != 0,
                vertex: payload[4] & 0x7f,
                segments: payload[4..].to_vec(),
            };
            if u32::from(polygon.vertex) >= vertex_count {
                return Err(format!(
                    "Polygon references external/out-of-range vertex at {offset:#x}."
                ));
            }
            polygons.push(polygon);
        }
        if opcode == 9 {
            let payload = &bytes[offset + 2..offset + 8];
            let thruster = Thruster {
                offset,
                material: word(payload, 0)?,
                vertices: [payload[2], payload[3]],
                width: word(payload, 4)? as i16,
            };
            if thruster
                .vertices
                .iter()
                .any(|index| u32::from(*index) >= vertex_count)
            {
                return Err(format!(
                    "Thruster references external/out-of-range vertex at {offset:#x}."
                ));
            }
            thrusters.push(thruster);
        }
        if opcode == 18 {
            let label = Label {
                offset,
                vertex: (code >> 5) as u8,
                string: word(bytes, offset + 2)?,
            };
            if u32::from(label.vertex) >= vertex_count {
                return Err(format!(
                    "Label references external/out-of-range vertex at {offset:#x}."
                ));
            }
            labels.push(label);
        }
        if opcode == 22 {
            let payload = &bytes[offset + 2..offset + 10];
            let spline = Spline {
                offset,
                material: word(payload, 0)?,
                vertices: [payload[2], payload[3], payload[4], payload[5]],
                normal: payload[6],
            };
            if spline
                .vertices
                .iter()
                .any(|v| u32::from(*v) >= vertex_count)
            {
                return Err(format!(
                    "Spline references external/out-of-range vertex at {offset:#x}."
                ));
            }
            splines.push(spline);
        }
        if opcode == 17 {
            let payload = &bytes[offset + 2..offset + 14];
            let beam = Beam {
                offset,
                material: word(payload, 0)?,
                vertices: [payload[2], payload[3]],
                parameters: std::array::from_fn(|i| {
                    i16::from_le_bytes([payload[4 + i * 2], payload[5 + i * 2]])
                }),
            };
            if beam.vertices.iter().any(|v| u32::from(*v) >= vertex_count) {
                return Err(format!(
                    "Beam references external/out-of-range vertex at {offset:#x}."
                ));
            }
            beams.push(beam);
        }
        if opcode == 24 {
            let payload = &bytes[offset + 2..offset + size];
            let chain = Chain {
                offset,
                material: word(payload, 0)?,
                size: word(payload, 2)?,
                pairs: payload[4..].chunks_exact(2).map(|p| [p[0], p[1]]).collect(),
            };
            if chain
                .pairs
                .iter()
                .flatten()
                .any(|v| *v != 0x7f && u32::from(*v) >= vertex_count)
            {
                return Err(format!(
                    "Vertex-pair command references external/out-of-range vertex at {offset:#x}."
                ));
            }
            chains.push(chain);
        }
        if opcode == 31 {
            let payload = &bytes[offset + 2..offset + size];
            let surface = Surface {
                offset,
                word: word(payload, 0)? as i16,
                origin: payload[2],
                axes: [payload[3], payload[4], payload[5]],
                parameters: [
                    word(payload, 6)? as i16,
                    word(payload, 8)? as i16,
                    word(payload, 10)? as i16,
                ],
                entries: payload[12..payload.len() - 2]
                    .chunks_exact(16)
                    .map(|entry| std::array::from_fn(|i| entry[i]))
                    .collect(),
            };
            if std::iter::once(surface.origin)
                .chain(surface.axes)
                .any(|index| u32::from(index) >= vertex_count)
            {
                return Err(format!(
                    "Surface references external/out-of-range vertex at {offset:#x}."
                ));
            }
            surfaces.push(surface);
        }
        if opcode == 26 {
            let payload = &bytes[offset + 2..offset + size];
            colours.push(Colour {
                offset,
                materials: std::iter::once(0)
                    .chain(if size == 20 { 2..9 } else { 0..0 })
                    .map(|index| word(payload, index * 2))
                    .collect::<Result<_, _>>()?,
                normal: payload[2] & 0x7f,
                to_normal_zero: payload[2] & 0x80 != 0,
                selector: payload[3],
            });
        }
        if matches!(opcode, 3 | 4 | 7 | 8) {
            if code != opcode {
                return Err(format!("Unexpected face command flags at {offset:#x}."));
            }
            let payload = &bytes[offset + 2..offset + size];
            let (vertices, normal) = if matches!(opcode, 3 | 7) {
                // F1707 passes byte3, byte2, byte5 (normal at byte4, not byte5).
                // F1711 passes byte4, byte2, byte5, byte3.
                (
                    vec![payload[3], payload[2], payload[5]],
                    u16::from(payload[4]),
                )
            } else {
                (
                    vec![payload[4], payload[2], payload[5], payload[3]],
                    word(payload, 6)?,
                )
            };
            if vertices
                .iter()
                .any(|index| !vertex_reference(*index, vertex_count))
            {
                return Err(format!(
                    "Face references external/out-of-range vertex at {offset:#x}."
                ));
            }
            let material = word(payload, 0)?;
            if matches!(opcode, 7 | 8) {
                let mirrored_vertices: Vec<_> = vertices.iter().map(|v| *v ^ 1).collect();
                if mirrored_vertices
                    .iter()
                    .any(|index| !vertex_reference(*index, vertex_count))
                {
                    return Err(format!(
                        "Reflected face references out-of-range vertex at {offset:#x}."
                    ));
                }
                faces.push(Face {
                    offset,
                    material,
                    normal: normal ^ 1,
                    vertices: mirrored_vertices,
                    reflected: true,
                });
            }
            faces.push(Face {
                offset,
                material,
                normal,
                vertices,
                reflected: false,
            });
        }
        commands.push(Command {
            offset,
            word: code,
            size,
        });
        offset += size;
    }
    if commands.last().is_none_or(|c| c.word != 0) {
        return Err("Model command stream lacks a final terminator.".to_owned());
    }
    for (source, target) in branches {
        if target < 0
            || !commands
                .iter()
                .any(|command| command.offset == target as usize)
        {
            return Err(format!(
                "Model branch at {source:#x} targets outside a record boundary."
            ));
        }
    }
    Ok(Program {
        commands,
        faces,
        points,
        lines,
        texts,
        submodels,
        colours,
        polygons,
        thrusters,
        surfaces,
        beams,
        chains,
        splines,
        labels,
        trailing: 0,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Material {
    pub rgb: [u8; 3],
    pub texture: u16,
    pub unlit: bool,
    pub local_colour: bool,
}

/// F1692 descriptor decoding only, not runtime palette/shading or texture sampling.
pub fn material(value: u16) -> Material {
    let textured = value >> 14 != 0;
    Material {
        rgb: if textured {
            [0; 3]
        } else {
            [
                (value >> 8 & 15) as u8,
                (value >> 4 & 15) as u8,
                (value & 15) as u8,
            ]
        },
        texture: if textured { value & 4095 } else { 0 },
        unlit: value & 0x2000 != 0,
        local_colour: value & 0x1000 != 0,
    }
}

/// F1758 arithmetic without a supplied table. Trig operations report a missing-table error.
pub fn arithmetic(
    code: u16,
    operands: [u8; 2],
    context: &mut VertexContext,
) -> Result<u16, String> {
    arithmetic_with_table(code, operands, context, None)
}

/// Commit the 16-bit result only on success, including table-based sine/cosine.
pub fn arithmetic_with_table(
    code: u16,
    operands: [u8; 2],
    context: &mut VertexContext,
    table: Option<&crate::model_math::SineTable>,
) -> Result<u16, String> {
    if !matches!(code & 31, 13 | 29) {
        return Err("Not a model arithmetic command.".to_owned());
    }
    let a = variable(operands[0], context);
    let b = variable(operands[1], context);
    let value = match code >> 4 & 15 {
        0 => a.wrapping_add(b),
        1 => a.wrapping_sub(b),
        2 => a.wrapping_mul(b),
        3 => {
            if b == 0 {
                a
            } else {
                a.checked_div(b).ok_or("Model signed division overflow.")?
            }
        }
        4 => (a as u32).wrapping_shr(b as u32) as i32,
        5 => a.wrapping_shl(b as u32),
        6 => {
            if b as u32 >= a as u32 {
                b
            } else {
                a
            }
        }
        7 => {
            if b as u32 <= a as u32 {
                b.wrapping_sub(1)
            } else {
                a
            }
        }
        8 => (a as u32)
            .wrapping_shl(16)
            .checked_div(b as u32)
            .ok_or("Model unsigned division by zero.")? as i32,
        9 => a.wrapping_shr(b as u32),
        10 => {
            let index = a.wrapping_add(b);
            i32::from(
                *context
                    .globals
                    .get(index as usize)
                    .ok_or("Model global lookup outside 0..63.")?,
            )
        }
        11 => {
            if b as u32 > a as u32 {
                0
            } else {
                a
            }
        }
        12 => {
            if (b as u32) < a as u32 {
                0
            } else {
                a
            }
        }
        13 | 14 => {
            let table =
                table.ok_or("Model sine/cosine arithmetic needs the original trig table.")?;
            let angle = if code >> 4 & 15 == 14 {
                b.wrapping_add(0x4000)
            } else {
                b
            };
            crate::model::multiply(a, table.sin32(angle))
        }
        _ => a & b,
    } as u16;
    context.locals[usize::from(code >> 8 & 7)] = i32::from(value);
    Ok(value)
}

/// F1753/1756/1757: byte offsets relative to the physical command, or None for a return.
pub fn bit_branch(
    code: u16,
    operands: [u8; 2],
    context: &VertexContext,
) -> Result<Option<i32>, String> {
    let opcode = code & 31;
    if !matches!(opcode, 19 | 20) {
        return Err("Not a model bit-branch command.".to_owned());
    }
    let value = variable(operands[0], context);
    let mask = if operands[1] == 0 {
        -1
    } else {
        1i32.wrapping_shl(u32::from(operands[1] - 1))
    };
    let take = (value & mask == 0) == (opcode == 19);
    Ok(branch_offset(code, take))
}

fn branch_offset(code: u16, take: bool) -> Option<i32> {
    if take && code >> 5 == 0 {
        return None;
    }
    Some(
        4 + if take {
            i32::from((code as i16) >> 5) * 2
        } else {
            0
        },
    )
}

#[derive(Debug, Clone, Copy)]
pub struct VisibilityContext<'a> {
    pub depth: i32,
    pub scale: i32,
    pub detail: [i32; 2],
    pub normals: &'a [crate::model_normals::Normal],
}

/// F1751; callers supply evaluated normals, not lazy legacy buffer pointers.
pub fn visibility(operand: i16, context: &VisibilityContext<'_>) -> Result<bool, String> {
    if operand < 0 {
        return context
            .normals
            .get(usize::from(operand as u16 & 0x7fff))
            .map(|n| n.culled)
            .ok_or_else(|| "Visibility branch references an out-of-range normal.".to_owned());
    }
    if context.depth < 0 {
        return Ok(false);
    }
    let detail = if context.detail[0] & 1 != 0 {
        context.detail[1]
    } else {
        context.detail[0]
    };
    let shift = context.scale.wrapping_add(detail);
    let value = i32::from(operand);
    let threshold = if shift > 0 {
        let shifted = value.wrapping_shl(shift as u32);
        if shifted.wrapping_shr(shift as u32) != value {
            return Ok(false);
        }
        shifted
    } else {
        value
    };
    Ok(threshold < context.depth)
}

/// F1752/F1755 with F1753 return/relative-offset semantics.
pub fn visibility_branch(
    code: u16,
    operand: i16,
    context: &VisibilityContext<'_>,
) -> Result<Option<i32>, String> {
    let opcode = code & 31;
    if !matches!(opcode, 11 | 12) {
        return Err("Not a model visibility-branch command.".to_owned());
    }
    let take = visibility(operand, context)? == (opcode == 11);
    Ok(branch_offset(code, take))
}
