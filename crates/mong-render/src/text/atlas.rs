//! Atlas glyph: một texture R8Unorm, xếp kệ (shelf packing).
//!
//! Không dùng compute shader, không mip — sàn WebGL2. 2048² phủ thoải mái
//! một font Việt + một font CJK ở cỡ thoại thường dùng.

use super::shape::{ShapedGlyph, Shaper};
use crate::{Fit, Sprite, TextureId};
use cosmic_text::{CacheKey, SwashCache, SwashContent};
use std::collections::HashMap;

pub const ATLAS_DIM: u32 = 2048;

#[derive(Clone, Copy)]
struct Slot {
    uv: [f32; 4],
    /// Kích thước pixel và lệch so với điểm đặt bút.
    w: f32,
    h: f32,
    left: f32,
    top: f32,
}

pub struct GlyphAtlas {
    texture: wgpu::Texture,
    id: TextureId,
    swash: SwashCache,
    slots: HashMap<CacheKey, Option<Slot>>,
    /// Con trỏ xếp kệ.
    pen_x: u32,
    pen_y: u32,
    shelf_h: u32,
    full: bool,
}

impl GlyphAtlas {
    /// `id` do `Renderer` cấp: atlas là một texture như mọi texture khác,
    /// nên gộp draw call và bind group đi chung một đường.
    pub fn new(texture: wgpu::Texture, id: TextureId) -> Self {
        GlyphAtlas {
            texture,
            id,
            swash: SwashCache::new(),
            slots: HashMap::new(),
            pen_x: 0,
            pen_y: 0,
            shelf_h: 0,
            full: false,
        }
    }

    pub fn texture_id(&self) -> TextureId {
        self.id
    }

    fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if self.full || w > ATLAS_DIM {
            return None;
        }
        if self.pen_x + w > ATLAS_DIM {
            self.pen_x = 0;
            self.pen_y += self.shelf_h;
            self.shelf_h = 0;
        }
        if self.pen_y + h > ATLAS_DIM {
            // Atlas đầy: chữ còn lại không hiện. Không evict — evict giữa
            // frame làm chữ nhấp nháy. Mốc sau: atlas thứ hai.
            self.full = true;
            eprintln!("canh bao: atlas glyph day, chu se thieu");
            return None;
        }
        let at = (self.pen_x, self.pen_y);
        self.pen_x += w;
        self.shelf_h = self.shelf_h.max(h);
        Some(at)
    }

    /// Rasterize và nạp glyph nếu chưa có. `None` = không vẽ được
    /// (glyph rỗng như dấu cách, hoặc glyph màu — xem ghi chú M3.5).
    fn ensure(&mut self, shaper: &mut Shaper, queue: &wgpu::Queue, key: CacheKey) -> Option<Slot> {
        if let Some(slot) = self.slots.get(&key) {
            return *slot;
        }
        let slot = self.rasterize(shaper, queue, key);
        self.slots.insert(key, slot);
        slot
    }

    fn rasterize(
        &mut self,
        shaper: &mut Shaper,
        queue: &wgpu::Queue,
        key: CacheKey,
    ) -> Option<Slot> {
        // let img = self
        //     .swash
        //     .get_image(shaper.font_system_mut(), key)
        //     .as_ref()?;
        // let (w, h) = (img.placement.width, img.placement.height);
        // if w == 0 || h == 0 {
        //     return None; // dấu cách, ký tự điều khiển
        // }
        // let mask: Vec<u8> = match img.content {
        //     SwashContent::Mask => img.data.clone(),
        //     SwashContent::SubpixelMask => img.data.iter().step_by(4).copied().collect(),
        //     SwashContent::Color => {
        //         // Emoji màu: chưa hỗ trợ (cần atlas RGBA thứ hai). Phần mở.
        //         eprintln!("glyph mau chua ho tro, bo qua");
        //         return None;
        //     }
        // };
        // Rút dữ liệu ra khỏi `swash` rồi trả lại mượn ngay: `alloc` bên dưới
        // cần `&mut self`, mà `img` giữ `&mut self.swash` nếu để nó sống tiếp.
        let (placement, mask) = {
            let img = self
                .swash
                .get_image(shaper.font_system_mut(), key)
                .as_ref()?;
            if img.placement.width == 0 || img.placement.height == 0 {
                return None; // dấu cách, ký tự điều khiển
            }
            let mask: Vec<u8> = match img.content {
                SwashContent::Mask => img.data.clone(),
                SwashContent::SubpixelMask => img.data.iter().step_by(4).copied().collect(),
                SwashContent::Color => {
                    // Emoji màu: chưa hỗ trợ (cần atlas RGBA thứ hai). Phần mở.
                    eprintln!("glyph mau chua ho tro, bo qua");
                    return None;
                }
            };
            (img.placement, mask)
        };
        let (w, h) = (placement.width, placement.height);
        let (x, y) = self.alloc(w, h)?;
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &mask,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let d = ATLAS_DIM as f32;
        Some(Slot {
            uv: [x as f32 / d, y as f32 / d, w as f32 / d, h as f32 / d],
            w: w as f32,
            h: h as f32,
            left: placement.left as f32,
            top: placement.top as f32,
        })
    }

    /// Quad của những glyph đã lộ, đặt tại `origin` (gốc trái-trên của dòng).
    pub fn quads(
        &mut self,
        shaper: &mut Shaper,
        queue: &wgpu::Queue,
        glyphs: impl Iterator<Item = ShapedGlyph>,
        origin: (f32, f32),
        color: [f32; 4],
    ) -> Vec<Sprite> {
        let id = self.id;
        glyphs
            .filter_map(|g| {
                let s = self.ensure(shaper, queue, g.cache_key)?;
                Some(Sprite {
                    texture: id,
                    fit: Fit::Rect {
                        x: origin.0 + g.x + s.left,
                        y: origin.1 + g.y - s.top,
                        w: s.w,
                        h: s.h,
                    },
                    tint: color,
                    uv: s.uv,
                    mask: true,
                })
            })
            .collect()
    }
}

/// Tạo texture atlas. Gọi từ `Renderer` vì nó giữ device + bind group layout.
pub(crate) fn create_atlas_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glyph_atlas"),
        size: wgpu::Extent3d {
            width: ATLAS_DIM,
            height: ATLAS_DIM,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

const _: () = assert!(ATLAS_DIM <= crate::MAX_TEXTURE_DIM);

impl std::fmt::Debug for GlyphAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlyphAtlas")
            .field("glyphs", &self.slots.len())
            .field("full", &self.full)
            .finish()
    }
}
