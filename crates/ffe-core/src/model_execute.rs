//! Bounded command control flow and operand emissions, not geometry evaluation or rasterization.
//!
//! `execute` runs without live vertex state and keeps the earlier behaviour: a reached
//! depth-order command fails explicitly. `execute_live` composes the verified F1696
//! depth commands and F1780/F1788 group ordering with the live view-space slots in
//! [`crate::model_instance`], and returns a render-ordered face stream in
//! [`Execution::ordered`]. Both variants are transactional: an error leaves the
//! caller's context and vertex cache unchanged.
use crate::model::variable;
use crate::model::VertexContext;
use crate::model_commands::{
    arithmetic_with_table, bit_branch, decode, material, visibility_branch, Colour, Face, Submodel,
    Text, VisibilityContext,
};
use crate::model_depth::{self, Command as DepthCommand};
use crate::model_instance::Vertices;
use crate::model_math::{angle_operand, SineTable};
use crate::model_order::Order;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emission {
    pub face: Face,
    pub context: VertexContext,
}

/// A point-source operand event. This is the original handler's decoded input,
/// not a projected point or a rasterized pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointEmission {
    pub offset: usize,
    pub material: u16,
    /// Raw F1474 packed-angle operand.
    pub encoded_angle: i16,
    /// Resolved angle (`angle_operand`), already verified separately.
    pub angle: i32,
    pub flag: bool,
    pub vertex: u8,
    pub context: VertexContext,
}

/// A line operand event. This is the original handler's decoded input, not a
/// clipped/rasterized line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEmission {
    pub offset: usize,
    pub material: u16,
    pub vertices: [u8; 2],
    pub context: VertexContext,
}

/// A text operand event (F1747). `culled` is the original normal-flag skip; the
/// glyph programs, string expansion and their nested primitives are not executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEmission {
    pub text: Text,
    pub culled: bool,
    pub context: VertexContext,
}

/// A child-model call operand event (F1749/F1750). The child's culling, frame and
/// program are not evaluated, so its nested primitives are absent from `ordered`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmodelEmission {
    pub submodel: Submodel,
    pub context: VertexContext,
}

/// Where an opcode 26 colour was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColourTarget {
    /// Instance colour (+0xf4..+0xf6), kept in [`VertexContext::colour`].
    Instance,
    /// Normal entry 0's colour, seen by later lighting through normal 0.
    NormalZero,
}

/// An executed F1762 colour command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColourEmission {
    pub colour: Colour,
    /// The selected material word.
    pub material: u16,
    pub target: ColourTarget,
    pub rgb: [u8; 3],
}

/// F1692/F1693 colour resolution: nibble RGB (zero when textured), plus the instance
/// colour for local materials, plus the normal's colour unless unlit or textured.
fn resolve_colour(value: u16, instance: [u8; 3], normal: [u8; 3]) -> [u8; 3] {
    let decoded = material(value);
    let mut rgb = decoded.rgb;
    if decoded.local_colour {
        rgb = std::array::from_fn(|i| rgb[i].wrapping_add(instance[i]));
    }
    if !decoded.unlit && decoded.texture == 0 {
        rgb = std::array::from_fn(|i| rgb[i].wrapping_add(normal[i]));
    }
    rgb
}

/// One depth-queue operation in program order: the original F1780 group changes
/// and F1778 primitive allocations. Sorting these is [`crate::model_order::Order`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueEvent {
    /// Close the current group and open one with this key, or none.
    Group(Option<u32>),
    Face(u32),
    Point(u32),
    Line(u32),
}

/// One render-order entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Primitive {
    Face(Emission),
    Point(PointEmission),
    Line(LineEmission),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Execution {
    /// Visited command offsets in execution order.
    pub commands: Vec<usize>,
    /// Face emissions in command order. Kept stable for diagnostics and earlier checks.
    pub emissions: Vec<Emission>,
    /// Point emissions in command order.
    pub points: Vec<PointEmission>,
    /// Line emissions in command order.
    pub lines: Vec<LineEmission>,
    /// Text operand events in command order; never in [`Self::ordered`].
    pub texts: Vec<TextEmission>,
    /// Child-model call operand events in command order; never in [`Self::ordered`].
    pub submodels: Vec<SubmodelEmission>,
    /// Executed colour commands in command order.
    pub colours: Vec<ColourEmission>,
    /// Final normal entry 0 colour if a colour command wrote it during this run.
    pub normal_zero_colour: Option<[u8; 3]>,
    /// Depth-queue operations in program order ([`execute_live`] only), excluding the
    /// top-level close after return.
    pub queue: Vec<QueueEvent>,
    /// All emitted primitives in original render order. Populated only by
    /// [`execute_live`]; empty for [`execute`], which has no live vertex keys.
    pub ordered: Vec<Primitive>,
    /// Effective group/standalone keys aligned with [`Self::ordered`].
    pub ordered_keys: Vec<u32>,
}

