# Checklist luật lint — Mộng Engine

Tài liệu này là **bằng chứng DoD M2**: "lint bắt đủ các lỗi mà prototype v4
bắt". Cột *Nguồn* ghi luật đến từ đâu; cột *Test* ghi test khoá luật đó.
Mã luật (`L###`) là hợp đồng — không tái sử dụng mã đã xoá.

## A. Luật gốc từ prototype v4

Tài liệu thiết kế mục 2 liệt kê đúng ba nhóm lỗi prototype v4 quét được:
**nhánh mồ côi, biến chưa khai báo, soft-lock**. Prototype quét trên cấu trúc
`lines + mode + next`; engine quét trên IR nên chính xác hơn và bắt được cả
các ca lồng trong nhánh `if`.

| Mã | Mức | Luật | Nguồn | Test |
|---|---|---|---|---|
| L001 | Error | Nhánh mồ côi: node không tới được từ `start` (BFS qua jump/call/choice-target) | v4 | `bat_nhanh_mo_coi_va_bien_chua_khai_bao` |
| L002 | Error | Dùng biến chưa khai báo trong `variables` (cond, effect, set, set_expr, rand) | v4 | `bat_nhanh_mo_coi_va_bien_chua_khai_bao` |
| L003 | Warning | Soft-lock: `choice` mà **mọi** arm đều có `cond` → có thể kết thúc đột ngột | v4 | `canh_bao_soft_lock` |

## B. Luật engine bổ sung (M0–M1, chạy trên IR)

| Mã | Mức | Luật | Test |
|---|---|---|---|
| L010 | Error | `formatVersion` mới hơn bản hỗ trợ | (validate) |
| L011 | Error | Node trùng `id` | (validate) |
| L012 | Error | `start` trỏ tới node không tồn tại | (validate) |
| L013 | Error | `jump`/`call`/arm-target trỏ tới node không tồn tại | `bat_dich_den_khong_ton_tai` |
| L014 | Error | `choice` không có arm nào | (validate) |
| L015 | Error | `label` đặt trong nhánh `if` (spec-ir: chỉ hợp lệ ở cấp cao nhất) | `bat_loi_label_goto_va_rand_v1` |
| L016 | Error | `goto` tới label không tồn tại trong node | `bat_loi_label_goto_va_rand_v1` |
| L017 | Error | `label` trùng tên trong cùng node | `bat_loi_label_goto_va_rand_v1` |
| L018 | Error | `rand` khoảng rỗng (`min > max`) | `bat_loi_label_goto_va_rand_v1` |

## C. Luật mới ở M2 (`lint.rs`)

Phát sinh từ các quyết định đã chốt của spec-mongscript.

| Mã | Mức | Luật | Quyết định nguồn | Test |
|---|---|---|---|---|
| L020 | Warning | Arm vắng `->` (target = None) → truyện kết thúc khi chọn | spec-mongscript QĐ 4 | `l020_arm_vang_target` |
| L021 | Warning | Lệnh đứng sau `choice` trong cùng block là bất khả đạt | spec-ir mục `choice` | `l021_lenh_sau_choice` |
| L022 | Error | Hai dòng dịch được dùng chung một `string_key` | spec-mongscript mục 6 | `l022_key_trung` |
| L023 | Warning | Key mồ côi: có trong bảng chuỗi, không dòng nào tham chiếu | spec-mongscript mục 6 | `l023_key_mo_coi` |
| L024 | Error | Key được tham chiếu nhưng vắng trong bảng defaultLocale | spec-mongscript mục 7 | `l024_key_thieu_o_default` |
| L025 | Warning | Chia/lấy dư cho literal 0 trong `set_expr` (runtime `DivByZero`) | spec-ir `set_expr` | `l025_chia_cho_khong` |
| L026 | Warning | `return` ở node không bao giờ được `call` (runtime `CallStackUnderflow`) | spec-ir `call/return` | `l026_return_khong_ai_call` |
| L027 | Warning | Locale khai báo trong `locales[]` thiếu bản dịch cho ≥1 key | spec-mongscript mục 7 | (mong-cli, dùng `Catalog::missing_in`) |

L022/L023/L024/L027 cần bảng chuỗi nên nằm ở `validate_strings()`, tách khỏi
`validate()` (chỉ nhận `Story`). `mong-cli lint` gọi cả hai.

## D. Cố ý KHÔNG lint (ghi để khỏi bàn lại)

- **Vòng lặp `goto` vô hạn**: không quyết định được tĩnh (halting problem);
  VM đã có ngân sách bước (`StepBudgetExceeded`) làm lưới an toàn.
- **Biến đọc trước khi ghi**: spec-ir định nghĩa giá trị mặc định rõ ràng
  (Int→0, Bool→false, Str→""), nên đây là ngữ nghĩa hợp lệ, không phải lỗi.
- **Node không có lối ra** (chạy hết body): spec-ir cho phép — hết body là
  `return` ngầm hoặc kết thúc truyện.
- **`ext` không ai xử lý**: plugin đăng ký ở runtime, lint không biết được.
- **Sai kiểu trong cond/effect**: cần suy luận kiểu toàn cục (một biến có thể
  đổi kiểu qua `assign`); để mốc sau, runtime đã có `TypeMismatch`.
