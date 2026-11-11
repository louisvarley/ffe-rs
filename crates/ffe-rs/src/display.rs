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
    /// xBRZ 2x: Edge-aware pixel upscaling (recommended for modern displays).
    XbrzX2,
    /// xBRZ 3x: Higher quality edge-aware upscaling (slower).
    XbrzX3,
    /// Scale2x: Fast edge-directed scaling.
    Scale2x,
}

impl Filter {
    pub fn key(self) -> &'static str {
        match self {
            Self::Nearest => "nearest",
            Self::Linear => "linear",
            Self::XbrzX2 => "xbrz-2x",
            Self::XbrzX3 => "xbrz-3x",
            Self::Scale2x => "scale2x",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "nearest" => Some(Self::Nearest),
            "linear" => Some(Self::Linear),
            "xbrz-2x" => Some(Self::XbrzX2),
            "xbrz-3x" => Some(Self::XbrzX3),
            "scale2x" => Some(Self::Scale2x),
            _ => None,
        }
    }
    
    /// Returns the upscaling factor applied before window scaling.
    pub fn upscale_factor(self) -> usize {
        match self {
            Self::Nearest | Self::Linear => 1,
            Self::XbrzX2 | Self::Scale2x => 2,
            Self::XbrzX3 => 3,
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
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DisplaySettings {
    pub scale: u32,
    pub fit: FitMode,
    pub filter: Filter,
    pub aspect: PixelAspect,
    /// CRT scanline effect enabled.
    pub scanlines: bool,
    /// Scanline intensity (0.0 = off, 1.0 = full black lines).
    pub scanline_intensity: f32,
}

impl Eq for DisplaySettings {}  // f32 fields are always finite, so Eq is safe

impl Default for DisplaySettings {
    /// 1280x960, pixel-perfect, nearest, 4:3: the port's existing initial
    /// window and the original's non-square DOS pixels.
    fn default() -> Self {
        Self {
            scale: 4,
            fit: FitMode::PixelPerfect,
            filter: Filter::Nearest,
            aspect: PixelAspect::Dos43,
            scanlines: false,
            scanline_intensity: 0.3,
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
            "scale={}\nfit={}\nfilter={}\naspect={}\nscanlines={}\nscanline-intensity={:.2}\n",
            self.scale.clamp(MIN_SCALE, MAX_SCALE),
            self.fit.key(),
            self.filter.key(),
            self.aspect.key(),
            if self.scanlines { "on" } else { "off" },
            self.scanline_intensity.clamp(0.0, 1.0)
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
                "scanlines" => settings.scanlines = matches!(value, "on" | "true" | "1"),
                "scanline-intensity" => {
                    let intensity: f32 = value.parse().ok()?;
                    settings.scanline_intensity = intensity.clamp(0.0, 1.0);
                }
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
    
    // Step 1: Apply pixel-art upscaling filter if needed
    let rect = settings.rect((width, height));
    
    match settings.filter {
        Filter::Nearest | Filter::Linear => {
            // No pre-upscaling, scale directly to window
            blit(
                logical,
                (LOGICAL_WIDTH, LOGICAL_HEIGHT),
                target,
                width,
                rect,
                settings.filter,
            );
        }
        Filter::XbrzX2 => {
            use crate::filters::xbrz_2x;
            let upscaled = xbrz_2x(logical);
            blit(
                &upscaled,
                (LOGICAL_WIDTH * 2, LOGICAL_HEIGHT * 2),
                target,
                width,
                rect,
                Filter::Nearest, // Use nearest for final scaling after upscale
            );
        }
        Filter::XbrzX3 => {
            use crate::filters::xbrz_3x;
            let upscaled = xbrz_3x(logical);
            blit(
                &upscaled,
                (LOGICAL_WIDTH * 3, LOGICAL_HEIGHT * 3),
                target,
                width,
                rect,
                Filter::Nearest,
            );
        }
        Filter::Scale2x => {
            use crate::filters::scale2x;
            let upscaled = scale2x(logical);
            blit(
                &upscaled,
                (LOGICAL_WIDTH * 2, LOGICAL_HEIGHT * 2),
                target,
                width,
                rect,
                Filter::Nearest,
            );
        }
    }
    
    // Step 2: Apply scanlines if enabled
    if settings.scanlines && settings.scanline_intensity > 0.0 {
        use crate::filters::apply_scanlines;
        apply_scanlines(target, width, height, settings.scanline_intensity);
    }
}

/// Copy the intermediate frame into `rect` of `target`, sampling with filter.
fn blit(
    source: &[u32],
    source_size: (usize, usize),
    target: &mut [u32],
    target_width: usize,
    rect: Rect,
    filter: Filter,
) {
    if rect.width == 0 || rect.height == 0 || target_width == 0 {
        return;
    }
    
    let (source_width, source_height) = source_size;
    
    // For pixel-art filters, we already upscaled, so use nearest for final scaling
    // For Nearest/Linear, we scale directly from 320×200
    let use_linear = matches!(filter, Filter::Linear);
    
    for dy in 0..rect.height {
        let row = (rect.y + dy) * target_width + rect.x;
        let Some(destination) = target.get_mut(row..row + rect.width) else {
            continue;
        };
        
        if use_linear {
            // Bilinear sampling
            let (y0, y1, fy) = linear_row(dy, rect.height, source_height);
            for (dx, pixel) in destination.iter_mut().enumerate() {
                let (x0, x1, fx) = linear_row(dx, rect.width, source_width);
                let top = mix(
                    source[y0 * source_width + x0],
                    source[y0 * source_width + x1],
                    fx,
                );
                let bottom = mix(
                    source[y1 * source_width + x0],
                    source[y1 * source_width + x1],
                    fx,
                );
                *pixel = mix(top, bottom, fy);
            }
        } else {
            // Nearest neighbor sampling
            let source_y = dy * source_height / rect.height;
            let source_row = &source[source_y * source_width..][..source_width];
            for (dx, pixel) in destination.iter_mut().enumerate() {
                *pixel = source_row[dx * source_width / rect.width];
            }
        }
    }
}

/// The two source indices and the 0..=65535 weight for one destination index.
fn linear_row(index: usize, destination: usize, source: usize) -> (usize, usize, u32) {
    // u = (index + 0.5) * source / destination - 0.5, in 16.16 fixed point.
    let value = ((2 * index + 1) as i64 * source as i64 * 65536) / (2 * destination as i64) - 32768;
    let value = value.clamp(0, (source.saturating_sub(1) as i64) * 65536) as u64;
    let first = (value / 65536) as usize;
    let weight = (value % 65536) as u32;
    (first, (first + 1).min(source - 1), weight)
}

/// Blend two packed colours by `weight` (0 = `a`, 65536 = `b`).
fn mix(a: u32, b: u32, weight: u32) -> u32 {
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

/// The game window. A thin wrapper so the presenter and the input session share
/// one type; it dereferences to the `minifb` window.
pub struct GameWindow {
    window: Window,
}

impl std::ops::Deref for GameWindow {
    type Target = Window;
    fn deref(&self) -> &Window {
        &self.window
    }
}

impl GameWindow {
    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    pub fn is_active(&mut self) -> bool {
        self.window.is_active()
    }
}

/// Open the game window. `minifb`'s scale is left at 1x and its stretch mode is
/// only a safety net: [`Presenter`] passes a client-sized buffer every frame.
pub fn open_window(title: &str, request: WindowRequest) -> Result<GameWindow, String> {
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
    Ok(GameWindow { window })
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
    size: (usize, usize),
    rect: Rect,
    valid: bool,
}

impl Presenter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget the cached buffer (after the window is reopened).
    pub fn reset(&mut self) {
        self.valid = false;
    }

    /// Scale the logical frame and present it. The black bars are only cleared
    /// when the window size or the fit rectangle changes.
    pub fn present(
        &mut self,
        window: &mut GameWindow,
        logical: &[u32],
        settings: &DisplaySettings,
    ) -> Result<bool, String> {
        let size = window.get_size();
        if size.0 == 0 || size.1 == 0 {
            return Ok(false);
        }
        
        // Use compose to handle filters and scanlines
        compose(logical, settings, size.0, size.1, &mut self.buffer);
        self.size = size;
        self.rect = settings.rect(size);
        self.valid = true;
        window
            .window
            .update_with_buffer(&self.buffer, size.0, size.1)
            .map_err(|error| format!("Window update: {error}"))?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            scanlines: false,
            scanline_intensity: 0.3,
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
