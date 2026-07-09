# M3 — Render + audio, chạy desktop

**Trạng thái: ĐÓNG** (2026-07-09)

## DoD đối chiếu

| Yêu cầu | Kết quả |
|---|---|
| Demo chạy 60fps cửa sổ desktop | 164.8 fps (màn 165Hz, vsync). CPU 0.66–1.4 ms/frame, ~6% ngân sách ở 60Hz |
| Tiếng Việt hiển thị đúng grapheme | Dấu chồng đúng chỗ; typewriter cắt theo grapheme cluster, `ế` không bao giờ hiện nửa chừng |
| Transition fade | Blend hai lớp nền, một pipeline |

Bộ test chuỗi hiểm (rủi ro số 4, mục 14 tài liệu thiết kế): 9 test trong
`crates/mong-render/tests/chuoi_hiem.rs`, chạy không cần GPU.

## Quyết định phát sinh

**RFC-001 — `manifest.json` là file riêng.** Metadata trình diễn (scene,
character, asset, font) tách khỏi `Story`, mang `format_version` riêng. Lý do:
`mong-core` không được biết bg/sprite là gì. Kéo theo: sân khấu không nằm
trong snapshot của core; `mong-runtime` giữ ngăn xếp `Stage` song song.

**Manifest v2 — `strings`.** Tên nhân vật/cảnh là key trỏ vào
`manifest.strings[locale]`, không phải văn bản thẳng (localization-first).
Miền key này tách khỏi bảng chuỗi nội dung sinh từ DSL; hợp nhất lúc tra cứu
qua `Catalog::merge_table`, key nội dung thắng khi trùng.

**`Shaper` không nạp font hệ thống.** `FontSystem::new()` và
`new_with_fonts()` **đều** gọi `load_system_fonts()`. Dùng
`new_with_locale_and_db` với db rỗng. Nếu không: chữ trên máy tác giả khác
chữ trên máy người chơi, và golden test chữ vô nghĩa.

**Sàn WebGL2 thi hành ngay trên desktop.** `required_limits =
downlevel_webgl2_defaults()`. Vi phạm nổ ở M3, không đợi mở Safari ở M4.

## Nợ chuyển sang M4

1. **`manifest.fonts[locale]` là chain, `LineSpec.family` chỉ một tên.**
   Fallback hiện chạy theo thứ tự nạp vào `FontSystem`, chung cho mọi locale.
   Đủ cho vi+en, **vỡ khi thêm CJK** (chữ Nhật ra chữ Trung). Sửa trước khi
   thêm locale thứ ba.
2. Font emoji + atlas RGBA cho glyph màu (`SwashContent::Color` đang bị bỏ).
3. Cache `shaped` không có trần — truyện 5000 dòng giữ 5000 `ShapedLine`.
   Cần LRU.
4. Rasterize glyph theo lô mỗi frame: dòng thoại mới tốn một frame ~3 ms CPU
   (shape + rasterize ~30 glyph). Không rớt frame ở 60Hz lẫn 165Hz. Ưu tiên
   thấp — làm khi có dòng thoại dài hoặc font CJK (glyph lớn hơn nhiều).
5. `mong-cli` chưa nạp manifest — tên nhân vật trong text-mode vẫn là id.
6. `project.rs` copy logic sidecar của CLI. Bản copy **thứ hai**. Bản thứ ba
   thì tách `mong-project` (dự kiến M6).
7. Con trỏ chờ; hit-test chuột cho lựa chọn (cần đảo ngược `letterbox`, và
   input abstraction cho mobile chưa quyết — M7).

## Ghi để khỏi điều tra lại

- U+0644 U+0627 (lam + alef) **không** gộp ligature với Noto Naskh qua cosmic-text 0.12.
  Không phải bug của engine.
- Chuỗi test không phải Latin viết bằng escape `\u{...}` kèm comment tên
  codepoint. Ký tự thật trong file nguồn có thể mang ZWJ/RLM vô hình hoặc dạng
  dựng sẵn (U+FEFB) tuỳ nguồn copy — test sẽ đo nhầm thứ mà không ai biết.
- `get_current_texture()` chặn tới nhịp quét, y như `present()`. Đo frame time
  quanh nó là đo giấc ngủ.
