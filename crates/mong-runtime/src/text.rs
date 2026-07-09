//! Typewriter chạy theo grapheme cluster, không theo byte hay char:
//! "ế" (U+0065 U+0302 U+0301) không bao giờ hiện nửa chừng thành "e".

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq)]
pub struct Typewriter {
    /// Vị trí byte của đầu mỗi grapheme, cộng thêm `text.len()` ở cuối.
    bounds: Vec<usize>,
    shown: usize,
    elapsed: f64,
}

const EPS: f64 = 1e-6;

impl Typewriter {
    pub fn new(text: &str) -> Self {
        let mut bounds: Vec<usize> = text.grapheme_indices(true).map(|(i, _)| i).collect();
        bounds.push(text.len());
        Typewriter {
            bounds,
            shown: 0,
            elapsed: 0.0,
        }
    }

    pub fn total(&self) -> usize {
        self.bounds.len() - 1
    }
    pub fn done(&self) -> bool {
        self.shown >= self.total()
    }
    pub fn reveal_all(&mut self) {
        self.shown = self.total();
    }

    /// `cps` = grapheme mỗi giây. `shown` là hàm thuần của thời gian đã trôi,
    /// nên 60fps và 144fps hiện chữ cùng nhịp — không có trạng thái nào để
    /// sai số bám vào. `reveal_all` chặn đứng ở `done()`, `shown` không lùi.
    pub fn tick(&mut self, dt: f32, cps: f32) {
        if self.done() || cps <= 0.0 {
            return;
        }
        self.elapsed += f64::from(dt);
        let n = (self.elapsed * f64::from(cps) + EPS).floor() as usize;
        self.shown = n.min(self.total());
    }

    /// Cắt luôn đúng biên grapheme.
    pub fn visible<'a>(&self, text: &'a str) -> &'a str {
        &text[..self.bounds[self.shown]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn khong_cat_giua_dau_tieng_viet() {
        // "ế" tổ hợp: e + circumflex + acute.
        let s = "n\u{0065}\u{0302}\u{0301}u";
        let mut tw = Typewriter::new(s);
        assert_eq!(tw.total(), 3);
        tw.tick(1.0, 2.0);
        assert_eq!(tw.visible(s), "n\u{0065}\u{0302}\u{0301}");
    }

    #[test]
    fn emoji_zwj_la_mot_grapheme() {
        let s = "👨‍👩‍👧";
        assert_eq!(Typewriter::new(s).total(), 1);
    }

    #[test]
    fn toc_do_khong_le_thuoc_framerate() {
        let s = "abcdefghij";
        let (mut a, mut b) = (Typewriter::new(s), Typewriter::new(s));
        for _ in 0..60 {
            a.tick(1.0 / 60.0, 10.0);
        }
        for _ in 0..144 {
            b.tick(1.0 / 144.0, 10.0);
        }
        assert_eq!(a.visible(s), b.visible(s));
        assert_eq!(a.visible(s), "abcdefghij");
    }

    #[test]
    fn reveal_all_bo_qua_thoi_gian() {
        let s = "xin chào";
        let mut tw = Typewriter::new(s);
        tw.reveal_all();
        assert!(tw.done());
        assert_eq!(tw.visible(s), s);
    }
}
