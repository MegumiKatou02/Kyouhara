// chen_bien — thay {ten_bien} trong thoại bằng giá trị biến của VM.
// Là filter nên phải thuần túy (spec-plugin mục 2): chỉ get_var, không
// side effect — chạy lại khi rollback dựng lại dòng vẫn ra đúng text.
fn filter_text(m) {
    let t = m.text;
    let out = "";
    loop {
        let i = t.index_of("{");
        if i < 0 { break; }
        let j = t.index_of("}", i);
        if j < 0 { break; }
        let name = t.sub_string(i + 1, j - i - 1);
        let v = get_var(name);
        out += t.sub_string(0, i);
        if type_of(v) == "()" {
            out += t.sub_string(i, j - i + 1); // biến không tồn tại: giữ nguyên
        } else {
            out += `${v}`;
        }
        t = t.sub_string(j + 1);
    }
    out + t
}
