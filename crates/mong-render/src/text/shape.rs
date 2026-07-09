//! Shaping một dòng thoại. Bất biến: shape **một lần**, mỗi glyph nhớ khoảng
//! byte gốc của mình. Typewriter chỉ lọc theo byte, không shape lại — nếu
//! shape lại chuỗi đã cắt thì chữ nhảy chỗ lúc xuống dòng, và với Ả Rập thì
//! chữ đã hiện còn đổi cả hình dạng khi gõ tiếp.

use cosmic_text::{fontdb, Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping, Wrap};

/// Một glyph đã định vị, toạ độ ảo, gốc trái-trên.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    pub cache_key: CacheKey,
    /// Vị trí baseline-relative đã quy về gốc trái-trên của dòng.
    pub x: f32,
    pub y: f32,
    /// Khoảng byte trong văn bản gốc — mốc lọc của typewriter.
    /// Với chữ ghép (ligature, `ế` tổ hợp) một glyph phủ nhiều byte.
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapedLine {
    pub glyphs: Vec<ShapedGlyph>,
    pub width: f32,
    pub height: f32,
}

impl ShapedLine {
    /// Glyph hiện ra khi typewriter đã lộ `visible_bytes` byte đầu.
    /// Dùng `byte_start`: glyph phủ nhiều byte hiện trọn vẹn ngay khi
    /// grapheme đầu tiên của nó tới lượt — không bao giờ nửa chữ.
    pub fn visible(&self, visible_bytes: usize) -> impl Iterator<Item = &ShapedGlyph> {
        self.glyphs
            .iter()
            .filter(move |g| g.byte_start < visible_bytes)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineSpec {
    pub font_size: f32,
    pub line_height: f32,
    /// Bề rộng để ngắt dòng, theo toạ độ ảo.
    pub max_width: f32,
    /// Font ưu tiên (tên family). Thiếu glyph → cosmic-text tự rơi về font
    /// khác trong `FontSystem`; thứ tự nạp quyết định ưu tiên fallback.
    pub family: String,
}

pub struct Shaper {
    font_system: FontSystem,
}

impl Shaper {
    /// Font đi kèm mongpack là nguồn **duy nhất**: db khởi tạo rỗng, không
    /// `load_system_fonts()`. Nhờ vậy desktop, web và CI shape ra cùng một
    /// kết quả — điều kiện để golden test chữ có nghĩa.
    ///
    /// Cảnh báo: `FontSystem::new()` và `new_with_fonts()` **đều** nạp font
    /// hệ thống. Chỉ `new_with_locale_and_db` với db tự dựng mới không.
    pub fn new() -> Self {
        Shaper {
            font_system: FontSystem::new_with_locale_and_db(
                "en-US".to_string(),
                fontdb::Database::new(),
            ),
        }
    }

    /// Số face đang có. Test dùng để khẳng định không có font lạ lọt vào.
    pub fn face_count(&self) -> usize {
        self.font_system.db().len()
    }

    /// Nạp một font (bytes từ mongpack). Trả tên family để đưa vào `LineSpec`.
    pub fn add_font(&mut self, bytes: Vec<u8>) -> Vec<String> {
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(
                bytes,
            )));
        ids.iter()
            .filter_map(|id| self.font_system.db().face(*id))
            .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
            .collect()
    }

    pub fn font_system_mut(&mut self) -> &mut FontSystem {
        &mut self.font_system
    }

    pub fn shape(&mut self, text: &str, spec: &LineSpec) -> ShapedLine {
        let metrics = Metrics::new(spec.font_size, spec.line_height);
        let mut buf = Buffer::new(&mut self.font_system, metrics);
        buf.set_wrap(&mut self.font_system, Wrap::WordOrGlyph);
        buf.set_size(&mut self.font_system, Some(spec.max_width), None);
        buf.set_text(
            &mut self.font_system,
            text,
            Attrs::new().family(Family::Name(&spec.family)),
            // `Advanced` = bật BiDi + shaping phức tạp: dấu chồng tiếng Việt,
            // Ả Rập nối chữ, CJK. `Basic` sẽ hỏng cả ba.
            Shaping::Advanced,
        );
        buf.shape_until_scroll(&mut self.font_system, false);

        let mut glyphs = Vec::new();
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for run in buf.layout_runs() {
            width = width.max(run.line_w);
            height = height.max(run.line_top + spec.line_height);
            for g in run.glyphs {
                glyphs.push(ShapedGlyph {
                    cache_key: g.physical((0.0, 0.0), 1.0).cache_key,
                    x: g.x,
                    y: run.line_y,
                    byte_start: g.start,
                    byte_end: g.end,
                });
            }
        }
        ShapedLine {
            glyphs,
            width,
            height,
        }
    }
}

impl Default for Shaper {
    fn default() -> Self {
        Self::new()
    }
}
