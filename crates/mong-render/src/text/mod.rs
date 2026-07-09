//! Chữ: shaping (cosmic-text) + atlas glyph R8.
//!
//! `shape` không đụng GPU — bộ test chuỗi hiểm chạy được trên CI headless.
//! `atlas` mới cần wgpu.

mod atlas;
mod shape;

pub use atlas::{GlyphAtlas, ATLAS_DIM};
pub use shape::{LineSpec, ShapedGlyph, ShapedLine, Shaper};

pub(crate) use atlas::create_atlas_texture;
