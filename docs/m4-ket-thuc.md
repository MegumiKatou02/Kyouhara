# M4 — Web WASM

**Trạng thái: ĐÓNG** (2026-07-11, cùng M5)

DoD + M4.5: tick với chú thích Chrome/Firefox ✓; Safari chưa xác minh — điều khoản "sửa mục 8+14 cho khớp sự thật" đã kích hoạt, xem nợ 0.

## DoD
- [x] Cùng demo chạy Chrome / Firefox / Safari
- [x] Bundle < 5 MB gzip, chưa tính assets

## Tiến độ
- [x] M4.1 `mong-project` + `mong-cli pack`; desktop nạp được cả thư mục lẫn gói
- [x] M4.2 `mong-audio` trên wasm
- [x] M4.3 `shells/web`
- [x] M4.4 CI wasm + trần bundle size
- [x] M4.5 Kiểm chứng ba trình duyệt

**RFC-002 — `shells/common` (`mong-shell`).** Vòng lặp, cửa sổ, input, và việc
dịch `Stage`/`Line` sang `Sprite` dùng chung desktop và web. Lệch mục 3 (cây
thư mục không có `shells/common`) nhưng lệch **bổ sung**: chiều phụ thuộc vẫn
`shells → mong-runtime → lõi`, và `mong-runtime` vẫn không đụng wgpu. Không
làm thì shell web là bản copy thứ hai của 300 dòng `State` — đúng vết xe
`project.rs`.

**`ui.rs` nâng lên `mong-runtime::ui`,** đúng điều kiện file cũ tự đặt ra
("nếu shell thứ hai copy nguyên file này...").

**`unlock()` dời từ `State::new` sang `State::input`.** Cử chỉ đầu tiên mở
thiết bị. Desktop không đổi hành vi (input đầu tới ngay); web thì sống nhờ nó.

**Ép `Backends::GL` trên web.** wgpu 22 gửi limit `maxInterStageShaderComponents`
mà WebGPU spec đã bỏ → Chrome ≥ M135 từ chối `requestDevice`. WebGL2 là sàn
(mục 8) nên không mất gì, nhưng "có WebGPU thì đẹp hơn" tạm thời là lời hứa
suông. Nợ 8.

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

**`content_catalog()` tách khỏi `catalog()`.** Hợp nhất `manifest.strings` là
việc của *lúc tra cứu* (tài liệu thiết kế mục 4), không phải lúc lưu. Lint đọc
miền nội dung; runtime đọc bảng đã hợp nhất. Bug này do chính `lint <thu_muc>`
mới thêm tự phát hiện trong lần chạy đầu tiên.

## Nợ mới (chuyển sang M6 trừ khi ghi khác)
0. **Safari — KHÔNG đóng được bằng CI trong hạn tự đặt (M5). Chuyển thành
   hạn chế đã biết.** Đã làm: job safari + safari_check.py + cờ __mong_ready
   do Rust đặt trong frame() (đúng đề bài, giữ trong code). Kết quả điều tra:
   safaridriver mở trang nhưng execute/sync không thấy JS của trang thực thi
   — hỏng ở tầng bộ gá, DƯỚI tầng engine; engine chưa từng được đo trên
   Safari. Theo đúng điều khoản bên dưới, mục 8 + 14 tài liệu thiết kế đã
   sửa cho khớp sự thật. Việc còn lại ghi ở m5-ket-thuc nợ 5.

1. `gen_placeholders` nằm nhầm nhà (`shells/desktop/examples/`) — nó sinh asset
   cho demo dùng chung, thuộc về `tools/`.
2. Demo dùng WAV thô, không nén. Transcode + nén asset lúc `pack` là việc của
   pipeline M6.
3. `mong-cli run`/`lint` chưa dùng `mong-project` — vẫn giữ `load_input` /
   `build_catalog` riêng, nên text-mode vẫn hiện id thay vì tên nhân vật
   (nợ M3 số 5). PR ngay sau M4.1.
4. Font 131 KB chưa subset. Chỉ giữ glyph có trong bảng chuỗi → cắt 80–90%.
   Nặng nhất trong đường tải tới-chữ-đầu-tiên. M6.

5. `WebAudio` chưa dùng bus `voice`, chưa có loop point cho nhạc có intro
   (mục 9 tài liệu thiết kế). `AudioBufferSourceNode::set_loop_start/end` làm
   được.
6. `sfx` xếp hàng qua `pending_sfx` kể cả sau `unlock`: buffer chưa giải mã
   xong thì âm phát trễ thay vì bị bỏ. Đúng cho VN; xem lại nếu M5 có sfx
   nhịp nhanh.

7. `cosmic-text` kéo `sys-locale` dù `Shaper` truyền locale cứng `"en-US"`.
   Xem có feature nào tắt được — vài KB.
