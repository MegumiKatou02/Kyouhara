//! mong-i18n — bảng chuỗi theo locale + fallback (phạm vi M2).
//!
//! Thay bảng chuỗi phẳng `--strings` của M1 (spec-mongscript mục 11):
//! mỗi locale một bảng `key → văn bản`; tra cứu rơi về defaultLocale khi
//! locale yêu cầu thiếu key. Plural và font map thuộc mốc sau.
//!
//! Crate này thuần dữ liệu — không I/O, không phụ thuộc platform: ai nạp
//! file (CLI, shell, editor) tự đọc rồi đưa map vào. Nhờ vậy dùng được ở
//! mọi tầng mà không phá ranh giới module.

use std::collections::BTreeMap;

/// Bảng chuỗi của một locale.
pub type Table = BTreeMap<String, String>;

/// Kết quả tra một key ở một locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved<'a> {
    /// Có ở đúng locale yêu cầu.
    Exact(&'a str),
    /// Thiếu ở locale yêu cầu, rơi về defaultLocale.
    Fallback(&'a str),
    /// Không có ở cả hai — lỗi dữ liệu, tầng trên quyết định cách hiển thị.
    Missing,
}

/// Tập bảng chuỗi của một dự án, tra cứu có fallback.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    default_locale: String,
    tables: BTreeMap<String, Table>,
}

impl Catalog {
    pub fn new(default_locale: impl Into<String>) -> Self {
        Catalog {
            default_locale: default_locale.into(),
            tables: BTreeMap::new(),
        }
    }

    /// Trộn thêm một bảng vào locale đã có (hoặc tạo mới). Key trùng thì
    /// bảng cũ thắng — bảng nội dung sinh từ nguồn là chân lý, metadata chỉ
    /// bù vào chỗ trống.
    pub fn merge_table(&mut self, locale: impl Into<String>, table: Table) {
        let e = self.tables.entry(locale.into()).or_default();
        for (k, v) in table {
            e.entry(k).or_insert(v);
        }
    }

    /// Ghi đè/chèn entry vào bảng của một locale — hot reload dev
    /// (spec-devlink, Runtime::patch_strings). Ngược với `merge_table`
    /// (bảng cũ thắng): ở đây bản mới thắng, vì editor gửi văn bản vừa
    /// sửa và mục đích là thay cái đang hiển thị. Chỉ đụng bản trong bộ
    /// nhớ của phiên preview — nguồn sự thật trên đĩa do editor tự ghi.
    pub fn update(&mut self, locale: impl Into<String>, entries: &Table) {
        let e = self.tables.entry(locale.into()).or_default();
        for (k, v) in entries {
            e.insert(k.clone(), v.clone());
        }
    }

    pub fn default_locale(&self) -> &str {
        &self.default_locale
    }

    /// Gắn (hoặc thay) bảng chuỗi của một locale.
    pub fn set_table(&mut self, locale: impl Into<String>, table: Table) {
        self.tables.insert(locale.into(), table);
    }

    pub fn has_locale(&self, locale: &str) -> bool {
        self.tables.contains_key(locale)
    }

    /// Các locale đang có bảng, theo thứ tự BTreeMap (xác định).
    pub fn locales(&self) -> impl Iterator<Item = &str> {
        self.tables.keys().map(String::as_str)
    }

    fn get(&self, locale: &str, key: &str) -> Option<&str> {
        self.tables.get(locale)?.get(key).map(String::as_str)
    }

    /// Bảng thô của một locale. `mong-cli` cần để chạy L022–L024.
    pub fn table(&self, locale: &str) -> Option<&Table> {
        self.tables.get(locale)
    }

