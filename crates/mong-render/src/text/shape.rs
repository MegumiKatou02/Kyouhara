//! Shaping một dòng thoại. Bất biến: shape **một lần**, mỗi glyph nhớ khoảng
//! byte gốc của mình. Typewriter chỉ lọc theo byte, không shape lại — nếu
//! shape lại chuỗi đã cắt thì chữ nhảy chỗ lúc xuống dòng, và với Ả Rập thì
//! chữ đã hiện còn đổi cả hình dạng khi gõ tiếp.

use cosmic_text::{
    fontdb, Attrs, AttrsList, Buffer, BufferLine, CacheKey, Family, FontSystem, LineEnding,
    Metrics, Shaping, Wrap,
};

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
    /// Chuỗi font theo thứ tự ưu tiên, lấy từ `manifest.fonts[locale
    /// Rỗng = lỗi cấu hình (`shape` panic — thiếu font là lỗi build, không
    /// phải thứ nên chạy tiếp trong im lặng).
    ///
    /// Giới hạn hiện tại: cả dòng dùng **một** family — cái đầu tiên phủ được
    /// ký tự đầu tiên. Câu trộn Việt-Nhật trong cùng một dòng vẫn sai font ở
    /// phần thiểu số. Sửa đúng cần phân đoạn theo script rồi gắn `AttrsList`
    /// riêng cho từng span (M5).
    pub families: Vec<String>,
}

pub struct Shaper {
    font_system: FontSystem,
}

/// Dấu tổ hợp Latin (U+0300–U+036F) và Latin mở rộng. Không tính là "ký tự
/// quyết định": chữ "ế" vẫn thuộc script Latin, chọn font theo nó là đúng
/// nhưng thừa — chọn theo `e` cho cùng kết quả.
fn la_dau_latin(c: char) -> bool {
    matches!(c as u32, 0x0080..=0x024F | 0x0300..=0x036F | 0x1E00..=0x1EFF)
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

    /// Family đầu tiên trong chain phủ được ký tự "quyết định" của `text` —
    /// ký tự không-Latin đầu tiên, hoặc ký tự đầu tiên nếu toàn Latin.
    ///
    /// Vì sao không dùng ký tự đầu: câu "Xin chào 世界" mở đầu bằng Latin, mà
    /// font Latin thường phủ luôn chữ Việt — nó sẽ thắng, rồi CJK rơi vào
    /// fallback mù của cosmic-text. Chọn theo ký tự không-Latin đầu tiên cho
    /// ra font đúng ở đa số ca thực tế (một câu, một script chính).
    ///
    /// Chain rỗng, hoặc không family nào phủ được: rơi về phần tử đầu và để
    /// cosmic-text tự fallback — chữ vẫn hiện, chỉ có thể sai font.
    fn pick_family(&self, text: &str, spec: &LineSpec) -> String {
        let Some(dau) = spec.families.first() else {
            panic!("LineSpec.families rong: du an phai khai bao it nhat mot font");
        };
        let quyet_dinh = text
            .chars()
            .find(|c| !c.is_ascii() && !la_dau_latin(*c))
            .or_else(|| text.chars().find(|c| !c.is_whitespace()));
        let Some(ch) = quyet_dinh else {
            return dau.clone();
        };
        spec.families
            .iter()
            .find(|f| self.family_covers(f, ch))
            .unwrap_or(dau)
            .clone()
    }

    fn family_covers(&self, family: &str, ch: char) -> bool {
        self.font_system.db().faces().any(|face| {
            face.families.iter().any(|(n, _)| n == family)
                && self
                    .font_system
                    .db()
                    .with_face_data(face.id, |data, index| {
                        ttf_parser::Face::parse(data, index)
                            .map(|f| f.glyph_index(ch).is_some())
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
        })
    }

    pub fn shape(&mut self, text: &str, spec: &LineSpec) -> ShapedLine {
        let family = self.pick_family(text, spec);
        let metrics = Metrics::new(spec.font_size, spec.line_height);
        let mut buf = Buffer::new(&mut self.font_system, metrics);
        buf.set_wrap(&mut self.font_system, Wrap::WordOrGlyph);
        buf.set_size(&mut self.font_system, Some(spec.max_width), None);
        buf.set_text(
            &mut self.font_system,
            text,
            Attrs::new().family(Family::Name(&family)),
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
