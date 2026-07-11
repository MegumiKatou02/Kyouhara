# spec-devlink — Hot reload editor ↔ runtime (M6)

**Trạng thái: NHÁP** (M6.0)

## Kênh
Runtime desktop build với feature `dev` mở WebSocket server
`ws://127.0.0.1:<port>`; editor là client. Thông điệp JSON, một object/frame.

## Thông điệp editor → runtime
| type | payload | ngữ nghĩa |
|---|---|---|
| `patch_strings` | `{locale, entries: {key: text}}` | Cập nhật catalog, re-resolve dòng/lựa chọn đang hiện. Không đụng VM. Đường DoD "< 1s". |
| `patch_node` | `{node: <IR Node JSON>, strings?: {...}}` | Thay node theo id + full-session replay (xem dưới). Node id chưa tồn tại → lỗi, không tự thêm. |
| `patch_story` | `{story, strings}` | Thêm/xoá node: nạp lại story, replay log. |

## Thông điệp runtime → editor
`{type: "ok"}` / `{type: "replay_stopped", at: <số input đã áp>, reason}` /
`{type: "error", msg}` / `{type: "node_entered", node}` (bắn theo VmEvent
NodeEntered — editor highlight node đang chạy, spec-ir đã hứa).

## Ngữ nghĩa replay (quyết định phát sinh M6)
Runtime ghi log sự kiện điều khiển từ `start()`:
`Input(Advance|Choose(i)|Rollback)` và `WaitElapsed` (wait hết giờ trong
tick). Patch node/story → dựng Vm mới từ story đã vá, replay log tuần tự,
wait coi như hết giờ ngay. Tính xác định của VM (mục 5 tài liệu thiết kế)
bảo đảm state tái tạo đúng. Replay vấp (vd. Choose(i) khi chỉ còn <i+1 arm)
→ dừng tại đó, giữ trạng thái hợp lệ cuối, báo `replay_stopped`.

Hạn chế: replay không tick typewriter → `on_type` không bắn (tương đương
người chơi skip mọi dòng — hành vi skip đã chốt ở M5). Hook khác bắn lại
bình thường và cho cùng kết quả nhờ plugin xác định (`no_time`).
