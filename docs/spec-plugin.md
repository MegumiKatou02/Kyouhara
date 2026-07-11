# Spec plugin nội dung — Mộng Engine (v1, M5)

**Trạng thái: ĐÃ DUYỆT** (M5, 2026-07-11 — đã thực chứng bằng 3 plugin demo + test host/runtime)**

Hợp đồng của tầng-một plugin (mục 7 tài liệu thiết kế): hook, payload, action,
quy tắc xác định, cô lập lỗi. Hợp đồng định nghĩa **bằng dữ liệu** — mọi
payload/giá trị là serde value (Int/Bool/Str/Map/Array). rhai là backend v1;
không mục nào dưới đây được viện dẫn chi tiết riêng của rhai ngoài mục
"Ràng buộc backend rhai".

## 1. Đóng gói và thứ tự

- Dự án: `plugins/<id>.rhai`. Gói: entry `plugins/<id>.rhai` kiểu `Plugin`.
- Thứ tự nạp, thứ tự bắn hook, thứ tự xâu chuỗi filter: theo `BTreeMap`
  của id. Thứ tự là một phần của tính xác định — đổi tên file là đổi hành vi,
  chấp nhận (giống Ren'Py init order theo tên file).

## 2. Bảng hook

| Hook | Payload | Lớp | Bắn khi |
|---|---|---|---|
| `on_game_start` | `{}` | logic | sau `vm.start()`, trước khi áp event đầu |
| `on_node_enter` | `{node}` | logic | mỗi `VmEvent::NodeEntered` tươi |
| `on_line_show` | `{speaker?, key, text}` | logic | dòng thoại bắt đầu hiển thị (`text` = sau tra catalog **và** sau filter) |
| `on_type` | `{grapheme, index, total}` | trình diễn | typewriter lộ thêm một grapheme |
| `on_choice_picked` | `{index, key}` | logic | sau `vm.choose(i)` thành công, trước khi áp event |
| `on_game_end` | `{}` | logic | `VmEvent::Ended` tươi |

**Filter `filter_text`:** `{speaker?, key, text} → text mới`. Chạy khi dựng
dòng thoại, xâu chuỗi qua các plugin theo thứ tự id. Filter phải **thuần túy**
(cùng input + cùng biến → cùng output): nó chạy lại khi rollback dựng lại
dòng, kết quả phải tái lập.

## 3. Action (ctx API)

| Action | Lớp cho phép | Ngữ nghĩa |
|---|---|---|
| `get_var(name) → value` | mọi nơi kể cả filter | đọc biến của **VM** |
| `set_var(name, value)` | hook logic | ghi biến của VM → tự nằm trong snapshot, rollback khôi phục đúng |
| `goto_node(node)` | hook logic | xếp hàng; áp ở điểm dừng kế tiếp của VM (AwaitAdvance/AwaitChoice). Node không tồn tại → log, bỏ qua |
| `play_sfx(asset_id)` | hook logic + `on_type` | đẩy `AudioCmd::Sfx` |
| `shake(bien_do_px, ms)` | hook logic + `on_type` | overlay rung: offset sân khấu decay tuyến tính về 0 |
| `set_cps(so_grapheme_moi_giay)` | hook logic + `on_type` | đổi tốc độ typewriter, hiệu lực tới khi đổi lại |

Hook **trình diễn** (`on_type`) gọi action lớp logic (`set_var`, `goto`) →
host log cảnh báo và **bỏ qua** action đó: `on_type` bắn theo nhịp gõ chữ
(phụ thuộc dt của shell), cho nó ghi biến là phá tính xác định của core.

## 4. Lệnh `ext`

Plugin đăng ký lệnh bằng cách khai báo hàm `ext_<command>(args)`.
`VmEvent::Ext{command, args}` → host gọi hàm tương ứng của **mọi** plugin có
khai báo (theo thứ tự id). Không plugin nào có → runtime log rồi bỏ qua
(spec-ir, không bao giờ là lỗi cứng). `args` là serde value nguyên vẹn từ IR.

## 5. Tính xác định và rollback

1. Biến plugin ghi đi qua VM → nằm trong snapshot. Plugin **không có state
   riêng ngoài VM**; plugin cần nhớ gì thì ghi vào biến (quy ước tiền tố
   `_p_<id>_` để lint sau này phân miền — chưa bắt buộc ở M5).
2. Hook **không bắn khi rollback replay**. Snapshot đã chứa hậu quả của hook
   lần đầu; bắn lại là double-apply. Runtime phân biệt event tươi / event
   replay khi gọi host.
3. Filter chạy lại khi replay (nó là một phần của "vẽ dòng thoại"), vì thế
   phải thuần túy — xem mục 2.
4. Host không cấp cho plugin: thời gian thực, random ngoài VM, I/O, mạng.
   Plugin cần random → dùng biến + lệnh `rand` của IR, hoặc chờ API
   `ctx.rand` nối vào PRNG của VM (ngoài phạm vi M5).

## 6. Cô lập lỗi

- Lỗi biên dịch một plugin: log, plugin đó bị vô hiệu, các plugin khác và
  game chạy tiếp.
- Lỗi runtime trong một hook call: log kèm plugin id + tên hook, kết quả
  hook đó bị bỏ, chuỗi filter dùng text của plugin liền trước, game chạy tiếp.
- Ngân sách: giới hạn số phép tính mỗi lần gọi hook (chống vòng lặp vô hạn);
  vượt = lỗi runtime của hook đó, xử lý như trên.

## 7. Ràng buộc backend rhai (v1)

- Mỗi file `.rhai` là một plugin. Hook = hàm top-level trùng tên hook;
  `filter_text` trả về String; `ext_<command>` nhận một tham số args.
- Payload map → tham số là object map của rhai; value sang kiểu tự nhiên
  (Int→i64, Bool→bool, Str→String).
- Action = hàm host đăng ký sẵn trong scope, không phải method của object
  nào — giữ bề mặt tối giản để backend sau (Lua/WASM) map 1:1.
- Sandbox: tắt `eval`, không module fs/net, đặt `max_operations`,
  `max_call_levels`, giới hạn kích thước string/array/map.
- rhai dành sẵn từ khoá `goto`, nên binding của action Goto tên là
  `goto_node`. Hợp đồng trung lập vẫn gọi action này là "goto" — backend
  khác được đặt tên tự nhiên của nó miễn ngữ nghĩa giữ nguyên.

## 8. Ba plugin mẫu (DoD M5)

| Plugin | Dùng gì | Chứng minh |
|---|---|---|
| `chen_bien` | `filter_text` + `get_var`: thay `{ten_bien}` trong thoại bằng giá trị biến | filter thuần túy, tái lập qua rollback |
| `rung` | `ext_rung(args)` + `shake`: DSL viết `ext rung {"ms": 300}` | đường ext end-to-end + overlay |
| `go_chu` | `on_type` + `play_sfx`: tiếng gõ mỗi grapheme | hook trình diễn + ranh giới action |