/// Instance fields needed to resolve live vertices and depth keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstanceContext {
    /// Instance position at +0x4.
    pub position: [i32; 3],
    /// Instance generation at +0xf0. Zero is invalid.
    pub generation: u32,
    /// Depth key scale at +0x130.
    pub key_scale: i32,
    /// Depth operand scale at +0x148.
    pub operand_scale: i32,
    /// Texture rendering enabled (the original's global at `DATA_008812` is >= 0,
    /// its default). Textured faces then skip the off-screen rejection.
    pub textures: bool,
}

impl Default for InstanceContext {
    fn default() -> Self {
        Self {
            position: [0; 3],
            generation: 1,
            key_scale: 0,
            operand_scale: 0,
            textures: true,
        }
    }
}

struct InnerInput<'a, 'v> {
    bytes: &'a [u8],
    vertex_count: u32,
    visibility: &'v VisibilityContext<'v>,
    table: Option<&'a SineTable>,
    instance: InstanceContext,
    budget: usize,
}

fn selector(value: u8) -> Result<(), String> {
    if value >= 0xd2 {
        return Err(
            "Execution cannot read local slots 18..63, which alias legacy instance fields."
                .to_owned(),
        );
    }
    Ok(())
}

/// Execute without a live vertex cache. A reached depth-order command fails explicitly;
/// use [`execute_live`] for the composed F1696/F1780 path.
pub fn execute(
    bytes: &[u8],
    vertex_count: u32,
    context: &mut VertexContext,
    visibility: &VisibilityContext<'_>,
    table: Option<&SineTable>,
    budget: usize,
) -> Result<Execution, String> {
    execute_inner(
        InnerInput {
            bytes,
            vertex_count,
            visibility,
            table,
            instance: InstanceContext::default(),
            budget,
        },
        None,
        context,
    )
}

/// Execute with a caller-owned live vertex cache. The cache is committed only after a
/// real return, so view-space slots can be reused across invocations by generation.
pub fn execute_live(
    bytes: &[u8],
    vertices: &mut Vertices<'_>,
    context: &mut VertexContext,
    visibility: &VisibilityContext<'_>,
    table: Option<&SineTable>,
    instance: InstanceContext,
    budget: usize,
) -> Result<Execution, String> {
    let vertex_count = u32::try_from(vertices.vertex_count())
        .map_err(|_| "Live vertex count exceeds u32.".to_owned())?;
    execute_inner(
        InnerInput {
            bytes,
            vertex_count,
            visibility,
            table,
            instance,
            budget,
        },
        Some(vertices),
        context,
    )
}

fn view_point(
    vertices: &mut Vertices<'_>,
    index: i8,
    context: &VertexContext,
    instance: &InstanceContext,
) -> Result<[i32; 3], String> {
    Ok(vertices
        .resolve(
            index,
            context,
            instance.position,
            instance.generation,
            false,
        )?
        .view)
}

