# M5 — Plugin host

**Trạng thái: ĐANG MỞ** (mở 2026-07-11)

## DoD
- [x] 3 plugin mẫu prototype (chèn biến, rung, gõ chữ) chạy desktop
- [x] 3 plugin đó chạy web — kiểm tay Chrome/Firefox. **Safari chưa xác minh**
      (bộ gá CI hỏng ở tầng WebDriver, xem Nợ 5; điều khoản tự ràng buộc
      của m4-ket-thuc đã kích hoạt — mục 8/14 tài liệu thiết kế đã sửa theo)

## Tiến độ
- [x] M5.0 Chốt mongpack §5.1/§5.2 + spec-plugin.md; plugins/*.rhai vào Loaded/gói
- [x] M5.1 mong-plugin host (rhai sandbox, cô lập lỗi, hợp đồng data-only)
- [x] M5.2 Tích hợp runtime (hook/filter/ext/action, Vm::set_var + jump_to)
- [x] M5.3 Ba plugin demo + sfx go_phim
- [x] M5.4 Kiểm chứng web (Chrome/Firefox tay) + đo bundle. Safari-CI: không đạt, tắt job (if: false)

## Số đo
- Bundle gzip sau khi rhai vào: **1.632.746 B (31% trần 5 MB)** — trước rhai
  1.320.898 B (25%); rhai + đường plugin cộng ~312 KB gzip.

## Quyết định phát sinh
- `set_var` vá cả snapshot gần nhất: hook bắn tại điểm dừng nên hậu quả
  thuộc trạng thái điểm dừng; rollback thấy giá trị sau-hook, hook không
  bắn lại khi replay.
- `goto` = ngữ nghĩa `jump` (không đụng call stack); binding rhai tên
  `goto_node` (rhai giữ từ khoá `goto`); cái cuối trong batch thắng;
  ngân sách 8 goto/lượt; `jump_gen` chặn event ôi sau cú nhảy.
- `on_type` bắn theo hiển thị thực tế (kể cả dòng dựng lại sau rollback);
  skip không bắn dồn; `cps ≤ 0` = hiện tức thì.
- Mã top-level ngoài hàm trong .rhai không chạy — plugin là tập hàm hook.
- rhai feature `no_time`: cấm thời gian ở tầng biên dịch.
- M5 không dựng wasm-test runner (hợp đồng giá trị chỉ Int/Bool/Str,
  rủi ro lệch thấp); ghi nợ M6.

## Nợ mới
1. Log plugin trên web rơi vào hư không (`eprintln` wasm) — cần đường ra
   console/log facade. M6 (editor cần cùng hạ tầng).
2. mong-audio nên cảnh báo một-lần-mỗi-id khi sfx chưa register (go_phim
   lặp log mỗi blip trước khi có manifest entry).
3. Hạ tầng chạy test trên wasm (wasm-bindgen-test + headless). M6.
4. Nợ 15 của M4 (phân đoạn script + AttrsList) gắn nhãn M5 nhưng không
   thuộc DoD plugin — dời M6, ghi rõ tại đây thay vì im lặng trượt.
5. **Bộ gá Safari-CI hỏng ở tầng WebDriver.** Sau đủ 60s, `execute/sync` trả
   về trang không có `__mong_stage` — JS của trang chưa từng thực thi theo
   góc nhìn của safaridriver, dù script thường đặt biến ngay khi parse HTML.
   Chưa xác định nguyên nhân (execute-context? safaridriver headless?).
   Engine CHƯA được kiểm trên Safari — không phải "Safari fail", mà là "chưa
   đo được". Job giữ nguyên với `if: false`; `ci/safari_check.py` giữ làm
   script chạy tay. Xử cùng lúc nâng wgpu (nợ 8 M4) hoặc khi có máy Mac.
