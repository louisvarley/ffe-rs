//! A reader for DirectX `.X` model files, as the FFED3D replacement models ship them
//! (`docs/notes/ffed3d-models.md`). It accepts the four encodings the format defines
//! (`txt `, `bin `, and their MSZIP-compressed forms `tzip`/`bzip`, with 32-bit floats) and
//! flattens the frame hierarchy into world-space triangles with normals, texture coordinates
//! and a material each. Animation sets are skipped for now.
//!
//! The files are the player's own local download; nothing here embeds or ships them.

use std::collections::HashMap;

/// The largest decompressed file accepted (the biggest FFED3D model is about 4 MB).
const MAX_SIZE: usize = 64 << 20;

/// One material: its diffuse colour (RGBA, 0..1) and texture file name, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub colour: [f32; 4],
    pub texture: Option<String>,
}

/// One triangle in the model's own space (the frames' transforms applied).
#[derive(Debug, Clone, PartialEq)]
pub struct Triangle {
    pub positions: [[f32; 3]; 3],
    /// Per-corner normals, or `None` when the mesh has none.
    pub normals: Option<[[f32; 3]; 3]>,
    /// Per-corner texture coordinates, or `None`.
    pub uvs: Option<[[f32; 2]; 3]>,
    /// Index into [`Model::materials`], or `None` for an unmaterialled face.
    pub material: Option<usize>,
    /// The frame the mesh hangs from, for later animation.
    pub frame: String,
}

/// A whole model: its triangles and materials.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    pub triangles: Vec<Triangle>,
    pub materials: Vec<Material>,
}

impl Model {
    /// The axis-aligned bounds of every corner, `None` for an empty model.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut corners = self.triangles.iter().flat_map(|t| t.positions);
        let first = corners.next()?;
        Some(corners.fold((first, first), |(lo, hi), p| {
            (
                std::array::from_fn(|i| lo[i].min(p[i])),
                std::array::from_fn(|i| hi[i].max(p[i])),
            )
        }))
    }
}

/// Read a `.X` file from its bytes.
pub fn read(bytes: &[u8]) -> Result<Model, String> {
    let (binary, body) = decode(bytes)?;
    let tokens = if binary {
        binary_tokens(&body)?
    } else {
        text_tokens(&body)?
    };
    let objects = Parser {
        tokens: &tokens,
        at: 0,
    }
    .objects()?;
    build(&objects)
}

/// The header check and the MSZIP decompression: whether the body is binary, and the body.
fn decode(bytes: &[u8]) -> Result<(bool, Vec<u8>), String> {
    if bytes.len() < 16 || &bytes[..4] != b"xof " {
        return Err("not a DirectX .X file (no `xof ` header)".to_owned());
    }
    if &bytes[12..16] != b"0032" {
        return Err("only 32-bit-float .X files are supported".to_owned());
    }
    match &bytes[8..12] {
        b"txt " => Ok((false, bytes[16..].to_vec())),
        b"bin " => Ok((true, bytes[16..].to_vec())),
        b"tzip" => Ok((false, mszip(&bytes[16..])?)),
        b"bzip" => Ok((true, mszip(&bytes[16..])?)),
        other => Err(format!(
            "unknown .X format {:?}",
            String::from_utf8_lossy(other)
        )),
    }
}

