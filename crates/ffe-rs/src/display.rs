//! Port extension: how the 320x200 logical framebuffer reaches the window.
//!
//! The original draws a 320x200 screen and leaves scaling to the DOS video
//! mode: 320x200 on a 4:3 monitor means the pixels are 1.2 times taller than
//! they are wide (`CLS-002`). The port keeps that 320x200 buffer authoritative
//! (`screen::Screen::logical_pixels`) and scales it here, after the frame is
//! composed. Nothing in this module changes the game's pixels, its captures or
//! the simulation.
//!
//! The presenter is deliberately its own small scaler rather than minifb's
//! `Scale` option: minifb can stretch a buffer to the window, but it cannot
//! centre an integer multiple and letterbox the rest, and it has no linear
//! filter choice. Options:
//!
//! | Option | Values | Effect |
//! |---|---|---|
//! | window scale | 1x..6x | initial window size (still resizable) |
//! | fit | pixel-perfect / fill window | integer multiples with black bars, or an aspect-preserving fractional fit |
//! | filter | nearest / linear | nearest is the original's blocky look; linear only differs in a fractional fit |
//! | pixels | 4:3 (DOS) / square | the original's non-square pixels, or square pixels |
//!
//! `Alt+Enter` toggles a borderless full-screen window. minifb 0.28 has no
//! live resize/fullscreen call, so the caller drops and reopens the window when
//! [`window_request`] changes. The screen size for full-screen comes from
//! `GetSystemMetrics` on Windows; other platforms fall back to the configured
//! window size. That is the module's only `unsafe` (documented in
//! `docs/ARCHITECTURE.md` §2.2).

use crate::indexed::{LOGICAL_HEIGHT, LOGICAL_WIDTH};
use crate::modern::{Graphics, ModernRenderer};
use crate::modern3d::{Gpu3d, World};
use minifb::{Scale, ScaleMode, Window, WindowOptions};

/// The smallest window scale the Display Options page offers.
pub const MIN_SCALE: u32 = 1;
/// The largest window scale the Display Options page offers.
pub const MAX_SCALE: u32 = 6;

/// How the logical frame is fitted into the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FitMode {
    /// Integer multiples of the logical frame, centred with black bars.
    PixelPerfect,
    /// Fill the window, keeping the display aspect ratio (fractional scale).
    FillWindow,
}

impl FitMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::PixelPerfect => "pixel-perfect",
            Self::FillWindow => "fill-window",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "pixel-perfect" => Some(Self::PixelPerfect),
            "fill-window" => Some(Self::FillWindow),
            _ => None,
        }
    }
}

/// The scaling filter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    /// One source pixel per block: the original's look, and the default.
    Nearest,
    /// Bilinear sampling; visible only when the fit is fractional.
    Linear,
}

impl Filter {
    pub fn key(self) -> &'static str {
        match self {
            Self::Nearest => "nearest",
            Self::Linear => "linear",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "nearest" => Some(Self::Nearest),
            "linear" => Some(Self::Linear),
            _ => None,
        }
    }
}

/// The logical pixel's shape on the display.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelAspect {
    /// The original DOS 4:3 screen: 320x200 is shown as 320x240.
    Dos43,
    /// Square pixels: 320x200 is shown as 320x200 (16:10).
    Square,
}

impl PixelAspect {
    pub fn key(self) -> &'static str {
        match self {
            Self::Dos43 => "dos-4:3",
            Self::Square => "square",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "dos-4:3" | "dos43" => Some(Self::Dos43),
            "square" => Some(Self::Square),
            _ => None,
        }
    }

    /// The logical height at scale 1, after the display correction.
    pub fn base_height(self) -> usize {
        match self {
            // 200 * 6 / 5 = 240.
            Self::Dos43 => LOGICAL_HEIGHT * 6 / 5,
            Self::Square => LOGICAL_HEIGHT,
        }
    }
}

/// The persisted display choices. `Alt+Enter` is session-only and lives in
/// `prefs::Prefs`, not here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DisplaySettings {
    pub scale: u32,
    pub fit: FitMode,
    pub filter: Filter,
    pub aspect: PixelAspect,
    /// Which engine shows the frame (`modern`); Classic by default.
    pub graphics: Graphics,
}

