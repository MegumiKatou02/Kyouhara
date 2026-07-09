//! Bộ test chuỗi hiểm (mục 14 tài liệu thiết kế): dấu chồng, ligature, RTL
//! trộn, emoji. Không cần GPU — shaping tách khỏi atlas có chủ đích.

use mong_render::text::{LineSpec, Shaper};

fn shaper() -> (Shaper, String) {
    let mut s = Shaper::new();
    let vi = s.add_font(include_bytes!("fonts/BeVietnamPro-Regular.ttf").to_vec());
    s.add_font(include_bytes!("fonts/NotoNaskhArabic-Regular.ttf").to_vec());
    let family = vi.first().expect("font phai co family").clone();
    (s, family)
}

fn spec(family: &str) -> LineSpec {
    LineSpec {
        font_size: 32.0,
        line_height: 40.0,
        max_width: 1600.0,
        family: family.to_string(),
    }
}

/// "ế" dựng bằng ba codepoint phải shape ra **một** glyph, không phải ba.
#[test]
fn dau_chong_tieng_viet_ghep_thanh_mot_glyph() {
    let (mut s, f) = shaper();
    let composed = s.shape("ế", &spec(&f));
    let decomposed = s.shape("e\u{0302}\u{0301}", &spec(&f));
    assert_eq!(composed.glyphs.len(), 1);
    assert_eq!(decomposed.glyphs.len(), 1, "dau chong phai ghep");
    assert!((composed.width - decomposed.width).abs() < 0.5);
}

/// Glyph ghép phủ trọn khoảng byte của cả cụm — nền tảng cho `visible()`.
#[test]
fn glyph_ghep_phu_tron_khoang_byte() {
    let (mut s, f) = shaper();
    let text = "e\u{0302}\u{0301}"; // 5 byte
    let line = s.shape(text, &spec(&f));
    let g = line.glyphs[0];
    assert_eq!((g.byte_start, g.byte_end), (0, text.len()));
}

/// Typewriter lộ byte đầu tiên của cụm → cả cụm hiện, không hiện nửa chữ.
#[test]
fn cum_hien_tron_ven_ngay_tu_byte_dau() {
    let (mut s, f) = shaper();
    let text = "n\u{0065}\u{0302}\u{0301}u";
    let line = s.shape(text, &spec(&f));
    assert_eq!(line.visible(0).count(), 0);
    assert_eq!(line.visible(1).count(), 1, "moi hien 'n'");
    assert_eq!(line.visible(2).count(), 2, "'e' toi luot -> ca 'ế' hien");
    assert_eq!(line.visible(text.len()).count(), 3);
}

/// BiDi: chữ Ả Rập trộn trong câu Việt vẫn shape, và typewriter lộ theo
/// thứ tự **logic** (thứ tự gõ), không theo thứ tự thị giác.
#[test]
fn rtl_tron_shape_va_lo_theo_thu_tu_logic() {
    let (mut s, f) = shaper();
    // U+0645 U+0631 U+062D U+0628 U+0627 = "marhaba".
    const MARHABA: &str = "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}";
    let text = format!("Xin chào {MARHABA} nhé");
    let line = s.shape(&text, &spec(&f));
    assert!(line.glyphs.len() > 10);

    let prefix = "Xin chào ".len();
    let visible: Vec<_> = line.visible(prefix).collect();
    assert!(
        visible.iter().all(|g| g.byte_start < prefix),
        "chua den luot chu A Rap"
    );
    // Chữ Ả Rập nằm bên phải chữ "Xin chào" dù chính nó chạy phải-sang-trái.
    let arab_x = line
        .glyphs
        .iter()
        .filter(|g| g.byte_start >= prefix && g.byte_start < prefix + MARHABA.len())
        .map(|g| g.x)
        .fold(f32::MAX, f32::min);
    let viet_max_x = visible.iter().map(|g| g.x).fold(0.0, f32::max);
    assert!(arab_x > viet_max_x);
}