/// MSZIP: a total size, then blocks of (uncompressed size, compressed size, `CK`, deflate),
/// each inflated with the previous block's data as its history.
fn mszip(data: &[u8]) -> Result<Vec<u8>, String> {
    use miniz_oxide::inflate::core::{decompress, DecompressorOxide};
    use miniz_oxide::inflate::TINFLStatus;
    let word = |at: usize| -> Result<usize, String> {
        data.get(at..at + 2)
            .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
            .ok_or_else(|| "truncated MSZIP block header".to_owned())
    };
    let total = data
        .get(..4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .ok_or("truncated MSZIP header")?
        .saturating_sub(16);
    if total > MAX_SIZE {
        return Err("compressed .X file is too large".to_owned());
    }
    // A 32 KB wrapping window: each block decodes into it after the last, so a back
    // reference into the previous block finds its bytes.
    const WINDOW: usize = 1 << 15;
    let mut window = vec![0u8; WINDOW];
    let mut position = 0usize;
    let mut out = Vec::with_capacity(total);
    let mut at = 4;
    while at < data.len() && out.len() < total {
        let size = word(at)?;
        let packed = word(at + 2)?;
        let block = data
            .get(at + 4..at + 4 + packed)
            .ok_or("truncated MSZIP block")?;
        at += 4 + packed;
        if block.get(..2) != Some(b"CK") {
            return Err("MSZIP block without its CK signature".to_owned());
        }
        let mut state = DecompressorOxide::new();
        let mut input = &block[2..];
        let mut written = 0usize;
        loop {
            let (status, read, wrote) = decompress(&mut state, input, &mut window, position, 0);
            for k in 0..wrote {
                out.push(window[(position + k) % WINDOW]);
            }
            position = (position + wrote) % WINDOW;
            written += wrote;
            input = &input[read..];
            match status {
                TINFLStatus::Done => break,
                TINFLStatus::HasMoreOutput if wrote > 0 => continue,
                other => return Err(format!("MSZIP inflate failed: {other:?}")),
            }
        }
        if written != size {
            return Err(format!(
                "MSZIP block inflated to {written} bytes, not {size}"
            ));
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    Str(String),
    Ints(Vec<u32>),
    Floats(Vec<f32>),
    Open,
    Close,
    /// `template` (binary token 31, or the word in text): a template definition follows.
    Template,
    Other,
}

fn binary_tokens(body: &[u8]) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut at = 0usize;
    let take = |at: &mut usize, n: usize| -> Result<&[u8], String> {
        let slice = body
            .get(*at..*at + n)
            .ok_or_else(|| "truncated binary .X token".to_owned())?;
        *at += n;
        Ok(slice)
    };
    let dword = |at: &mut usize| -> Result<u32, String> {
        take(at, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    while at + 2 <= body.len() {
        let kind = u16::from_le_bytes([body[at], body[at + 1]]);
        at += 2;
        match kind {
            1 => {
                let n = dword(&mut at)? as usize;
                tokens.push(Token::Name(
                    String::from_utf8_lossy(take(&mut at, n)?).into_owned(),
                ));
            }
            2 => {
                let n = dword(&mut at)? as usize;
                let text = String::from_utf8_lossy(take(&mut at, n)?).into_owned();
                take(&mut at, 2)?; // the terminating token (`;` or `,`)
                tokens.push(Token::Str(text));
            }
            3 => tokens.push(Token::Ints(vec![dword(&mut at)?])),
            5 => {
                take(&mut at, 16)?;
                tokens.push(Token::Other);
            }
            6 => {
                let n = dword(&mut at)? as usize;
                if n > MAX_SIZE / 4 {
                    return Err("binary .X integer list too long".to_owned());
                }
                let mut list = Vec::with_capacity(n);
                for _ in 0..n {
                    list.push(dword(&mut at)?);
                }
                tokens.push(Token::Ints(list));
            }
            7 => {
                let n = dword(&mut at)? as usize;
                if n > MAX_SIZE / 4 {
                    return Err("binary .X float list too long".to_owned());
                }
                let mut list = Vec::with_capacity(n);
                for _ in 0..n {
                    list.push(f32::from_bits(dword(&mut at)?));
                }
                tokens.push(Token::Floats(list));
            }
            10 => tokens.push(Token::Open),
            11 => tokens.push(Token::Close),
            31 => tokens.push(Token::Template),
            12..=20 | 40..=53 => tokens.push(Token::Other),
            other => return Err(format!("unknown binary .X token {other} at {at}")),
        }
    }
    Ok(tokens)
}

fn text_tokens(body: &[u8]) -> Result<Vec<Token>, String> {
    let text = String::from_utf8_lossy(body);
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == ',' || c == ';' {
            i += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') || c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '{' {
            tokens.push(Token::Open);
            i += 1;
        } else if c == '}' {
            tokens.push(Token::Close);
            i += 1;
        } else if c == '"' {
            let start = i + 1;
            i = start;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            tokens.push(Token::Str(
                chars[start..i.min(chars.len())].iter().collect(),
            ));
            i += 1;
        } else if c == '<' {
            while i < chars.len() && chars[i] != '>' {
                i += 1;
            }
            tokens.push(Token::Other);
            i += 1;
        } else if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '-' | '+' | '.'))
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if word.chars().all(|c| c == '.') {
                // An open template's `[...]`.
                tokens.push(Token::Other);
            } else if word.contains(['.', 'e', 'E']) {
                let value = word
                    .parse::<f32>()
                    .map_err(|_| format!("bad number {word}"))?;
                tokens.push(Token::Floats(vec![value]));
            } else {
                let value = word
                    .parse::<i64>()
                    .map_err(|_| format!("bad number {word}"))?;
                tokens.push(Token::Ints(vec![value as u32]));
            }
        } else if c.is_alphanumeric() || c == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '-' | '.'))
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            tokens.push(if word == "template" {
                Token::Template
            } else {
                Token::Name(word)
            });
        } else {
            tokens.push(Token::Other);
            i += 1;
        }
    }
    Ok(tokens)
}

