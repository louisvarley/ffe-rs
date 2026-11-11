// Display filters for pixel-art upscaling and CRT effects.
//
// These operate on the 320×200 RGBA frame after palette lookup but before
// final window scaling. See docs/notes/display-filters-design.md.

pub mod scale2x;
pub mod xbrz;
pub mod scanlines;

pub use scale2x::scale2x;
pub use xbrz::{xbrz_2x, xbrz_3x};
pub use scanlines::apply_scanlines;