impl Default for DisplaySettings {
    /// 1280x960, pixel-perfect, nearest, 4:3: the port's existing initial
    /// window and the original's non-square DOS pixels.
    fn default() -> Self {
        Self {
            scale: 4,
            fit: FitMode::PixelPerfect,
            filter: Filter::Nearest,
            aspect: PixelAspect::Dos43,
            graphics: Graphics::Classic,
        }
    }
}

impl DisplaySettings {
    /// The logical frame's size after the pixel-aspect correction (320x240 or
    /// 320x200), i.e. the unit an integer fit repeats.
    pub fn unit_size(self) -> (usize, usize) {
        (LOGICAL_WIDTH, self.aspect.base_height())
    }

    /// The initial window's client size. With the defaults this is 1280x960.
    pub fn window_size(self) -> (usize, usize) {
        let (width, height) = self.unit_size();
        let scale = self.scale.clamp(MIN_SCALE, MAX_SCALE) as usize;
        (width * scale, height * scale)
    }

    /// Where the logical frame lands in a window of `window` size.
    pub fn rect(self, window: (usize, usize)) -> Rect {
        fit_rect(window.0, window.1, self.fit, self.aspect.base_height())
    }

    /// The persistable form (`saves/display.cfg`).
    pub fn to_config(self) -> String {
        format!(
            "scale={}\nfit={}\nfilter={}\naspect={}\ngraphics={}\n",
            self.scale.clamp(MIN_SCALE, MAX_SCALE),
            self.fit.key(),
            self.filter.key(),
            self.aspect.key(),
            self.graphics.key()
        )
    }

    /// Parse `to_config`'s form. Missing keys keep the default; an unknown key
    /// is ignored (forward compatibility), but a known key with a bad value
    /// rejects the file. The caller keeps its default in that case.
    pub fn from_config(text: &str) -> Option<Self> {
        let mut settings = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=')?;
            let value = value.trim();
            match key.trim() {
                "scale" => {
                    let scale: u32 = value.parse().ok()?;
                    if !(MIN_SCALE..=MAX_SCALE).contains(&scale) {
                        return None;
                    }
                    settings.scale = scale;
                }
                "fit" => settings.fit = FitMode::from_key(value)?,
                "filter" => settings.filter = Filter::from_key(value)?,
                "aspect" => settings.aspect = PixelAspect::from_key(value)?,
                "graphics" => settings.graphics = Graphics::from_key(value)?,
                _ => {}
            }
        }
        Some(settings)
    }
}

/// A centred destination rectangle in window pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// Fit the 320x200 frame into a `window_width` x `window_height` client area.
///
/// `base_height` is 240 for the original 4:3 pixels and 200 for square pixels.
/// A pixel-perfect fit uses the largest integer multiple of 320 x `base_height`
/// that fits; when the window is smaller than the frame it falls back to the
/// aspect-preserving fractional fit so the whole frame stays visible.
pub fn fit_rect(
    window_width: usize,
    window_height: usize,
    fit: FitMode,
    base_height: usize,
) -> Rect {
    if window_width == 0 || window_height == 0 || base_height == 0 {
        return Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    }
    let base_width = LOGICAL_WIDTH;
    let integer = (fit == FitMode::PixelPerfect)
        .then(|| (window_width / base_width).min(window_height / base_height))
        .filter(|scale| *scale >= 1);
    let (width, height) = match integer {
        Some(scale) => (base_width * scale, base_height * scale),
        None => {
            // Aspect-preserving fractional fit. Compare window_width /
            // window_height with base_width / base_height without floats.
            if window_width * base_height >= window_height * base_width {
                let height = window_height;
                (window_height * base_width / base_height, height)
            } else {
                let width = window_width;
                (width, window_width * base_height / base_width)
            }
        }
    };
    let width = width.clamp(1, window_width);
    let height = height.clamp(1, window_height);
    Rect {
        x: (window_width - width) / 2,
        y: (window_height - height) / 2,
        width,
        height,
    }
}