8. Ép `Backends::GL` trên web. wgpu 22 gửi limit `maxInterStageShaderComponents`
   mà WebGPU spec đã bỏ → Chrome ≥ M135 từ chối `requestDevice`. Nâng wgpu (24+)
   để mở lại đường WebGPU. Không chặn DoD M4 (WebGL2 là sàn, mục 8), nhưng câu
   "có WebGPU thì đẹp hơn" hiện là lời hứa suông.

9. ~~`wasm-opt` chưa vào pipeline.~~ **Đóng bằng phép đo (2026-07-10).** `-Oz`
   → 1.351.300 B gzip; `-Os` → 1.349.567 B; không dùng gì → 1.320.898 B. Cắt
   11% byte thô nhưng xáo trộn code, deflate ăn ít đi. Thứ người chơi tải là
   gzip. Không đưa vào CI. Lệnh đo lại nằm trong `build.ps1`.

10. `naga` (compiler WGSL) nằm trong bundle dù shader là hằng số. Cắt được nếu
    ngân sách chật.

11. Font emoji + atlas RGBA cho glyph màu (`SwashContent::Color` đang bị bỏ).
12. Cache `shaped` không có trần — truyện 5000 dòng giữ 5000 `ShapedLine`. LRU.
13. Rasterize glyph theo lô mỗi frame. Ưu tiên thấp.
14. Con trỏ chờ; hit-test chuột cho lựa chọn (cần đảo ngược `letterbox`; input
    abstraction cho mobile chưa quyết — M7).
15. Một dòng trộn hai script vẫn dùng một font. Cần phân đoạn theo script +
    `AttrsList` — M5.
## DoD đối chiếu

| Yêu cầu | Kết quả |
|---|---|
| Chrome | ✅ chữ, fade, crossfade, resize, rollback, audio-on-first-click |
| Firefox | ✅ |
| Safari | ⚠ chưa xác minh được (bộ gá WebDriver hỏng — m5-ket-thuc nợ 5) |
| Bundle < 5 MB gzip | ✅ 1.320.898 B (25% trần), CI canh |

**Safari: hoãn có chủ đích, không phải đạt.** Không có phần cứng macOS; job
`macos-latest` + `safaridriver` chưa viết. `Backends::GL` (nợ 8) mới ép ở M4.3
nên Safari chưa từng chạy qua đường code hiện tại — kể cả một lần.

Đây là **rủi ro số 2 của mục 14**, và cách phòng mà tài liệu thiết kế chỉ định
("test Safari trong CI của M4") chưa được thực hiện. Mục 13 nói không sang mốc
sau khi mốc trước chưa xanh CI; ta đang cố ý phá luật đó một lần, có ghi chép.

**Hạn chót tự đặt: trước khi M5 đóng.** Nếu tới đó vẫn chưa chạy Safari, coi
như engine không hỗ trợ Safari và phải sửa mục 8 + mục 14 của tài liệu thiết
kế cho khớp sự thật — chứ không để lời hứa treo.


## Ghi để khỏi điều tra lại

- `os error 3` trên Windows = **path** not found; `os error 2` = **file** not
  found. Bọc mọi `io::Error` kèm đường dẫn, nếu không thì đoán mò.
- Item (`struct`, `impl`, `use`…) khai báo trong thân hàm là hợp lệ trong Rust.
  Một file bị dán chồng vào chính nó **vẫn compile**, chỉ sinh `dead_code`.
  Thứ bắt được là `-D warnings`, không phải `cargo fmt --check` hay test.
- Tỉ lệ nén của demo (17%) là ảo: WAV sine lặp chu kỳ chính xác + PNG phẳng.
  Asset thật nén ~0%. Đừng dùng số này để bàn DEFLATE vs zstd.

- `fetch` không ném khi 404. Phải kiểm `res.ok`, nếu không HTML lỗi sẽ đi
  thẳng vào `read_pack` và báo `BadMagic` ở tận Rust.
- `cargo check` sinh metadata stub; `cargo build` sau đó gặp stub thì rustc kêu
  "only metadata stub found for rlib dependency core". Nguyên nhân thường là
  rust-analyzer dùng chung `target/`. Đặt `rust-analyzer.cargo.targetDir: true`.
- Script build phải kiểm `$LASTEXITCODE` sau mỗi lệnh ngoài, nếu không nó in
  số byte của bản build trước và bạn tin.

- `Loaded::catalog()` hợp nhất `manifest.strings` vào bảng nội dung. Đúng cho
  hiển thị, **sai cho lint**: key metadata thành "mồ côi". Tài liệu thiết kế
  mục 4 nói "hợp nhất lúc tra cứu" — không phải lúc lưu. Hai getter:
  `content_catalog()` để lint, `catalog()` để chạy.
