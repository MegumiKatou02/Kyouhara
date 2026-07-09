//! Bố cục hộp thoại, toạ độ ảo (1920×1080). Thuần dữ liệu, không GPU.
//!
//! Nâng từ `shells/desktop/src/ui.rs` ở M4.3 khi shell thứ hai cần đúng bố
//! cục này (đúng điều kiện file cũ tự đặt ra). Editor M6 có hộp thoại riêng
//! thì dựng bố cục riêng, không sửa file này.

use crate::{VIRTUAL_H, VIRTUAL_W};

pub const BOX_X: f32 = 120.0;
pub const BOX_W: f32 = VIRTUAL_W - BOX_X * 2.0;
pub const BOX_H: f32 = 260.0;
pub const BOX_Y: f32 = VIRTUAL_H - BOX_H - 60.0;

pub const PAD: f32 = 36.0;
pub const NAME_SIZE: f32 = 34.0;
pub const TEXT_SIZE: f32 = 32.0;
pub const LINE_H: f32 = 44.0;

/// Chỗ đặt tên người nói và thân thoại (gốc trái-trên).
pub const NAME_POS: (f32, f32) = (BOX_X + PAD, BOX_Y + PAD);
pub const TEXT_POS: (f32, f32) = (BOX_X + PAD, BOX_Y + PAD + LINE_H + 8.0);
pub const TEXT_W: f32 = BOX_W - PAD * 2.0;

pub const BOX_TINT: [f32; 4] = [0.05, 0.04, 0.08, 0.82];
pub const TEXT_COLOR: [f32; 4] = [0.95, 0.94, 0.92, 1.0];
pub const CHOICE_COLOR: [f32; 4] = [0.85, 0.90, 1.0, 1.0];

/// Màu tên: `#rrggbb` của manifest, hỏng thì trắng ngà.
pub fn parse_color(hex: Option<&str>) -> [f32; 4] {
    let Some(h) = hex.and_then(|h| h.strip_prefix('#')) else {
        return TEXT_COLOR;
    };
    let f = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .map(|v| f32::from(v) / 255.0)
            .unwrap_or(1.0)
    };
    if h.len() != 6 {
        return TEXT_COLOR;
    }
    [f(0), f(2), f(4), 1.0]
}

/// Dòng lựa chọn thứ `i`, xếp từ giữa màn hình xuống.
pub fn choice_pos(i: usize) -> (f32, f32) {
    (BOX_X + PAD, 420.0 + i as f32 * (LINE_H + 16.0))
}
