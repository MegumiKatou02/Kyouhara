# Hợp đồng entry của `.mongpack`

**Trạng thái: ĐÃ DUYỆT; §5 đã chốt trong M5**

Bố cục nhị phân (magic, header, khung entry) là hợp đồng của `mong-assets`,
tài liệu ngay trong `crates/mong-assets/src/lib.rs`. Tài liệu này định nghĩa
tầng trên: **entry nào phải có, tên gọi là gì, thứ tự ra sao** — hợp đồng của
`mong-project`, thứ mà mọi công cụ đọc/ghi gói phải tuân theo.

Đổi bất cứ điều gì ở đây = tăng `FORMAT_VERSION` + viết migration.

## 1. Bảng entry

| Tên | `EntryKind` | Bắt buộc | Nội dung |
|---|---|---|---|
| `manifest.json` | `Meta` (0) | ✅ | `Manifest` serialize JSON. Có `format_version` **riêng** (hiện 2), độc lập với `FORMAT_VERSION` của gói. |
| `story.ir` | `StoryIr` (1) | ✅ | `Story` serialize JSON. Chỉ chứa key, không chứa văn bản. |
| `strings/<locale>.json` | `Strings` (2) | ⬜ | Bảng chuỗi **miền nội dung** của một locale. **Không** gồm `manifest.strings`. |
| `assets/<asset_id>` | `Image` (3) / `Audio` (4) / `Font` (6) | ⬜ | Bytes thô, chưa giải mã. Tên là **asset id**, không phải `Asset.path`. |
| plugins/<plugin_id>.rhai | Plugin (5) | ⬜ | Mã nguồn rhai UTF-8. Tên là plugin id, thứ tự nạp theo BTreeMap của id.

Thiếu `manifest.json` hoặc `story.ir` → `ProjectError::MissingEntry`.

### Vì sao tên asset là id, không phải path

`Asset.path` là bố cục **thư mục dự án** — chuyện của tác giả. Runtime tra
asset theo id (`manifest.scenes[x].bg` là id). Gói mang id để runtime không
phải biết dự án từng được sắp xếp thế nào. `path` vẫn nằm trong
`manifest.json` để công cụ ngược dòng, nhưng gói không dùng tới.

### Vì sao `manifest.strings` không nằm trong `strings/`

Hai miền key tách biệt (tài liệu thiết kế mục 4): `mong-cli fmt` quản miền nội
dung, không bao giờ đụng manifest. Hợp nhất là việc của **lúc tra cứu**
(`Loaded::catalog()`), không phải lúc lưu. Trộn khi pack thì lint sẽ báo
`char.lan` là "key mồ côi" — đúng bug đã gặp và sửa ở M4.

## 2. Thứ tự entry — xác định

`to_pack` ghi theo thứ tự cố định:

1. `manifest.json`
2. `story.ir`
3. `strings/<locale>.json` — theo thứ tự `BTreeMap` của locale
4. `assets/<id>` — theo thứ tự `BTreeMap` của `manifest.assets`
5. `plugins/<id>.rhai` — theo thứ tự `BTreeMap` của plugin id

Hệ quả: **cùng dự án cho ra cùng byte.** `pack` idempotent, CI so được hash,
diff gói có nghĩa. Test khoá: `mong-project/tests/round_trip.rs::pack_xac_dinh`.

Đọc thì không phụ thuộc thứ tự — `load_pack` duyệt và phân loại theo `kind`.

## 3. Quy tắc khi đọc

- Entry `Meta` có tên khác `manifest.json`: **bỏ qua**.
- Entry `Plugin`: tên không khớp plugins/<x>.rhai → lấy nguyên tên/bỏ đuôi khoan dung như asset; nội dung không phải UTF-8 → lỗi Json. Kind lạ (Unknown): bỏ qua
- Entry trùng tên: cái sau ghi đè cái trước. Không phải lỗi. `to_pack` không
  bao giờ sinh ra tình huống này.
- Tên `Strings` không khớp `strings/<x>.json`: lỗi `Json`.
- Asset có tên không bắt đầu bằng `assets/`: lấy nguyên tên làm id (khoan dung).

## 4. Ai được thêm entry mới

Thêm **tên** entry mới dưới một `EntryKind` đã có (vd. `plugins/rung.rhai`
kiểu `Plugin`): không cần tăng `FORMAT_VERSION`. Runtime cũ bỏ qua nó theo
quy tắc mục 3.  

Thêm **`EntryKind`** mới: runtime cũ đọc và bỏ qua (Unknown), nhưng tài liệu này vẫn phải cập nhật.

## 5. Câu hỏi mở

### 5.1 ⚠ Tương thích ngược chưa hiện thực

Tài liệu thiết kế mục 4 hứa: *"runtime đọc được mongpack có `formatVersion` cũ
hơn trong cùng major"*. `read_pack` hiện đòi bằng đúng:

```rust
if ver != FORMAT_VERSION { return Err(PackError::BadVersion(ver)); }
```

`FORMAT_VERSION = 0` nên chưa ai đau. Phải chốt trước khi tăng lên 1:
- (a) `ver > FORMAT_VERSION` → lỗi; `ver < FORMAT_VERSION` → migrate;
- (b) tách major/minor trong `u32`;
- (c) bỏ lời hứa, viết lại mục 4.

### 5.2 ⚠ `EntryKind` lạ làm hỏng cả gói

`load_pack` có comment *"gói mới vẫn chạy trên runtime cũ"*. Sai: `read_pack`
từ chối byte kind lạ bằng `Corrupt`, trước khi `load_pack` kịp thấy gì.

Muốn giữ lời hứa đó, `read_pack` phải trả entry kind-lạ dưới dạng
`EntryKind::Unknown(u8)` thay vì chết. Khung entry đã có `raw_len`/`comp_len`
nên **bỏ qua được an toàn** — chỉ là quyết định chưa được đưa ra.

Cả hai câu hỏi nên chốt trong M5, vì M5 là mốc đầu tiên ghi `Plugin` vào gói.

-> 5.1 và 5.2: đổi ⚠ thành ĐÃ CHỐT (M5.0), ghi phương án (a) và EntryKind::Unknown(u8), giữ nguyên phần lập luận cũ làm ngữ cảnh.

## 6. Codec

Header có trường `codec u8`. `CODEC_DEFLATE = 1` là cái duy nhất được hiện
thực. Bảng quyết định (mục 12) chọn zstd; chỗ dành sẵn là `codec = 2`.

Tỉ lệ nén đo trên demo (17%) **không có giá trị tham khảo**: WAV placeholder là
sine lặp chu kỳ chính xác, PNG placeholder là màu phẳng. Asset thật nén ~0%.
Quyết định DEFLATE-vs-zstd phải đo trên asset thật (M6).
