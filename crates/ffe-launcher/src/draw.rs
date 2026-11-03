//! The launcher's look: the mockup's palette (`assets/theme.json`), fonts,
//! the background artwork and the drawn shapes and icons (the mockup's SVG
//! icons, redrawn from their 24-unit paths).

use eframe::egui::{
    self, epaint::Mesh, pos2, vec2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape,
    Stroke, TextureHandle, Vec2,
};
use std::sync::Arc;

pub const CREAM: Color32 = Color32::from_rgb(0xf6, 0xf5, 0xed);
pub const OLIVE: Color32 = Color32::from_rgb(0x71, 0x80, 0x00);
pub const CYAN: Color32 = Color32::from_rgb(0x00, 0xa9, 0xe6);
pub const VERIFIED: Color32 = Color32::from_rgb(0x8d, 0xff, 0x00);
pub const DIM: Color32 = Color32::from_rgb(170, 178, 160);
/// `panel_border.svg`'s fill (`#07120f` at 88%).
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(6, 16, 13, 224);

pub const REGULAR: &str = "regular";
pub const BOLD: &str = "bold";
pub const ITALIC: &str = "italic";

pub fn font(size: f32, family: &str) -> FontId {
    FontId::new(size, FontFamily::Name(family.into()))
}

/// The system's UI faces when present (Segoe UI on Windows, Helvetica/Arial
/// on macOS, Liberation/DejaVu on Linux); egui's own font otherwise.
pub fn fonts(ctx: &egui::Context) {
    let candidates: [(&str, &[&str]); 3] = [
        (
            REGULAR,
            &[
                "C:\\Windows\\Fonts\\segoeui.ttf",
                "/System/Library/Fonts/Supplemental/Arial.ttf",
                "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ],
        ),
        (
            BOLD,
            &[
                "C:\\Windows\\Fonts\\segoeuib.ttf",
                "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
                "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            ],
        ),
        (
            ITALIC,
            &[
                "C:\\Windows\\Fonts\\segoeuii.ttf",
                "/System/Library/Fonts/Supplemental/Arial Italic.ttf",
                "/usr/share/fonts/truetype/liberation/LiberationSans-Italic.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
            ],
        ),
    ];
    let mut definitions = egui::FontDefinitions::default();
    let fallback = definitions
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (name, paths) in candidates {
        let mut family = Vec::new();
        if let Some(bytes) = paths.iter().find_map(|path| std::fs::read(path).ok()) {
            definitions
                .font_data
                .insert(name.to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
            family.push(name.to_owned());
        }
        family.extend(fallback.iter().cloned());
        definitions
            .families
            .insert(FontFamily::Name(name.into()), family);
    }
    if definitions.font_data.contains_key(REGULAR) {
        if let Some(proportional) = definitions.families.get_mut(&FontFamily::Proportional) {
            proportional.insert(0, REGULAR.to_owned());
        }
    }
    ctx.set_fonts(definitions);
}

pub fn style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(CREAM);
    visuals.panel_fill = Color32::BLACK;
    visuals.window_fill = Color32::from_rgb(7, 18, 15);
    visuals.extreme_bg_color = Color32::from_rgb(4, 12, 10);
    visuals.hyperlink_color = CYAN;
    visuals.selection.bg_fill = Color32::from_rgb(80, 92, 0);
    visuals.selection.stroke = Stroke::new(1.0_f32, CREAM);
    let edge = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(215, 222, 210, 110));
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(16, 30, 24);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(16, 30, 24);
    visuals.widgets.inactive.bg_stroke = edge;
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 52, 10);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 52, 10);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, CREAM);
    visuals.widgets.active.weak_bg_fill = OLIVE;
    visuals.widgets.active.bg_fill = OLIVE;
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(4);
    }
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = vec2(12.0, 8.0);
        style.spacing.button_padding = vec2(16.0, 7.0);
        style.spacing.interact_size.y = 32.0;
        style.spacing.scroll.bar_width = 8.0;
        for (text_style, size) in [
            (egui::TextStyle::Body, 18.0),
            (egui::TextStyle::Button, 18.0),
            (egui::TextStyle::Small, 15.0),
            (egui::TextStyle::Heading, 30.0),
        ] {
            style.text_styles.insert(
                text_style,
                FontId::new(size, FontFamily::Name(REGULAR.into())),
            );
        }
    });
}

