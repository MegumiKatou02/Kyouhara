# Spec tập lệnh IR — Mộng Engine (v1)

Tài liệu này là hợp đồng của IR: mọi frontend (editor trực quan, DSL MộngScript,
JSON dự án) biên dịch về IR này, và mọi backend (runtime desktop/web/mobile,
text-mode runner, bộ lint) chỉ được hiểu IR theo đúng ngữ nghĩa ghi ở đây.
Mã nguồn tham chiếu: `crates/mong-core/src/ir.rs` và `vm.rs`; mỗi quy tắc
ngữ nghĩa dưới đây đều có test tương ứng trong `mong-core` hoặc
`mong-assets/tests/roundtrip.rs`.

## Mô hình thực thi

Một cốt truyện (`Story`) gồm các `Node`; mỗi node là một dãy lệnh chạy tuần tự.
Máy ảo là máy trạng thái: `Idle → Running → (AwaitAdvance | AwaitChoice) →
Running → … → Ended`. Ở `Running`, VM thực thi liên tục và gom các
`VmEvent` (sự kiện trình diễn); gặp lệnh cần chờ thì dừng, trả toàn bộ event
đã gom cho tầng runtime, và tự chụp **snapshot**. Snapshot là đơn vị của
rollback, time-travel và save/load; nó chứa con trỏ chương trình, call stack,
kho biến, danh sách lựa chọn đang chờ, và các event cần phát lại khi khôi phục.

Quy tắc xác định (deterministic) — bất biến quan trọng nhất của core: VM không
đọc đồng hồ, không sinh ngẫu nhiên ngoài PRNG có seed trong state (PRNG sẽ thêm
ở M1 dưới dạng lệnh/hàm riêng), không I/O. Hệ quả: cùng Story + cùng chuỗi
input (`advance`/`choose`) → cùng chuỗi event, trên mọi nền tảng. Test
`xac_dinh_sau_restore` khoá bất biến này.

PRNG (v1): một SplitMix64 tự cài nằm trong state của VM và trong snapshot — rollback/save/load tái lập đúng chuỗi số đã rút. Seed mặc định là hằng số (golden test không cần cấu hình); shell đổi bằng `Vm::set_seed`, hiệu lực từ lần `start()` kế tiếp. Thuật toán PRNG là một phần của hợp đồng ngữ nghĩa, không đổi trong cùng major — golden test khoá nó.
Ngân sách bước (v1): mỗi lượt `Running` thực thi tối đa 100 000 lệnh (`Vm::set_step_budget` để đổi); vượt → `StepBudgetExceeded`. Lưới an toàn cho vòng lặp `goto` vô hạn tác giả viết nhầm — VM báo lỗi, không bao giờ treo.

Sau mục `set` thêm ba mục lệnh:

`set_expr {var, expr}` (v1) — gán kết quả biểu thức vào biến; không phát event, không dừng. `expr` là AST có cấu trúc ngay trong IR (không phải chuỗi text — cú pháp text là việc của DSL M2): `lit` (Value), `var` (đọc biến), `neg`, `bin {op ∈ add|sub|mul|div|rem, lhs, rhs}`. Số học Int-only; sai kiểu → `TypeMismatch`, không ghi gì; tràn thì bão hoà; `div`/`rem` cho 0 → `DivByZero`. Biến chưa tồn tại đọc ra 0 (nhất quán add/sub của `set`). Ví dụ: `{"op":"set_expr","var":"i","expr":{"bin":{"op":"add","lhs":{"var":"i"},"rhs":{"lit":1}}}}`.
`rand {var, min, max}` (v1) — rút một Int trong `[min, max]` (bao cả hai đầu) từ PRNG, gán vào var; không phát event, không dừng. `min > max` → `BadRandRange` (lint bắt từ lúc soạn). Map khoảng bằng nhân 128-bit, không modulo.
`label {name`} / `goto {label}` (v1) — `label` là mốc trong node, no-op khi thực thi, chỉ hợp lệ ở cấp cao nhất của body (lint bắt label trong nhánh if). `goto` nhảy tới label cùng node từ bất kỳ đâu kể cả trong nhánh if (đường `parents` xoá sạch). Label không tồn tại → `UnknownLabel`.

Và mục "Tương thích": thêm câu "v1 (rand, label/goto, set_expr) là superset thuần của v0 — migration 0→1 là no-op. VM/lint nhận mọi `format_version ≤ 1`; lớn hơn → `UnsupportedFormatVersion`."

## Kiểu dữ liệu

`Value` là `Int(i64) | Bool | Str`, serialize dạng untagged (JSON tự nhiên:
`1`, `true`, `"a"`). Điều kiện `Cond{var, op, value}` với `op ∈ {ge, le, eq,
ne}`; `ge/le` chỉ hợp lệ trên Int. Phép ghi `Effect{var, op, value}` với
`op ∈ {assign, add, sub, toggle}`. Biến chưa tồn tại: khi đọc trong điều kiện
nhận giá trị mặc định theo kiểu vế phải (Int→0, Bool→false, Str→"");
khi `add/sub` coi như 0; khi `toggle` coi như false. Sai kiểu → lỗi runtime
`TypeMismatch`, không ghi gì. `add/sub` bão hoà (không wrap) khi tràn i64.