/// A data object: its template name, optional instance name, the numbers and strings in its
/// body in order, its children and the names it references (`{ Name }`).
#[derive(Debug, Clone, Default)]
struct Object {
    kind: String,
    name: Option<String>,
    numbers: Vec<Number>,
    strings: Vec<String>,
    children: Vec<Object>,
    references: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
enum Number {
    Int(u32),
    Float(f32),
}

impl Number {
    fn int(self) -> u32 {
        match self {
            Number::Int(v) => v,
            Number::Float(v) => v as u32,
        }
    }

    fn float(self) -> f32 {
        match self {
            Number::Int(v) => v as f32,
            Number::Float(v) => v,
        }
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
}

impl Parser<'_> {
    fn objects(&mut self) -> Result<Vec<Object>, String> {
        let mut list = Vec::new();
        while self.at < self.tokens.len() {
            match &self.tokens[self.at] {
                Token::Template => self.skip_template()?,
                Token::Name(_) => list.push(self.object(0)?),
                _ => self.at += 1,
            }
        }
        Ok(list)
    }

    fn skip_template(&mut self) -> Result<(), String> {
        while self.at < self.tokens.len() && self.tokens[self.at] != Token::Open {
            self.at += 1;
        }
        self.skip_braces()
    }

    fn skip_braces(&mut self) -> Result<(), String> {
        let mut depth = 0usize;
        while self.at < self.tokens.len() {
            match self.tokens[self.at] {
                Token::Open => depth += 1,
                Token::Close => {
                    depth -= 1;
                    if depth == 0 {
                        self.at += 1;
                        return Ok(());
                    }
                }
                _ => {}
            }
            self.at += 1;
        }
        Err("unbalanced braces in .X file".to_owned())
    }

    /// `Kind [Name] { ... }` with `self.at` on `Kind`.
    fn object(&mut self, depth: usize) -> Result<Object, String> {
        if depth > 64 {
            return Err(".X objects nested too deeply".to_owned());
        }
        let Token::Name(kind) = &self.tokens[self.at] else {
            return Err("expected an object name".to_owned());
        };
        let mut object = Object {
            kind: kind.clone(),
            ..Object::default()
        };
        self.at += 1;
        if let Some(Token::Name(name)) = self.tokens.get(self.at) {
            object.name = Some(name.clone());
            self.at += 1;
        }
        while matches!(self.tokens.get(self.at), Some(Token::Other)) {
            self.at += 1;
        }
        if self.tokens.get(self.at) != Some(&Token::Open) {
            return Err(format!("expected `{{` after {}", object.kind));
        }
        self.at += 1;
        loop {
            match self.tokens.get(self.at) {
                None => return Err(format!("unterminated {} object", object.kind)),
                Some(Token::Close) => {
                    self.at += 1;
                    return Ok(object);
                }
                Some(Token::Ints(list)) => {
                    object.numbers.extend(list.iter().map(|v| Number::Int(*v)));
                    self.at += 1;
                }
                Some(Token::Floats(list)) => {
                    object
                        .numbers
                        .extend(list.iter().map(|v| Number::Float(*v)));
                    self.at += 1;
                }
                Some(Token::Str(text)) => {
                    object.strings.push(text.clone());
                    self.at += 1;
                }
                Some(Token::Open) => {
                    // A reference: `{ Name }` (an optional GUID after it).
                    self.at += 1;
                    if let Some(Token::Name(name)) = self.tokens.get(self.at) {
                        object.references.push(name.clone());
                    }
                    while !matches!(self.tokens.get(self.at), Some(Token::Close) | None) {
                        self.at += 1;
                    }
                    self.at += 1;
                }
                Some(Token::Name(_)) => {
                    // A child object, unless it is a bare reference word in a list.
                    if matches!(
                        (self.tokens.get(self.at + 1), self.tokens.get(self.at + 2)),
                        (Some(Token::Open), _) | (Some(Token::Name(_)), Some(Token::Open))
                    ) {
                        let child = self.object(depth + 1)?;
                        object.children.push(child);
                    } else {
                        self.at += 1;
                    }
                }
                Some(Token::Other) => self.at += 1,
                Some(Token::Template) => self.skip_template()?,
            }
        }
    }
}

type Matrix = [f32; 16];

const IDENTITY: Matrix = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// Row-vector convention (Direct3D): `point * local * parent`.
fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|i| {
        let (row, col) = (i / 4, i % 4);
        (0..4).map(|k| a[row * 4 + k] * b[k * 4 + col]).sum()
    })
}