/// The cleaned mockup artwork (`assets/background.jpg`: the approved
/// reference with its baked-in buttons, rows and labels removed).
pub fn background(ctx: &egui::Context) -> TextureHandle {
    let bytes = include_bytes!("../assets/background.jpg");
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        .map(|image| image.to_rgba8())
        .ok();
    let colour = match image {
        Some(image) => egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        None => egui::ColorImage::new([1, 1], Color32::from_rgb(7, 16, 12)),
    };
    ctx.load_texture("background", colour, egui::TextureOptions::LINEAR)
}

/// An octagon: `rect` with its corners cut by `cut`.
pub fn chamfer(rect: Rect, cut: f32) -> Vec<Pos2> {
    vec![
        pos2(rect.left() + cut, rect.top()),
        pos2(rect.right() - cut, rect.top()),
        pos2(rect.right(), rect.top() + cut),
        pos2(rect.right(), rect.bottom() - cut),
        pos2(rect.right() - cut, rect.bottom()),
        pos2(rect.left() + cut, rect.bottom()),
        pos2(rect.left(), rect.bottom() - cut),
        pos2(rect.left(), rect.top() + cut),
    ]
}

/// `panel_border.svg`: a chamfered panel with a thin light border.
pub fn chamfer_panel(painter: &Painter, rect: Rect, cut: f32, fill: Color32, stroke: Stroke) {
    painter.add(Shape::convex_polygon(
        chamfer(rect, cut),
        fill,
        Stroke::NONE,
    ));
    painter.add(Shape::closed_line(chamfer(rect, cut), stroke));
}