/// The logical 320x200 pixel under a client position, or `None` on the black
/// bars/outside the window. This replaces the fixed 4:3 mapping of
/// `screen::Screen::logical_point` so clicks follow the chosen presentation.
pub fn logical_point(
    settings: &DisplaySettings,
    x: f32,
    y: f32,
    window: (usize, usize),
) -> Option<(i32, i32)> {
    let rect = settings.rect(window);
    if rect.width == 0 || rect.height == 0 {
        return None;
    }
    let x = x as i64;
    let y = y as i64;
    if x < rect.x as i64
        || y < rect.y as i64
        || x >= (rect.x + rect.width) as i64
        || y >= (rect.y + rect.height) as i64
    {
        return None;
    }
    let logical_x = (x - rect.x as i64) * LOGICAL_WIDTH as i64 / rect.width as i64;
    let logical_y = (y - rect.y as i64) * LOGICAL_HEIGHT as i64 / rect.height as i64;
    Some((
        logical_x.min(LOGICAL_WIDTH as i64 - 1) as i32,
        logical_y.min(LOGICAL_HEIGHT as i64 - 1) as i32,
    ))
}

/// Fill `target` (a `width`-pixel-wide window buffer) with black bars and the
/// scaled logical frame. Used by the presenter and by `--capture-display`.
pub fn compose(
    logical: &[u32],
    settings: &DisplaySettings,
    width: usize,
    height: usize,
    target: &mut Vec<u32>,
) {
    target.clear();
    target.resize(width.saturating_mul(height), 0);
    if width == 0 || height == 0 || logical.len() < LOGICAL_WIDTH * LOGICAL_HEIGHT {
        return;
    }
    let rect = settings.rect((width, height));
    match settings.graphics {
        Graphics::Classic => blit(logical, target, width, rect, settings.filter),
        Graphics::Modern => ModernRenderer::new().render(logical, target, width, rect),
    }
}

/// Bilinear resampling of a `source_width` by `source_height` image into `rect` of `target` (a
/// `width`-pixel-wide buffer): the modern engine's last step.
pub fn resample_linear(
    source: &[u32],
    source_width: usize,
    source_height: usize,
    target: &mut [u32],
    width: usize,
    rect: Rect,
) {
    if rect.width == 0 || rect.height == 0 || width == 0 || source_width == 0 {
        return;
    }
    let columns: Vec<(usize, usize, u32)> = (0..rect.width)
        .map(|dx| linear_row(dx, rect.width, source_width))
        .collect();
    for dy in 0..rect.height {
        let row = (rect.y + dy) * width + rect.x;
        let Some(destination) = target.get_mut(row..row + rect.width) else {
            continue;
        };
        let (y0, y1, fy) = linear_row(dy, rect.height, source_height);
        let (upper, lower) = (&source[y0 * source_width..], &source[y1 * source_width..]);
        for (pixel, (x0, x1, fx)) in destination.iter_mut().zip(&columns) {
            let top = mix(upper[*x0], upper[*x1], *fx);
            let bottom = mix(lower[*x0], lower[*x1], *fx);
            *pixel = mix(top, bottom, fy);
        }
    }
}

/// Copy the logical frame into `rect` of `target`, sampling with `filter`.
fn blit(logical: &[u32], target: &mut [u32], width: usize, rect: Rect, filter: Filter) {
    if rect.width == 0 || rect.height == 0 || width == 0 {
        return;
    }
    for dy in 0..rect.height {
        let row = (rect.y + dy) * width + rect.x;
        let Some(destination) = target.get_mut(row..row + rect.width) else {
            continue;
        };
        match filter {
            Filter::Nearest => {
                let source_y = dy * LOGICAL_HEIGHT / rect.height;
                let source = &logical[source_y * LOGICAL_WIDTH..][..LOGICAL_WIDTH];
                for (dx, pixel) in destination.iter_mut().enumerate() {
                    *pixel = source[dx * LOGICAL_WIDTH / rect.width];
                }
            }
            Filter::Linear => {
                let (y0, y1, fy) = linear_row(dy, rect.height, LOGICAL_HEIGHT);
                for (dx, pixel) in destination.iter_mut().enumerate() {
                    let (x0, x1, fx) = linear_row(dx, rect.width, LOGICAL_WIDTH);
                    let top = mix(
                        logical[y0 * LOGICAL_WIDTH + x0],
                        logical[y0 * LOGICAL_WIDTH + x1],
                        fx,
                    );
                    let bottom = mix(
                        logical[y1 * LOGICAL_WIDTH + x0],
                        logical[y1 * LOGICAL_WIDTH + x1],
                        fx,
                    );
                    *pixel = mix(top, bottom, fy);
                }
            }
        }
    }
}

