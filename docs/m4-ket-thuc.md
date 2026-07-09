# M4 — Web WASM

**Trạng thái: ĐANG MỞ** (mở 2026-07-10)

## DoD
- [ ] Cùng demo chạy Chrome / Firefox / Safari
- [ ] Bundle < 5 MB gzip, chưa tính assets

## Tiến độ
- [x] M4.1 `mong-project` + `mong-cli pack`; desktop nạp được cả thư mục lẫn gói
- [ ] M4.2 `mong-audio` trên wasm
- [ ] M4.3 `shells/web`
- [ ] M4.4 CI wasm + trần bundle size
- [ ] M4.5 Kiểm chứng ba trình duyệt

## Quyết định phát sinh

**`Loaded` giữ bảng chuỗi nội dung thô, không giữ `Catalog`.** Giữ Catalog đã
merge thì `manifest.strings` bị nướng vào entry `Strings` lúc pack rồi merge
lần hai lúc load — bẩn miền key mà spec mục 4 cố ý tách. `Loaded::catalog()`
dựng lúc cần.

**Tên entry trong mongpack là hợp đồng.** `manifest.json`, `story.ir`,
`strings/<locale>.json`, `assets/<asset_id>` (tên = **id**, không phải path).
Thứ tự entry cố định theo `BTreeMap` ⇒ cùng dự án cho ra cùng byte.

**`load_dir` cảnh báo asset thiếu file, `to_pack` từ chối.** Dev chạy câm vẫn
hơn không chạy; gói hỏng phải lộ lúc build, không phải lúc người chơi mở.

**Demo chuyển `.ogg` → `.wav`.** Safari không chắc nuốt Ogg Vorbis qua
`decodeAudioData`. Quyết định về *dữ liệu demo*, không về engine — engine nhận
mọi định dạng backend giải mã được.

## Nợ mới (chuyển sang M6 trừ khi ghi khác)

1. `gen_placeholders` nằm nhầm nhà (`shells/desktop/examples/`) — nó sinh asset
   cho demo dùng chung, thuộc về `tools/`.
2. Demo dùng WAV thô, không nén. Transcode + nén asset lúc `pack` là việc của
   pipeline M6.
3. `mong-cli run`/`lint` chưa dùng `mong-project` — vẫn giữ `load_input` /
   `build_catalog` riêng, nên text-mode vẫn hiện id thay vì tên nhân vật
   (nợ M3 số 5). PR ngay sau M4.1.
4. Font 131 KB chưa subset. Chỉ giữ glyph có trong bảng chuỗi → cắt 80–90%.
   Nặng nhất trong đường tải tới-chữ-đầu-tiên. M6.

## Ghi để khỏi điều tra lại

- `os error 3` trên Windows = **path** not found; `os error 2` = **file** not
  found. Bọc mọi `io::Error` kèm đường dẫn, nếu không thì đoán mò.
- Item (`struct`, `impl`, `use`…) khai báo trong thân hàm là hợp lệ trong Rust.
  Một file bị dán chồng vào chính nó **vẫn compile**, chỉ sinh `dead_code`.
  Thứ bắt được là `-D warnings`, không phải `cargo fmt --check` hay test.
- Tỉ lệ nén của demo (17%) là ảo: WAV sine lặp chu kỳ chính xác + PNG phẳng.
  Asset thật nén ~0%. Đừng dùng số này để bàn DEFLATE vs zstd.