/// A vertical gradient through `stops` (`(fraction, colour)`) over `rect`.
pub fn vertical_gradient(painter: &Painter, rect: Rect, stops: &[(f32, Color32)]) {
    let mut mesh = Mesh::default();
    for (index, (t, colour)) in stops.iter().enumerate() {
        let y = rect.top() + rect.height() * t;
        mesh.colored_vertex(pos2(rect.left(), y), *colour);
        mesh.colored_vertex(pos2(rect.right(), y), *colour);
        if index > 0 {
            let base = (index as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// A chamfered shape filled with a vertical gradient: a fan of horizontal
/// bands, each clipped to the chamfer.
fn chamfer_gradient(painter: &Painter, rect: Rect, cut: f32, stops: &[(f32, Color32)]) {
    let bands = 48;
    let mut mesh = Mesh::default();
    let colour_at = |t: f32| -> Color32 {
        let mut previous = stops[0];
        for stop in stops {
            if t <= stop.0 {
                let span = (stop.0 - previous.0).max(1e-6);
                return previous
                    .1
                    .lerp_to_gamma(stop.1, ((t - previous.0) / span).clamp(0.0, 1.0));
            }
            previous = *stop;
        }
        previous.1
    };
    let inset = |y: f32| -> f32 {
        let from_top = y - rect.top();
        let from_bottom = rect.bottom() - y;
        (cut - from_top).max(cut - from_bottom).max(0.0)
    };
    for band in 0..=bands {
        let t = band as f32 / bands as f32;
        let y = rect.top() + rect.height() * t;
        let colour = colour_at(t);
        mesh.colored_vertex(pos2(rect.left() + inset(y), y), colour);
        mesh.colored_vertex(pos2(rect.right() - inset(y), y), colour);
        if band > 0 {
            let base = (band as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// `launch_button_clean.svg` with the play mark and the spaced LAUNCH text.
pub fn launch_button(painter: &Painter, rect: Rect, enabled: bool, hovered: bool, pressed: bool) {
    let cut = 24.0;
    let stops: [(f32, Color32); 3] = if !enabled {
        [
            (0.0, Color32::from_rgb(92, 96, 80)),
            (0.45, Color32::from_rgb(56, 60, 48)),
            (1.0, Color32::from_rgb(22, 26, 20)),
        ]
    } else if pressed {
        [
            (0.0, Color32::from_rgb(0x6b, 0x76, 0x06)),
            (0.45, Color32::from_rgb(0x3a, 0x46, 0x00)),
            (1.0, Color32::from_rgb(0x10, 0x18, 0x03)),
        ]
    } else if hovered {
        [
            (0.0, Color32::from_rgb(0xa4, 0xb2, 0x14)),
            (0.45, Color32::from_rgb(0x5c, 0x6c, 0x02)),
            (1.0, Color32::from_rgb(0x1c, 0x2a, 0x06)),
        ]
    } else {
        [
            (0.0, Color32::from_rgb(0x89, 0x95, 0x09)),
            (0.45, Color32::from_rgb(0x4b, 0x5a, 0x00)),
            (1.0, Color32::from_rgb(0x15, 0x20, 0x04)),
        ]
    };
    chamfer_gradient(painter, rect, cut, &stops);
    let edge = if enabled {
        Color32::from_rgb(0xf2, 0xf4, 0xdd)
    } else {
        Color32::from_rgb(140, 142, 130)
    };
    painter.add(Shape::closed_line(
        chamfer(rect, cut),
        Stroke::new(3.0_f32, edge),
    ));
    let text_colour = if enabled {
        CREAM
    } else {
        Color32::from_rgb(170, 172, 160)
    };
    let galley =
        painter.layout_no_wrap("L  A  U  N  C  H".to_owned(), font(54.0, BOLD), text_colour);
    let total = 50.0 + 42.0 + galley.size().x;
    let left = rect.center().x - total / 2.0;
    let mid = rect.center().y;
    painter.add(Shape::convex_polygon(
        vec![
            pos2(left, mid - 25.0),
            pos2(left + 46.0, mid),
            pos2(left, mid + 25.0),
        ],
        text_colour,
        Stroke::NONE,
    ));
    painter.galley(
        pos2(left + 92.0, mid - galley.size().y / 2.0),
        galley,
        text_colour,
    );
}

/// The rows' small chamfered buttons (folder and Browse).
pub fn small_button(painter: &Painter, rect: Rect, hovered: bool) {
    let fill = if hovered {
        Color32::from_rgb(52, 64, 12)
    } else {
        Color32::from_rgba_unmultiplied(4, 10, 8, 230)
    };
    chamfer_panel(painter, rect, 6.0, fill, Stroke::new(1.5_f32, CREAM));
}

pub fn spinner(painter: &Painter, rect: Rect, time: f32, colour: Color32) {
    let centre = rect.center();
    let radius = rect.width() * 0.36;
    let start = time * 5.0;
    let points: Vec<Pos2> = (0..=24)
        .map(|i| {
            let a = start + i as f32 / 24.0 * std::f32::consts::PI * 1.5;
            centre + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    painter.add(Shape::line(points, Stroke::new(2.5_f32, colour)));
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Settings,
    Info,
    Folder,
    Verified,
    Circle,
    Failed,
    Warning,
    Cube,
    Disc,
}

/// Draw `icon` (from the mockup's 24-unit SVG paths) into `rect`.
pub fn icon(painter: &Painter, icon: Icon, rect: Rect, colour: Color32) {
    let scale = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - Vec2::splat(12.0 * scale);
    let p = |x: f32, y: f32| origin + vec2(x, y) * scale;
    let stroke = Stroke::new(1.8 * scale, colour);
    let polyline = |points: &[(f32, f32)], closed: bool| {
        let points: Vec<Pos2> = points.iter().map(|(x, y)| p(*x, *y)).collect();
        if closed {
            Shape::closed_line(points, stroke)
        } else {
            Shape::line(points, stroke)
        }
    };
    match icon {
        Icon::Play => {
            painter.add(Shape::convex_polygon(
                vec![p(7.0, 4.0), p(21.0, 12.0), p(7.0, 20.0)],
                colour,
                Stroke::NONE,
            ));
        }
        Icon::Settings => {
            let gear = [
                (10.0, 2.0),
                (14.0, 2.0),
                (14.6, 4.4),
                (16.6, 5.5),
                (19.0, 4.7),
                (21.0, 8.1),
                (19.2, 9.8),
                (19.2, 12.2),
                (21.0, 13.9),
                (19.0, 17.3),
                (16.6, 16.5),
                (14.6, 17.6),
                (14.0, 22.0),
                (10.0, 22.0),
                (9.4, 19.6),
                (7.4, 18.5),
                (5.0, 19.3),
                (3.0, 15.9),
                (4.8, 14.2),
                (4.8, 11.8),
                (3.0, 10.1),
                (5.0, 6.7),
                (7.4, 7.5),
                (9.4, 6.4),
            ];
            painter.add(Shape::closed_line(
                gear.iter().map(|(x, y)| p(*x, *y)).collect(),
                Stroke::new(1.7 * scale, colour),
            ));
            painter.circle_stroke(p(12.0, 12.0), 3.4 * scale, Stroke::new(1.7 * scale, colour));
        }
        Icon::Info => {
            painter.circle_stroke(p(12.0, 12.0), 9.0 * scale, stroke);
            painter.add(polyline(&[(12.0, 10.5), (12.0, 17.0)], false));
            painter.circle_filled(p(12.0, 7.2), 1.1 * scale, colour);
        }
        Icon::Folder => {
            painter.add(polyline(
                &[
                    (2.5, 6.5),
                    (9.5, 6.5),
                    (11.7, 8.8),
                    (21.5, 8.8),
                    (21.5, 19.5),
                    (2.5, 19.5),
                ],
                true,
            ));
        }
        Icon::Verified => {
            painter.circle_stroke(p(12.0, 12.0), 9.0 * scale, stroke);
            painter.add(polyline(&[(7.5, 12.0), (10.6, 15.1), (16.6, 8.7)], false));
        }
        Icon::Circle => {
            painter.circle_stroke(p(12.0, 12.0), 9.0 * scale, stroke);
        }
        Icon::Failed => {
            painter.circle_stroke(p(12.0, 12.0), 9.0 * scale, stroke);
            painter.add(polyline(&[(8.5, 8.5), (15.5, 15.5)], false));
            painter.add(polyline(&[(15.5, 8.5), (8.5, 15.5)], false));
        }
        Icon::Warning => {
            painter.circle_stroke(p(12.0, 12.0), 9.0 * scale, stroke);
            painter.add(polyline(&[(12.0, 6.8), (12.0, 13.5)], false));
            painter.circle_filled(p(12.0, 16.8), 1.1 * scale, colour);
        }
        Icon::Cube => {
            // A glassy cube: three shaded faces under the SVG's outline.
            let faces: [(&[(f32, f32)], Color32); 3] = [
                (
                    &[(12.0, 2.0), (21.0, 7.0), (12.0, 12.0), (3.0, 7.0)],
                    Color32::from_rgba_unmultiplied(220, 228, 235, 120),
                ),
                (
                    &[(3.0, 7.0), (12.0, 12.0), (12.0, 22.0), (3.0, 17.0)],
                    Color32::from_rgba_unmultiplied(150, 165, 180, 110),
                ),
                (
                    &[(12.0, 12.0), (21.0, 7.0), (21.0, 17.0), (12.0, 22.0)],
                    Color32::from_rgba_unmultiplied(90, 105, 125, 120),
                ),
            ];
            for (face, fill) in faces {
                painter.add(Shape::convex_polygon(
                    face.iter().map(|(x, y)| p(*x, *y)).collect(),
                    fill,
                    Stroke::NONE,
                ));
            }
            let thin = Stroke::new(1.2 * scale, colour);
            painter.add(Shape::closed_line(
                [
                    (12.0, 2.0),
                    (21.0, 7.0),
                    (21.0, 17.0),
                    (12.0, 22.0),
                    (3.0, 17.0),
                    (3.0, 7.0),
                ]
                .iter()
                .map(|(x, y)| p(*x, *y))
                .collect(),
                thin,
            ));
            painter.add(Shape::line(
                vec![p(3.0, 7.0), p(12.0, 12.0), p(21.0, 7.0)],
                thin,
            ));
            painter.add(Shape::line(vec![p(12.0, 12.0), p(12.0, 22.0)], thin));
        }
        Icon::Disc => {
            // A compact disc: silver body, a sheen and the hub.
            let centre = p(12.0, 12.0);
            painter.circle_filled(centre, 11.0 * scale, Color32::from_rgb(150, 154, 160));
            painter.circle_filled(centre, 10.2 * scale, Color32::from_rgb(196, 200, 206));
            let sheen: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = -2.4 + i as f32 / 16.0 * 1.4;
                    centre + vec2(a.cos(), a.sin()) * 7.5 * scale
                })
                .collect();
            painter.add(Shape::line(
                sheen,
                Stroke::new(3.0 * scale, Color32::from_rgb(240, 242, 245)),
            ));
            painter.circle_filled(centre, 3.6 * scale, Color32::from_rgb(110, 114, 120));
            painter.circle_filled(centre, 1.6 * scale, Color32::from_rgb(20, 22, 24));
            painter.circle_stroke(
                centre,
                11.0 * scale,
                Stroke::new(0.8 * scale, Color32::from_rgb(90, 94, 100)),
            );
        }
    }
}