fn transform(m: &Matrix, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|c| p[0] * m[c] + p[1] * m[4 + c] + p[2] * m[8 + c] + m[12 + c])
}

fn rotate(m: &Matrix, v: [f32; 3]) -> [f32; 3] {
    let r: [f32; 3] = std::array::from_fn(|c| v[0] * m[c] + v[1] * m[4 + c] + v[2] * m[8 + c]);
    let length = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
    if length > 0.0 {
        r.map(|x| x / length)
    } else {
        r
    }
}

struct Builder {
    model: Model,
    named: HashMap<String, usize>,
}

fn build(objects: &[Object]) -> Result<Model, String> {
    let mut builder = Builder {
        model: Model::default(),
        named: HashMap::new(),
    };
    // Top-level materials first, so meshes can reference them by name.
    for object in objects.iter().filter(|o| o.kind == "Material") {
        builder.material(object);
    }
    for object in objects {
        match object.kind.as_str() {
            "Frame" => builder.frame(object, &IDENTITY, "")?,
            "Mesh" => builder.mesh(object, &IDENTITY, "")?,
            _ => {}
        }
    }
    Ok(builder.model)
}

impl Builder {
    fn material(&mut self, object: &Object) -> usize {
        let n = &object.numbers;
        let colour = if n.len() >= 4 {
            [n[0].float(), n[1].float(), n[2].float(), n[3].float()]
        } else {
            [1.0; 4]
        };
        let texture = object
            .children
            .iter()
            .find(|c| c.kind == "TextureFilename" || c.kind == "TextureFileName")
            .and_then(|c| c.strings.first().cloned());
        self.model.materials.push(Material { colour, texture });
        let index = self.model.materials.len() - 1;
        if let Some(name) = &object.name {
            self.named.insert(name.clone(), index);
        }
        index
    }