/// F1708/F1712 face queueing. A culled normal queues nothing. Otherwise every vertex is
/// projected (F1471) in the original order: b, c, (d,) then a. A textured face whose
/// screen points are all within the guard range and whose screen perimeter is at
/// least 6 is queued; any other face is queued unless every point is off screen and
/// the F1713 outcodes put them all left, all right (or above/below) or all behind
/// the near plane. The key is always
/// F1777 of vertex a's view.
fn face_queue_key(
    face: &Face,
    live: &mut Vertices<'_>,
    context: &VertexContext,
    instance: &InstanceContext,
    visibility: &VisibilityContext<'_>,
) -> Result<Option<u32>, String> {
    let culled = visibility
        .normals
        .get(usize::from(face.normal))
        .ok_or_else(|| {
            format!(
                "Face at {:#x} references an out-of-range normal.",
                face.offset
            )
        })?
        .culled;
    if culled {
        return Ok(None);
    }
    let count = face.vertices.len();
    let mut points = vec![crate::model_instance::Vertex::default(); count];
    for slot in (1..count).chain(std::iter::once(0)) {
        points[slot] = live.resolve(
            face.vertices[slot] as i8,
            context,
            instance.position,
            instance.generation,
            true,
        )?;
    }
    let screens: Vec<[i32; 2]> = points
        .iter()
        .map(|p| [i32::from(p.screen[0]), i32::from(p.screen[1])])
        .collect();
    let decoded = material(face.material);
    let guarded = screens
        .iter()
        .all(|[x, y]| (-2000..=2320).contains(x) && (-2000..=2158).contains(y));
    let perimeter: i32 = (0..count)
        .map(|i| {
            let [ax, ay] = screens[i];
            let [bx, by] = screens[(i + 1) % count];
            (ax - bx).abs() + (ay - by).abs()
        })
        .sum();
    let key = model_depth::key(points[0].view, instance.key_scale);
    if instance.textures && decoded.texture != 0 && guarded && perimeter >= 6 {
        return Ok(Some(key));
    }
    let on_screen = |[x, y]: [i32; 2]| (x as u32) < 320 && (y as u32) < 158;
    if screens.iter().any(|s| on_screen(*s)) {
        return Ok(Some(key));
    }
    // F1715/F1716 outcodes over a, b, c and (for triangles, c again) d.
    let code = |value: i32, limit: i32| match value {
        -32766 => 8,
        v if (0..limit).contains(&v) => 2,
        v if v < 0 => 1,
        _ => 4,
    };
    let rejected = |axis: usize, limit: i32| {
        let combined = screens.iter().fold(0, |acc, s| acc | code(s[axis], limit));
        // DATA_007818 rejects all-left (1), all-right (4) and all-behind (8).
        matches!(combined, 1 | 4 | 8)
    };
    Ok((!rejected(0, 320) && !rejected(1, 158)).then_some(key))
}

fn resolve_depth_key(
    depth: &DepthCommand,
    vertices: &mut Vertices<'_>,
    context: &VertexContext,
    instance: &InstanceContext,
) -> Result<Option<u32>, String> {
    Ok(match depth {
        DepthCommand::Close => None,
        DepthCommand::First => Some(u32::MAX),
        DepthCommand::Position => Some(model_depth::key(instance.position, instance.key_scale)),
        DepthCommand::Vertex(index) => Some(model_depth::key(
            view_point(vertices, *index, context, instance)?,
            instance.key_scale,
        )),
        DepthCommand::Maximum(indices) | DepthCommand::Minimum(indices) => {
            let mut items = indices.iter();
            let first = *items
                .next()
                .ok_or_else(|| "Empty depth-order vertex list.".to_owned())?;
            let mut key = model_depth::key(
                view_point(vertices, first, context, instance)?,
                instance.key_scale,
            ) as i32;
            for index in items {
                let candidate = model_depth::key(
                    view_point(vertices, *index, context, instance)?,
                    instance.key_scale,
                ) as i32;
                key = if matches!(depth, DepthCommand::Maximum(_)) {
                    key.max(candidate)
                } else {
                    key.min(candidate)
                };
            }
            Some(key as u32)
        }
        DepthCommand::Offset { vertex, amount } => {
            let mut point = match vertex {
                Some(index) => view_point(vertices, *index, context, instance)?,
                None => [0; 3],
            };
            point[2] = point[2]
                .wrapping_add(i32::from(*amount).wrapping_shl(instance.operand_scale as u32));
            Some(model_depth::key(point, instance.key_scale))
        }
    })
}