/// Nối chữ Ả Rập đổi *hình dạng* glyph, không đổi *số* glyph: `م` đứng đầu
/// từ là dạng initial, đứng một mình là dạng isolated — hai glyph id khác nhau.
/// Đây là thứ `Shaping::Basic` sẽ làm hỏng.
#[test]
fn a_rap_chon_dang_theo_ngu_canh() {
    let (mut s, f) = shaper();
    // U+0645 U+0631 U+062D U+0628 U+0627 — escape để IDE và copy-paste
    // không lén chèn ZWJ hay đổi sang ligature dựng sẵn.
    let trong_tu = s.shape("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}", &spec(&f));
    let dung_mot_minh = s.shape("\u{0645}", &spec(&f));

    let dau_tu = trong_tu
        .glyphs
        .iter()
        .find(|g| g.byte_start == 0)
        .expect("chu cai dau");
    assert_ne!(
        dau_tu.cache_key.glyph_id, dung_mot_minh.glyphs[0].cache_key.glyph_id,
        "dang initial phai khac dang isolated"
    );

    for g in &trong_tu.glyphs {
        println!(
            "glyph={:?} bytes={}..{} x={}",
            g.cache_key.glyph_id, g.byte_start, g.byte_end, g.x
        );
    }
}

/// Bất biến của `visible()`, đúng bất kể font có gộp ligature hay không:
/// glyph là nguyên tử. Lộ được byte đầu của một glyph ⇒ glyph đó hiện trọn,
/// và số glyph hiện chỉ tăng đơn điệu theo byte. Không bao giờ nửa chữ.
///
/// Cố ý *không* khẳng định lam-alef ra mấy glyph — đó là chuyện của bảng
/// `rlig` trong font, không phải hợp đồng của engine.
#[test]
fn glyph_la_nguyen_tu_tren_chuoi_a_rap() {
    let (mut s, f) = shaper();
    // U+0644 U+0627 (lam, alef) + space + "marhaba".
    let text = "\u{0644}\u{0627} \u{0645}\u{0631}\u{062D}\u{0628}\u{0627}";
    let line = s.shape(text, &spec(&f));

    for g in &line.glyphs {
        assert!(g.byte_start < g.byte_end, "khoang byte phai khong rong");
        assert!(g.byte_end <= text.len());
        // Lộ đúng byte đầu của glyph này là đủ để nó hiện.
        assert!(
            line.visible(g.byte_start + 1)
                .any(|v| v.byte_start == g.byte_start),
            "glyph tai byte {} phai hien khi byte do da lo",
            g.byte_start
        );
    }

    let mut truoc = 0;
    for n in 0..=text.len() {
        let sau = line.visible(n).count();
        assert!(sau >= truoc, "so glyph hien khong duoc giam");
        truoc = sau;
    }
    assert_eq!(line.visible(text.len()).count(), line.glyphs.len());
}

#[test]
fn khong_co_font_he_thong_nao_lot_vao() {
    let s = Shaper::new();
    assert_eq!(s.face_count(), 0, "db phai rong luc khoi tao");

    let (s, _) = shaper();
    assert_eq!(s.face_count(), 2, "dung hai font test, khong hon");
}

/// Emoji ZWJ: shaping không panic, không nổ thành nhiều glyph rác.
/// (Vẽ ra được hay chưa là chuyện của atlas — glyph màu còn là phần mở.)
#[test]
fn emoji_zwj_khong_lam_shaping_no_tung() {
    let (mut s, f) = shaper();
    let line = s.shape("👨‍👩‍👧 xin chào", &spec(&f));
    assert!(!line.glyphs.is_empty());
    assert!(line.width > 0.0);
}

/// Ngắt dòng đẩy chữ xuống dòng dưới, `y` phải tăng — chứng minh vì sao
/// không được shape lại chuỗi đã cắt mỗi frame.
#[test]
fn ngat_dong_day_chu_xuong_dong_duoi() {
    let (mut s, f) = shaper();
    let mut sp = spec(&f);
    sp.max_width = 200.0;
    let line = s.shape("Nắng chiều đổ nghiêng qua ô cửa kính quán cà phê.", &sp);
    let ys: Vec<f32> = line.glyphs.iter().map(|g| g.y).collect();
    let max_y = ys.iter().cloned().fold(0.0, f32::max);
    assert!(max_y > 0.0, "phai co it nhat hai dong");
    assert!(line.height >= max_y);
}
