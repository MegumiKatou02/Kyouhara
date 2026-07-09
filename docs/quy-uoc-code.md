# Quy ước code

## Ngôn ngữ định danh

- **Code chính**: tên hàm, biến, kiểu, module — **tiếng Anh**.
- **Comment và doc comment**: tiếng Việt (có dấu).
- **Test**: tên hàm test, biến trong test, thông điệp assert — tiếng Việt
  không dấu. Test là tài liệu sống, đọc bằng tiếng mẹ đẻ nhanh hơn.
- Thông điệp lỗi hướng tới người dùng (`Display for *Error`, `eprintln!`):
  tiếng Việt không dấu — tương thích terminal Windows mặc định.

Nợ: `mong-cli` (M1) còn tên hàm tiếng Việt (`doc_bang_chuoi`,
`nhac_fmt_neu_thieu_key`, `cmd_*` thì đã Anh). Đổi trong một PR cơ học riêng,
không trộn vào mốc đang làm.
