//! Bố cục sân khấu → danh sách vẽ. Thuần dữ liệu: không wgpu, không texture,
//! chỉ asset id + toạ độ ảo. Renderer tra id ra texture và tự lo kích thước.

use crate::stage::{Stage, TransitionKind};
use mong_assets::Manifest;
use mong_core::StagePos;

/// Độ phân giải ảo: mọi toạ độ tính theo khung này, shell letterbox về cửa sổ
/// thật. Bố cục vì thế không đổi theo kích thước cửa sổ.
pub const VIRTUAL_W: f32 = 1920.0;
pub const VIRTUAL_H: f32 = 1080.0;

/// Nhân vật không nói bị làm tối (hành vi prototype).
const DIM: f32 = 0.55;

/// Cách đặt sprite vào khung ảo. Renderer biết kích thước texture, runtime thì không.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fit {
    /// Phủ kín khung ảo, cắt phần thừa — dùng cho nền.
    Cover,
    /// Neo theo điểm `(x, y)`: x là tâm ngang, y là chân sprite.
    Anchor { x: f32, y: f32 },
}

/// Một quad cần vẽ, theo đúng thứ tự trong danh sách (không sort lại).
#[derive(Debug, Clone, PartialEq)]
pub struct DrawItem {
    pub asset: String,
    pub fit: Fit,
    /// RGBA nhân vào màu texture. Alpha < 1 = trong suốt.
    pub tint: [f32; 4],
}

fn anchor_x(pos: StagePos) -> f32 {
    match pos {
        StagePos::Left => VIRTUAL_W * 0.25,
        StagePos::Center => VIRTUAL_W * 0.5,
        StagePos::Right => VIRTUAL_W * 0.75,
    }
}

impl Stage {
    /// Danh sách vẽ của frame hiện tại, từ dưới lên trên:
    /// nền cũ → nền mới (alpha = tiến độ fade) → nhân vật trái/giữa/phải,
    /// mỗi nhân vật là chồng lớp base/face/outfit theo manifest.
    pub fn draw_list(&self, man: &Manifest) -> Vec<DrawItem> {
        let mut out = Vec::new();

        // Nền cũ chỉ tồn tại trong lúc fade.
        let bg_alpha = match &self.transition {
            Some(t) if t.kind == TransitionKind::Fade => {
                if let Some(prev) = &t.from_bg {
                    out.push(DrawItem {
                        asset: prev.clone(),
                        fit: Fit::Cover,
                        tint: [1.0, 1.0, 1.0, 1.0],
                    });
                }
                t.progress()
            }
            _ => 1.0,
        };
        if let Some(bg) = &self.bg {
            out.push(DrawItem {
                asset: bg.clone(),
                fit: Fit::Cover,
                tint: [1.0, 1.0, 1.0, bg_alpha],
            });
        }

        // Thứ tự vẽ nhân vật theo vị trí, không theo thứ tự `show` — để hai
        // lần `show` khác thứ tự nhưng cùng bố cục cho ra cùng một frame.
        let mut chars: Vec<_> = self.chars.iter().collect();
        chars.sort_by_key(|c| match c.pos {
            StagePos::Left => 0,
            StagePos::Center => 1,
            StagePos::Right => 2,
        });

        for c in chars {
            let k = if c.dim { DIM } else { 1.0 };
            let fit = Fit::Anchor {
                x: anchor_x(c.pos),
                y: VIRTUAL_H,
            };
            for asset in man.sprite_stack(&c.id, c.pose.as_deref()) {
                out.push(DrawItem {
                    asset: asset.to_string(),
                    fit,
                    tint: [k, k, k, 1.0],
                });
            }
        }
        out
    }
}