/// The two source indices and the 0..=65535 weight for one destination index.
pub(crate) fn linear_row(index: usize, destination: usize, source: usize) -> (usize, usize, u32) {
    // u = (index + 0.5) * source / destination - 0.5, in 16.16 fixed point.
    let value = ((2 * index + 1) as i64 * source as i64 * 65536) / (2 * destination as i64) - 32768;
    let value = value.clamp(0, (source.saturating_sub(1) as i64) * 65536) as u64;
    let first = (value / 65536) as usize;
    let weight = (value % 65536) as u32;
    (first, (first + 1).min(source - 1), weight)
}

/// Blend two packed colours by `weight` (0 = `a`, 65536 = `b`).
pub(crate) fn mix(a: u32, b: u32, weight: u32) -> u32 {
    let channel = |shift: u32| {
        let first = (a >> shift) & 0xff;
        let second = (b >> shift) & 0xff;
        ((first * (65536 - weight) + second * weight + 32768) >> 16) & 0xff
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// The window the presenter needs. `minifb` cannot change a window's size or
/// border after creation, so the caller compares this and reopens on a change.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WindowRequest {
    pub width: usize,
    pub height: usize,
    pub fullscreen: bool,
}

/// The request for these settings. Full-screen uses the desktop size where the
/// platform reports it; elsewhere it keeps the configured window size.
pub fn window_request(settings: &DisplaySettings, fullscreen: bool) -> WindowRequest {
    let (width, height) = if fullscreen {
        desktop_size().unwrap_or_else(|| settings.window_size())
    } else {
        settings.window_size()
    };
    WindowRequest {
        width,
        height,
        fullscreen,
    }
}

/// Open the game window. `minifb`'s scale is left at 1x and its stretch mode is
/// only a safety net: [`Presenter`] passes a client-sized buffer every frame.
pub fn open_window(title: &str, request: WindowRequest) -> Result<Window, String> {
    let mut window = Window::new(
        title,
        request.width,
        request.height,
        WindowOptions {
            resize: !request.fullscreen,
            borderless: request.fullscreen,
            none: request.fullscreen,
            scale: Scale::X1,
            scale_mode: ScaleMode::Stretch,
            ..WindowOptions::default()
        },
    )
    .map_err(|error| format!("Cannot open game window: {error}"))?;
    window.set_target_fps(60);
    window.topmost(request.fullscreen);
    if request.fullscreen {
        window.set_position(0, 0);
    }
    Ok(window)
}

/// The desktop size for borderless full-screen. Windows only; the port's
/// release target. `GetSystemMetrics` takes and returns plain integers, so
/// this is the module's only foreign call and needs no pointer handling.
#[cfg(windows)]
fn desktop_size() -> Option<(usize, usize)> {
    use std::ffi::c_int;
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(index: c_int) -> c_int;
    }
    const SM_CXSCREEN: c_int = 0;
    const SM_CYSCREEN: c_int = 1;
    // SAFETY: `GetSystemMetrics` reads a system constant; it has no pointer
    // arguments and cannot retain anything.
    let (width, height) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    (width > 0 && height > 0).then_some((width as usize, height as usize))
}

#[cfg(not(windows))]
fn desktop_size() -> Option<(usize, usize)> {
    None
}