    /// Tra key ở `locale`, fallback về defaultLocale (spec-mongscript mục 7).
    pub fn resolve(&self, locale: &str, key: &str) -> Resolved<'_> {
        if let Some(t) = self.get(locale, key) {
            return Resolved::Exact(t);
        }
        if locale != self.default_locale {
            if let Some(t) = self.get(&self.default_locale, key) {
                return Resolved::Fallback(t);
            }
        }
        Resolved::Missing
    }

    /// Tiện cho runner/renderer: `Missing` thì trả về chính key — thoại
    /// không bao giờ "biến mất", chỉ lộ key để tác giả thấy mà sửa.
    pub fn text_or_key<'a>(&'a self, locale: &str, key: &'a str) -> &'a str {
        match self.resolve(locale, key) {
            Resolved::Exact(t) | Resolved::Fallback(t) => t,
            Resolved::Missing => key,
        }
    }

    /// Key có ở defaultLocale nhưng thiếu ở `locale` — danh sách việc cho
    /// người dịch, và nguyên liệu cho lint cảnh báo bản dịch chưa đủ.
    pub fn missing_in(&self, locale: &str) -> Vec<&str> {
        let Some(default) = self.tables.get(&self.default_locale) else {
            return Vec::new();
        };
        let target = self.tables.get(locale);
        default
            .keys()
            .filter(|k| target.is_none_or(|t| !t.contains_key(*k)))
            .map(String::as_str)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let mut c = Catalog::new("vi");
        c.set_table(
            "vi",
            Table::from([
                ("a.l1".into(), "Xin chào".into()),
                ("a.l2".into(), "Tạm biệt".into()),
            ]),
        );
        c.set_table("en", Table::from([("a.l1".into(), "Hello".into())]));
        c
    }

    #[test]
    fn update_ban_moi_thang_va_tao_locale_neu_chua_co() {
        let mut c = catalog();
        c.update(
            "vi",
            &Table::from([
                ("a.l1".into(), "Chào buổi sáng".into()), // ghi đè
                ("a.l3".into(), "Câu mới".into()),        // chèn
            ]),
        );
        assert_eq!(c.resolve("vi", "a.l1"), Resolved::Exact("Chào buổi sáng"));
        assert_eq!(c.resolve("vi", "a.l3"), Resolved::Exact("Câu mới"));
        assert_eq!(c.resolve("vi", "a.l2"), Resolved::Exact("Tạm biệt")); // không đụng key khác
        c.update("ja", &Table::from([("a.l1".into(), "こんにちは".into())]));
        assert_eq!(c.resolve("ja", "a.l1"), Resolved::Exact("こんにちは"));
    }

    #[test]
    fn tra_dung_locale() {
        let c = catalog();
        assert_eq!(c.resolve("en", "a.l1"), Resolved::Exact("Hello"));
        assert_eq!(c.resolve("vi", "a.l1"), Resolved::Exact("Xin chào"));
    }

    #[test]
    fn thieu_thi_roi_ve_default() {
        let c = catalog();
        assert_eq!(c.resolve("en", "a.l2"), Resolved::Fallback("Tạm biệt"));
        // Locale chưa có bảng nào vẫn fallback được.
        assert_eq!(c.resolve("ja", "a.l1"), Resolved::Fallback("Xin chào"));
    }

    #[test]
    fn thieu_ca_hai_la_missing_va_text_or_key_tra_key() {
        let c = catalog();
        assert_eq!(c.resolve("en", "vang.l9"), Resolved::Missing);
        assert_eq!(c.text_or_key("en", "vang.l9"), "vang.l9");
        assert_eq!(c.text_or_key("en", "a.l2"), "Tạm biệt");
    }

    #[test]
    fn missing_in_liet_ke_viec_cho_nguoi_dich() {
        let c = catalog();
        assert_eq!(c.missing_in("en"), vec!["a.l2"]);
        assert_eq!(c.missing_in("ja"), vec!["a.l1", "a.l2"]);
        assert!(c.missing_in("vi").is_empty());
    }

    #[test]
    fn default_khong_tu_fallback_vao_chinh_no() {
        let mut c = Catalog::new("vi");
        c.set_table("vi", Table::new());
        assert_eq!(c.resolve("vi", "x"), Resolved::Missing);
    }
}