## Từng lệnh

**`say {speaker?, text, opts?}`** — phát `VmEvent::Say` rồi dừng ở
`AwaitAdvance`. `text` là StringKey trỏ vào bảng chuỗi (core không bao giờ
thấy văn bản thật — dịch là việc của mong-i18n ở tầng trên). `opts`
(`pose/pos/sfx/exit`) là dữ liệu trình diễn, core chỉ chuyển tiếp nguyên vẹn.

**`choice {arms[]}`** — mỗi arm: `{text, target?, cond?, effects[]}`. VM lọc
arm theo `cond` (arm không cond luôn hiện), phát `Choices` với danh sách đã
lọc và đánh chỉ số lại từ 0, dừng ở `AwaitChoice`. Khi `choose(i)`: áp
`effects` theo thứ tự khai báo, rồi nhảy tới `target`; `target = None` nghĩa
là kết thúc truyện. Nếu **không arm nào thoả điều kiện**, truyện kết thúc ngay
(soft-lock) — bộ lint bắt buộc cảnh báo trường hợp mọi arm đều có cond từ lúc
soạn. Các lệnh đứng sau `choice` trong cùng block là bất khả đạt.

**`jump {target}`** — chuyển sang node khác trong cùng khung gọi; phát
`NodeEntered`. Không đụng call stack. Node không tồn tại → lỗi `UnknownNode`
(lint bắt từ trước, runtime vẫn phải xử lý an toàn).

**`call {target}` / `return`** — gọi node như thủ tục con: `call` đẩy con trỏ
quay về (lệnh ngay sau `call`, kể cả khi đang ở trong nhánh `if`) vào call
stack; `return` (hoặc chạy hết body của node được gọi) quay về đó. `return`
khi stack rỗng → lỗi `CallStackUnderflow`. Dùng cho cảnh tái sử dụng
(mini-scene, đoạn lặp).

**`set {effect}`** — áp một phép ghi biến, không phát event, không dừng.

**`if {cond, then_branch, else_branch?}`** — đánh giá cond một lần lúc đi vào,
rồi thực thi nhánh tương ứng; hết nhánh thì tiếp tục lệnh sau `if`. Lồng nhau
tuỳ ý (con trỏ chương trình lưu đường vào các nhánh, xem `Cursor.parents`).

**`scene {scene, transition?}` / `show` / `hide` / `sfx` / `bgm`** — thuần
trình diễn: phát event tương ứng rồi chạy tiếp, không dừng. Quy ước cho
runtime (không phải nghĩa vụ của core): `scene` dọn sạch sân khấu và đổi BGM
theo khai báo của scene; `bgm{asset: None}` là tắt nhạc.

**`wait {ms}`** — phát `Wait` rồi dừng ở `AwaitAdvance`. Core **không** tự
hẹn giờ (giữ tính xác định); runtime có màn hình sẽ gọi `advance()` khi hết
giờ, text-mode runner gọi ngay lập tức.

**`ext {command, args}`** — cửa mở rộng cho plugin: phát `Ext` nguyên vẹn,
không dừng. Plugin đăng ký lệnh qua mong-plugin (M5); lệnh không ai xử lý thì
runtime bỏ qua và ghi log — không bao giờ là lỗi cứng, để mongpack có plugin
vẫn chạy được trên runtime không bật plugin đó.

**`end`** — kết thúc truyện, phát `Ended`. Chạy hết body node mà không có
`jump/end` và call stack rỗng cũng tương đương `end` (kết thúc ngầm — lint
liệt kê dạng "ghi chú" để tác giả xác nhận chủ đích).

## Sự kiện (VmEvent)

`Say, Show, Hide, SceneChanged, Choices, Wait, Sfx, Bgm, Ext, NodeEntered,
Ended`. Đây là hợp đồng core ↔ runtime: renderer chỉ được phản ứng theo
event, không được với tay vào state của VM ngoài API công khai. `NodeEntered`
phát mỗi lần vào node (start/jump/call/choose) — editor dùng nó cho debug path
và hot reload.

## Tương thích và mở rộng

Thêm lệnh mới = thêm variant `Instr` + tăng `format_version` của Story; VM
đọc version cũ hơn trong cùng major phải chạy đúng. Serialize IR trong
mongpack v0 dùng JSON (dễ soi, dễ diff); chuyển sang định dạng nhị phân
(CBOR/bincode) là việc của tối ưu sau này và chỉ đổi codec entry, không đổi
ngữ nghĩa. Ba lệnh dự kiến của M1: `rand` (PRNG có seed), `label/goto` trong
node, và `set` với biểu thức số học — đều đã có chỗ trong mô hình hiện tại.