    fn frame(&mut self, object: &Object, parent: &Matrix, _: &str) -> Result<(), String> {
        let local = object
            .children
            .iter()
            .find(|c| c.kind == "FrameTransformMatrix")
            .filter(|c| c.numbers.len() >= 16)
            .map(|c| std::array::from_fn(|i| c.numbers[i].float()))
            .unwrap_or(IDENTITY);
        let world = multiply(&local, parent);
        let name = object.name.clone().unwrap_or_default();
        for child in &object.children {
            match child.kind.as_str() {
                "Frame" => self.frame(child, &world, &name)?,
                "Mesh" => self.mesh(child, &world, &name)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn mesh(&mut self, object: &Object, matrix: &Matrix, frame: &str) -> Result<(), String> {
        let n = &object.numbers;
        let mut at = 0usize;
        let mut next = |what: &str| -> Result<Number, String> {
            let value = n
                .get(at)
                .copied()
                .ok_or_else(|| format!("mesh {what} truncated"))?;
            at += 1;
            Ok(value)
        };
        let count = next("vertex count")?.int() as usize;
        if count > MAX_SIZE / 12 {
            return Err("mesh vertex count too large".to_owned());
        }
        let mut vertices = Vec::with_capacity(count);
        for _ in 0..count {
            let p = [
                next("vertex")?.float(),
                next("vertex")?.float(),
                next("vertex")?.float(),
            ];
            vertices.push(transform(matrix, p));
        }
        let face_list = faces(&mut next)?;
        // Normals and their own face list, texture coordinates per vertex.
        let normals = object
            .children
            .iter()
            .find(|c| c.kind == "MeshNormals")
            .and_then(|c| {
                let mut at = 0usize;
                let mut next = |_: &str| -> Result<Number, String> {
                    let v = c.numbers.get(at).copied().ok_or("normals truncated")?;
                    at += 1;
                    Ok(v)
                };
                let count = next("").ok()?.int() as usize;
                let mut list = Vec::with_capacity(count.min(1 << 20));
                for _ in 0..count {
                    let v = [
                        next("").ok()?.float(),
                        next("").ok()?.float(),
                        next("").ok()?.float(),
                    ];
                    list.push(rotate(matrix, v));
                }
                let indices = faces(&mut next).ok()?;
                Some((list, indices))
            });
        let uvs = object
            .children
            .iter()
            .find(|c| c.kind == "MeshTextureCoords")
            .map(|c| {
                c.numbers
                    .get(1..)
                    .unwrap_or(&[])
                    .chunks_exact(2)
                    .map(|p| [p[0].float(), p[1].float()])
                    .collect::<Vec<_>>()
            });
        let face_materials = object
            .children
            .iter()
            .find(|c| c.kind == "MeshMaterialList")
            .map(|c| self.material_list(c));
        for (index, face) in face_list.iter().enumerate() {
            let normal_face = normals.as_ref().and_then(|(_, indices)| indices.get(index));
            let material = face_materials
                .as_ref()
                .and_then(|list| list.get(index.min(list.len().saturating_sub(1))).copied())
                .flatten();
            // Fan triangulation of each face.
            for k in 1..face.len().saturating_sub(1) {
                let corners = [0, k, k + 1];
                let position = |c: usize| vertices.get(face[c] as usize).copied();
                let (Some(a), Some(b), Some(d)) = (
                    position(corners[0]),
                    position(corners[1]),
                    position(corners[2]),
                ) else {
                    return Err("mesh face references a missing vertex".to_owned());
                };
                let normals = normal_face.and_then(|nf| {
                    let list = &normals.as_ref()?.0;
                    Some([
                        *list.get(*nf.get(corners[0])? as usize)?,
                        *list.get(*nf.get(corners[1])? as usize)?,
                        *list.get(*nf.get(corners[2])? as usize)?,
                    ])
                });
                let uvs = uvs.as_ref().and_then(|list| {
                    Some([
                        *list.get(face[corners[0]] as usize)?,
                        *list.get(face[corners[1]] as usize)?,
                        *list.get(face[corners[2]] as usize)?,
                    ])
                });
                self.model.triangles.push(Triangle {
                    positions: [a, b, d],
                    normals,
                    uvs,
                    material,
                    frame: frame.to_owned(),
                });
            }
        }
        Ok(())
    }

    /// `MeshMaterialList`: material count, face count, a material index per face, then the
    /// materials themselves (inline objects or `{ Name }` references).
    fn material_list(&mut self, object: &Object) -> Vec<Option<usize>> {
        let mut local = Vec::new();
        for child in object.children.iter().filter(|c| c.kind == "Material") {
            local.push(self.material(child));
        }
        for name in &object.references {
            if let Some(index) = self.named.get(name) {
                local.push(*index);
            }
        }
        let n = &object.numbers;
        let faces = n.get(1).map_or(0, |v| v.int() as usize);
        (0..faces)
            .map(|i| {
                n.get(2 + i)
                    .and_then(|v| local.get(v.int() as usize).copied())
            })
            .collect()
    }
}

/// A face list: a count, then each face as its corner count and corner indices.
fn faces(next: &mut dyn FnMut(&str) -> Result<Number, String>) -> Result<Vec<Vec<u32>>, String> {
    let count = next("face count")?.int() as usize;
    if count > MAX_SIZE / 16 {
        return Err("mesh face count too large".to_owned());
    }
    let mut faces = Vec::with_capacity(count);
    for _ in 0..count {
        let corners = next("face")?.int() as usize;
        if corners > 64 {
            return Err("mesh face with too many corners".to_owned());
        }
        let mut face = Vec::with_capacity(corners);
        for _ in 0..corners {
            face.push(next("face")?.int());
        }
        faces.push(face);
    }
    Ok(faces)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "xof 0303txt 0032
template Vector { <3D82AB5E-62DA-11cf-AB39-0020AF71E433> FLOAT x; FLOAT y; FLOAT z; }
Material Red { 1.0;0.0;0.0;1.0;; 5.0; 0.0;0.0;0.0;; 0.0;0.0;0.0;; TextureFilename { \"skin.png\"; } }
Frame Root {
  FrameTransformMatrix { 1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0, 0.0,0.0,1.0,0.0, 10.0,0.0,0.0,1.0;; }
  Mesh Quad {
    4; 0.0;0.0;0.0;, 1.0;0.0;0.0;, 1.0;1.0;0.0;, 0.0;1.0;0.0;;
    1; 4;0,1,2,3;;
    MeshTextureCoords { 4; 0.0;0.0;, 1.0;0.0;, 1.0;1.0;, 0.0;1.0;; }
    MeshMaterialList { 1; 1; 0;; { Red } }
  }
}";

    #[test]
    fn a_text_quad_becomes_two_moved_triangles_with_its_material() {
        let model = read(TEXT.as_bytes()).unwrap();
        assert_eq!(model.triangles.len(), 2);
        assert_eq!(model.materials.len(), 1);
        assert_eq!(model.materials[0].texture.as_deref(), Some("skin.png"));
        let first = &model.triangles[0];
        assert_eq!(first.positions[0], [10.0, 0.0, 0.0]);
        assert_eq!(first.positions[2], [11.0, 1.0, 0.0]);
        assert_eq!(first.uvs.unwrap()[1], [1.0, 0.0]);
        assert_eq!(first.material, Some(0));
        assert_eq!(first.frame, "Root");
        assert_eq!(model.bounds(), Some(([10.0, 0.0, 0.0], [11.0, 1.0, 0.0])));
    }

    /// The same quad in binary tokens.
    fn binary_quad() -> Vec<u8> {
        let mut body = Vec::new();
        let token = |body: &mut Vec<u8>, kind: u16| body.extend(kind.to_le_bytes());
        let name = |body: &mut Vec<u8>, text: &str| {
            body.extend(1u16.to_le_bytes());
            body.extend((text.len() as u32).to_le_bytes());
            body.extend(text.as_bytes());
        };
        let ints = |body: &mut Vec<u8>, list: &[u32]| {
            body.extend(6u16.to_le_bytes());
            body.extend((list.len() as u32).to_le_bytes());
            for v in list {
                body.extend(v.to_le_bytes());
            }
        };
        let floats = |body: &mut Vec<u8>, list: &[f32]| {
            body.extend(7u16.to_le_bytes());
            body.extend((list.len() as u32).to_le_bytes());
            for v in list {
                body.extend(v.to_bits().to_le_bytes());
            }
        };
        name(&mut body, "Mesh");
        token(&mut body, 10);
        ints(&mut body, &[3]);
        floats(&mut body, &[0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0]);
        ints(&mut body, &[1, 3, 0, 1, 2]);
        token(&mut body, 11);
        let mut file = b"xof 0303bin 0032".to_vec();
        file.extend(body);
        file
    }

    #[test]
    fn a_binary_mesh_reads_the_same_way() {
        let model = read(&binary_quad()).unwrap();
        assert_eq!(model.triangles.len(), 1);
        assert_eq!(model.triangles[0].positions[1], [2.0, 0.0, 0.0]);
        assert!(model.triangles[0].material.is_none());
    }

    #[test]
    fn mszip_blocks_inflate_with_the_previous_block_as_history() {
        // Two stored-mode deflate blocks (BFINAL=1, BTYPE=00) wrapped as MSZIP.
        let block = |data: &[u8]| {
            let mut out = vec![b'C', b'K', 1];
            out.extend((data.len() as u16).to_le_bytes());
            out.extend((!(data.len() as u16)).to_le_bytes());
            out.extend(data);
            out
        };
        let parts: [&[u8]; 2] = [b"hello ", b"world"];
        let mut data = (16 + 11u32).to_le_bytes().to_vec();
        for part in parts {
            let packed = block(part);
            data.extend((part.len() as u16).to_le_bytes());
            data.extend((packed.len() as u16).to_le_bytes());
            data.extend(packed);
        }
        assert_eq!(mszip(&data).unwrap(), b"hello world");
    }

    #[test]
    fn bad_headers_and_truncation_fail_clearly() {
        assert!(read(b"not an x file at all").is_err());
        assert!(read(b"xof 0303txt 0064").is_err());
        let mut truncated = binary_quad();
        truncated.truncate(truncated.len() - 6);
        assert!(read(&truncated).is_err());
    }
}