/// Whether Alt is held, for the Alt+Enter toggle.
///
/// minifb 0.28's Windows scan-code table maps only the *extended* Alt
/// (scancode `0x15d`) to `Key::Menu`, so the ordinary left Alt never appears in
/// `Window::get_keys`. Ask Win32 for the physical key state instead; the
/// caller still accepts minifb's Alt keys for platforms that report them.
#[cfg(windows)]
pub fn alt_held() -> bool {
    use std::ffi::c_int;
    #[link(name = "user32")]
    extern "system" {
        fn GetAsyncKeyState(key: c_int) -> i16;
    }
    const VK_MENU: c_int = 0x12;
    // SAFETY: `GetAsyncKeyState` takes a virtual-key code and has no pointer
    // arguments; the high bit is the held state.
    unsafe { (GetAsyncKeyState(VK_MENU) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
pub fn alt_held() -> bool {
    false
}

/// Keeps the client-sized presentation buffer between frames.
#[derive(Default)]
pub struct Presenter {
    buffer: Vec<u32>,
    modern: ModernRenderer,
    /// The modern engine's GPU 3-D view (`modern3d`).
    gpu: Gpu3d,
    size: (usize, usize),
    rect: Rect,
    valid: bool,
}

impl Presenter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The modern engine, to give it replacement art.
    pub fn modern_mut(&mut self) -> &mut ModernRenderer {
        &mut self.modern
    }

    /// Read the GPU view's upscaled textures from the owner's image export (`img-export`),
    /// again whenever a file there changes.
    pub fn set_art_dir(&mut self, dir: std::path::PathBuf) {
        self.gpu.set_art_dir(dir);
    }

    /// What went wrong reading upscaled textures since the last call.
    pub fn take_art_problems(&mut self) -> Vec<String> {
        self.gpu.take_art_problems()
    }

    /// Whether Modern graphics can draw the 3-D view on the GPU (opening it on first use). The
    /// game sets its model faces aside for the GPU only when this holds.
    pub fn gpu_available(&mut self) -> bool {
        self.gpu.available()
    }

    /// The GPU adapter's name, or why there is none.
    pub fn gpu_description(&mut self) -> String {
        self.gpu.describe()
    }

    /// Forget the cached buffer (after the window is reopened).
    pub fn reset(&mut self) {
        self.valid = false;
    }

    /// Scale the logical frame and present it. The black bars are only cleared
    /// when the window size or the fit rectangle changes.
    pub fn present(
        &mut self,
        window: &mut Window,
        logical: &[u32],
        world: Option<&World>,
        settings: &DisplaySettings,
    ) -> Result<(), String> {
        let size = window.get_size();
        if size.0 == 0 || size.1 == 0 {
            return Ok(());
        }
        let rect = settings.rect(size);
        if !self.valid || self.size != size || self.rect != rect {
            self.buffer.clear();
            self.buffer.resize(size.0 * size.1, 0);
            self.size = size;
            self.rect = rect;
            self.valid = true;
        }
        match settings.graphics {
            Graphics::Classic => blit(logical, &mut self.buffer, size.0, rect, settings.filter),
            Graphics::Modern => {
                self.modern.render(logical, &mut self.buffer, size.0, rect);
                if let Some(world) = world {
                    self.gpu
                        .render(logical, world, &mut self.buffer, size.0, rect)?;
                }
            }
        }
        window
            .update_with_buffer(&self.buffer, size.0, size.1)
            .map_err(|error| format!("Window update: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engines_share_the_frame_and_only_modern_smooths_it() {
        // A frame with a diagonal staircase: the two engines show the same logical pixels,
        // the frame itself is never written, and a flat area stays the same colour.
        let mut logical = vec![0x0c0c4cu32; LOGICAL_WIDTH * LOGICAL_HEIGHT];
        for step in 0..100 {
            for x in 0..=step {
                logical[step * LOGICAL_WIDTH + x + 100] = 0xffffff;
            }
        }
        let before = logical.clone();
        let classic = DisplaySettings::default();
        let modern = DisplaySettings {
            graphics: Graphics::Modern,
            ..classic
        };
        let (mut a, mut b) = (Vec::new(), Vec::new());
        compose(&logical, &classic, 1280, 960, &mut a);
        compose(&logical, &modern, 1280, 960, &mut b);
        assert_eq!(logical, before);
        assert_ne!(a, b);
        // Far from the staircase the two agree exactly.
        assert_eq!(a[900 * 1280 + 40], b[900 * 1280 + 40]);
        // Classic is the original's blocky nearest scaling; Modern adds in-between shades on
        // the edge only.
        let shades = |image: &[u32]| {
            let mut seen: Vec<u32> = image.to_vec();
            seen.sort_unstable();
            seen.dedup();
            seen.len()
        };
        assert_eq!(shades(&a), 2);
        assert!(shades(&b) > 2);
    }

    #[test]
    fn the_defaults_are_the_ports_existing_1280x960_4_3_window() {
        let settings = DisplaySettings::default();
        assert_eq!(settings.unit_size(), (320, 240));
        assert_eq!(settings.window_size(), (1280, 960));
        assert_eq!(settings.scale, 4);
        assert_eq!(settings.filter, Filter::Nearest);
    }

    #[test]
    fn integer_fits_are_centred_with_letterbox_bars() {
        let settings = DisplaySettings::default();
        assert_eq!(
            settings.rect((1280, 960)),
            Rect {
                x: 0,
                y: 0,
                width: 1280,
                height: 960
            }
        );
        // 1920x1080: the largest 4:3 integer step is 4x, 1280x960.
        assert_eq!(
            settings.rect((1920, 1080)),
            Rect {
                x: 320,
                y: 60,
                width: 1280,
                height: 960
            }
        );
        // 1000x700: only 2x (640x480) fits.
        assert_eq!(
            settings.rect((1000, 700)),
            Rect {
                x: 180,
                y: 110,
                width: 640,
                height: 480
            }
        );
        // A window smaller than the frame falls back to the whole frame.
        assert_eq!(
            settings.rect((200, 150)),
            Rect {
                x: 0,
                y: 0,
                width: 200,
                height: 150
            }
        );
        // Square pixels at 1920x1080 give a 5x 1600x1000 fit.
        let square = DisplaySettings {
            aspect: PixelAspect::Square,
            ..settings
        };
        assert_eq!(
            square.rect((1920, 1080)),
            Rect {
                x: 160,
                y: 40,
                width: 1600,
                height: 1000
            }
        );
    }

    #[test]
    fn fill_window_preserves_the_display_aspect() {
        let settings = DisplaySettings {
            fit: FitMode::FillWindow,
            ..DisplaySettings::default()
        };
        // 4:3 in a 16:9 window: full height, bars left and right.
        assert_eq!(
            settings.rect((1920, 1080)),
            Rect {
                x: 240,
                y: 0,
                width: 1440,
                height: 1080
            }
        );
        let square = DisplaySettings {
            aspect: PixelAspect::Square,
            ..settings
        };
        // 16:10 in a 16:9 window: full height, narrower bars.
        assert_eq!(
            square.rect((1920, 1080)),
            Rect {
                x: 96,
                y: 0,
                width: 1728,
                height: 1080
            }
        );
        // A tall window: full width, bars above and below.
        assert_eq!(
            settings.rect((1000, 1000)),
            Rect {
                x: 0,
                y: 125,
                width: 1000,
                height: 750
            }
        );
    }

    #[test]
    fn nearest_scaling_repeats_each_logical_pixel() {
        let mut logical = vec![0u32; LOGICAL_WIDTH * LOGICAL_HEIGHT];
        for (index, pixel) in logical.iter_mut().enumerate() {
            *pixel = index as u32;
        }
        let settings = DisplaySettings {
            scale: 2,
            ..DisplaySettings::default()
        };
        let mut target = Vec::new();
        compose(&logical, &settings, 640, 480, &mut target);
        // The 4:3 correction stretches 200 rows to 480.
        assert_eq!(target[0], 0);
        assert_eq!(target[1], 0);
        assert_eq!(target[2], 1);
        assert_eq!(target[640], 0);
        assert_eq!(target[641], 0);
        // Row 2 samples source row 0 as well (2.4 rows per source row).
        assert_eq!(target[640 * 2], 0);
        assert_eq!(target[640 * 2 + 2], 1);
        // Every destination pixel is a source pixel value.
        assert!(target.iter().all(|pixel| (*pixel as usize) < logical.len()));
        assert_eq!(target.len(), 640 * 480);
    }

    #[test]
    fn linear_scaling_blends_a_fractional_fit() {
        let mut logical = vec![0u32; LOGICAL_WIDTH * LOGICAL_HEIGHT];
        for (index, pixel) in logical.iter_mut().enumerate() {
            *pixel = if index % LOGICAL_WIDTH < LOGICAL_WIDTH / 2 {
                0x00_00_00
            } else {
                0xff_ff_ff
            };
        }
        let settings = DisplaySettings {
            fit: FitMode::FillWindow,
            filter: Filter::Linear,
            ..DisplaySettings::default()
        };
        let mut target = Vec::new();
        // 1000x750 is a 3.125x fit of 4:3, so samples land between pixels.
        compose(&logical, &settings, 1000, 750, &mut target);
        assert_eq!(target.len(), 1000 * 750);
        assert!(target.contains(&0x00_00_00));
        assert!(target.contains(&0xff_ff_ff));
        assert!(
            target
                .iter()
                .any(|pixel| *pixel != 0 && *pixel != 0xff_ff_ff),
            "a linear fit produces intermediate channels"
        );
    }

    #[test]
    fn logical_points_follow_the_fit_and_ignore_the_bars() {
        let settings = DisplaySettings::default();
        assert_eq!(
            logical_point(&settings, 640.0, 480.0, (1280, 960)),
            Some((160, 100))
        );
        assert_eq!(
            logical_point(&settings, 300.0, 408.0, (1280, 960)),
            Some((75, 85))
        );
        // The 1920x1080 letterbox is 320 pixels wide.
        assert_eq!(logical_point(&settings, 100.0, 500.0, (1920, 1080)), None);
        assert_eq!(
            logical_point(&settings, 320.0 + 640.0, 60.0 + 480.0, (1920, 1080)),
            Some((160, 100))
        );
        // Square pixels change the vertical mapping.
        let square = DisplaySettings {
            aspect: PixelAspect::Square,
            ..settings
        };
        assert_eq!(
            logical_point(&square, 640.0, 400.0, (1280, 960)),
            Some((160, 80))
        );
    }

    #[test]
    fn the_config_round_trips_and_rejects_bad_values() {
        let settings = DisplaySettings {
            scale: 3,
            fit: FitMode::FillWindow,
            filter: Filter::Linear,
            aspect: PixelAspect::Square,
            graphics: crate::modern::Graphics::Modern,
        };
        assert_eq!(
            DisplaySettings::from_config(&settings.to_config()),
            Some(settings)
        );
        assert_eq!(
            DisplaySettings::from_config("scale=2\n"),
            Some(DisplaySettings {
                scale: 2,
                ..DisplaySettings::default()
            })
        );
        assert_eq!(DisplaySettings::from_config("scale=0\n"), None);
        assert_eq!(DisplaySettings::from_config("scale=7\n"), None);
        assert_eq!(DisplaySettings::from_config("filter=aniso\n"), None);
        assert_eq!(DisplaySettings::from_config("nonsense\n"), None);
        // Unknown keys are ignored so older builds can read newer files.
        assert_eq!(
            DisplaySettings::from_config("future=1\nscale=2\n"),
            Some(DisplaySettings {
                scale: 2,
                ..DisplaySettings::default()
            })
        );
    }

    #[test]
    fn every_scale_is_a_positive_window() {
        for scale in MIN_SCALE..=MAX_SCALE {
            let settings = DisplaySettings {
                scale,
                ..DisplaySettings::default()
            };
            assert_eq!(
                settings.window_size(),
                (320 * scale as usize, 240 * scale as usize)
            );
            assert_eq!(window_request(&settings, false).width, 320 * scale as usize);
        }
    }
}