fn execute_inner(
    input: InnerInput<'_, '_>,
    vertices: Option<&mut Vertices<'_>>,
    context: &mut VertexContext,
) -> Result<Execution, String> {
    let InnerInput {
        bytes,
        vertex_count,
        visibility,
        table,
        instance,
        budget,
    } = input;
    if !(1..=4096).contains(&budget) {
        return Err("Model execution budget must be 1..=4096 commands.".to_owned());
    }
    let program = decode(bytes, vertex_count)?;
    let mut working = context.clone();
    let mut working_vertices = match vertices.as_deref() {
        Some(value) => {
            if value.vertex_count() != vertex_count as usize {
                return Err("Live vertex cache does not match the model vertex count.".to_owned());
            }
            Some(value.clone())
        }
        None => None,
    };
    // The command budget bounds execution; the order queue keeps its own 1..=4096 guard.
    let mut order = if working_vertices.is_some() {
        Some(Order::new(4096)?)
    } else {
        None
    };
    let mut result = Execution {
        commands: Vec::new(),
        emissions: Vec::new(),
        points: Vec::new(),
        lines: Vec::new(),
        texts: Vec::new(),
        submodels: Vec::new(),
        colours: Vec::new(),
        normal_zero_colour: None,
        queue: Vec::new(),
        ordered: Vec::new(),
        ordered_keys: Vec::new(),
    };
    let mut offset = 0usize;
    let mut returned = false;
    for _ in 0..budget {
        let index = program
            .commands
            .binary_search_by_key(&offset, |c| c.offset)
            .map_err(|_| format!("Execution left a command boundary at {offset:#x}."))?;
        let command = &program.commands[index];
        result.commands.push(offset);
        let code = command.word;
        let payload = &bytes[offset + 2..offset + command.size];
        let relative = match code & 31 {
            0 => {
                returned = true;
                None
            }
            1 => {
                for point in program.points.iter().filter(|p| p.offset == offset) {
                    let emission = PointEmission {
                        offset: point.offset,
                        material: point.material,
                        encoded_angle: point.angle,
                        angle: angle_operand(point.angle, &working),
                        flag: point.flag,
                        vertex: point.vertex,
                        context: working.clone(),
                    };
                    result.points.push(emission.clone());
                    if let (Some(queue), Some(live)) = (order.as_mut(), working_vertices.as_mut()) {
                        let vertex = live.resolve(
                            point.vertex as i8,
                            &working,
                            instance.position,
                            instance.generation,
                            false,
                        )?;
                        let key = model_depth::key(vertex.view, instance.key_scale);
                        queue.push(key, Primitive::Point(emission))?;
                        result.queue.push(QueueEvent::Point(key));
                    }
                }
                Some(command.size as i32)
            }
            2 => {
                for line in program.lines.iter().filter(|l| l.offset == offset) {
                    let emission = LineEmission {
                        offset: line.offset,
                        material: line.material,
                        vertices: line.vertices,
                        context: working.clone(),
                    };
                    result.lines.push(emission.clone());
                    if let (Some(queue), Some(live)) = (order.as_mut(), working_vertices.as_mut()) {
                        let vertex = live.resolve(
                            line.vertices[0] as i8,
                            &working,
                            instance.position,
                            instance.generation,
                            false,
                        )?;
                        let key = model_depth::key(vertex.view, instance.key_scale);
                        queue.push(key, Primitive::Line(emission))?;
                        result.queue.push(QueueEvent::Line(key));
                    }
                }
                Some(command.size as i32)
            }
            3 | 4 | 7 | 8 => {
                for face in program.faces.iter().filter(|f| f.offset == offset) {
                    let emission = Emission {
                        face: face.clone(),
                        context: working.clone(),
                    };
                    result.emissions.push(emission.clone());
                    if let (Some(queue), Some(live)) = (order.as_mut(), working_vertices.as_mut()) {
                        if let Some(key) =
                            face_queue_key(face, live, &working, &instance, visibility)?
                        {
                            queue.push(key, Primitive::Face(emission))?;
                            result.queue.push(QueueEvent::Face(key));
                        }
                    }
                }
                Some(command.size as i32)
            }
            10 => {
                for text in program.texts.iter().filter(|t| t.offset == offset) {
                    let culled = visibility
                        .normals
                        .get(usize::from(text.normal))
                        .ok_or_else(|| {
                            format!("Text at {offset:#x} references an out-of-range normal.")
                        })?
                        .culled;
                    if let (false, Some(live)) = (culled, working_vertices.as_mut()) {
                        live.resolve(
                            text.vertex as i8,
                            &working,
                            instance.position,
                            instance.generation,
                            false,
                        )?;
                    }
                    result.texts.push(TextEmission {
                        text: text.clone(),
                        culled,
                        context: working.clone(),
                    });
                }
                Some(command.size as i32)
            }
            14 => {
                for submodel in program.submodels.iter().filter(|s| s.offset == offset) {
                    if let Some(live) = working_vertices.as_mut() {
                        live.resolve(
                            submodel.vertex as i8,
                            &working,
                            instance.position,
                            instance.generation,
                            true,
                        )?;
                    }
                    result.submodels.push(SubmodelEmission {
                        submodel: submodel.clone(),
                        context: working.clone(),
                    });
                }
                Some(command.size as i32)
            }
            26 => {
                for colour in program.colours.iter().filter(|c| c.offset == offset) {
                    let index = if colour.selector == 0 {
                        0
                    } else {
                        selector(colour.selector)?;
                        (variable(colour.selector, &working) & 7) as usize
                    };
                    let value = colour
                        .materials
                        .get(index)
                        .copied()
                        .unwrap_or(colour.materials[0]);
                    let lighting = match (colour.normal, result.normal_zero_colour) {
                        (0, Some(rgb)) => rgb,
                        _ => {
                            visibility
                                .normals
                                .get(usize::from(colour.normal))
                                .ok_or_else(|| {
                                    format!(
                                        "Colour at {offset:#x} references an out-of-range normal."
                                    )
                                })?
                                .colour
                        }
                    };
                    let rgb = resolve_colour(value, working.colour, lighting);
                    let target = if colour.to_normal_zero {
                        if material(value).texture != 0 {
                            return Err(format!(
                                "Colour at {offset:#x} needs the runtime texture colour table."
                            ));
                        }
                        result.normal_zero_colour = Some(rgb);
                        ColourTarget::NormalZero
                    } else {
                        working.colour = rgb;
                        ColourTarget::Instance
                    };
                    result.colours.push(ColourEmission {
                        colour: colour.clone(),
                        material: value,
                        target,
                        rgb,
                    });
                }
                Some(command.size as i32)
            }
            6 => {
                let Some(live) = working_vertices.as_mut() else {
                    return Err(format!(
                        "Unsupported executed command {code:#06x} at {offset:#x}."
                    ));
                };
                let (depth, consumed) = model_depth::decode(&bytes[offset..])?;
                if consumed != command.size {
                    return Err(format!(
                        "Depth command at {offset:#x} consumed {consumed} bytes, expected {}.",
                        command.size
                    ));
                }
                let key = resolve_depth_key(&depth, live, &working, &instance)?;
                order
                    .as_mut()
                    .ok_or_else(|| "Depth execution requires an order queue.".to_owned())?
                    .group(key)?;
                result.queue.push(QueueEvent::Group(key));
                Some(command.size as i32)
            }
            13 | 29 => {
                selector(payload[0])?;
                selector(payload[1])?;
                arithmetic_with_table(code, [payload[0], payload[1]], &mut working, table)?;
                Some(command.size as i32)
            }
            19 | 20 => {
                selector(payload[0])?;
                let branch = bit_branch(code, [payload[0], payload[1]], &working)?;
                if branch.is_none() {
                    returned = true;
                }
                branch
            }
            11 | 12 => {
                let branch = visibility_branch(
                    code,
                    i16::from_le_bytes([payload[0], payload[1]]),
                    visibility,
                )?;
                if branch.is_none() {
                    returned = true;
                }
                branch
            }
            28 => {
                let table =
                    table.ok_or("Model rotation execution needs the original sine table.")?;
                if payload[0] & 0xc0 == 0xc0 {
                    selector(payload[0])?;
                }
                table
                    .rotate_command(
                        &mut working,
                        code,
                        i16::from_le_bytes([payload[0], payload[1]]),
                    )
                    .map_err(|error| format!("{error} ({code:#06x} at {offset:#x})"))?;
                Some(command.size as i32)
            }
            _ => {
                return Err(format!(
                    "Unsupported executed command {code:#06x} at {offset:#x}."
                ))
            }
        };
        let Some(relative) = relative else {
            break;
        };
        offset = offset
            .checked_add_signed(relative as isize)
            .ok_or_else(|| format!("Execution branch leaves the program at {offset:#x}."))?;
    }
    if !returned {
        return Err(format!(
            "Model execution exceeded its {budget}-command budget; context unchanged."
        ));
    }
    // Finalize before committing. Like the top-level object-draw callers (F1683/F1684
    // call F1780(0,0) after the model returns), close a group the program left open;
    // nested child/glyph runs, which do not close it, are not executed here.
    let (ordered, ordered_keys) = match order {
        Some(mut order) => {
            order.group(None)?;
            let pairs = order.finish_with_keys()?;
            let mut ordered = Vec::with_capacity(pairs.len());
            let mut ordered_keys = Vec::with_capacity(pairs.len());
            for (key, primitive) in pairs {
                ordered.push(primitive);
                ordered_keys.push(key);
            }
            (ordered, ordered_keys)
        }
        None => (Vec::new(), Vec::new()),
    };
    *context = working;
    if let (Some(source), Some(destination)) = (working_vertices, vertices) {
        *destination = source;
    }
    result.ordered = ordered;
    result.ordered_keys = ordered_keys;
    Ok(result)
}
