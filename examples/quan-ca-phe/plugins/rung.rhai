// rung — đăng ký lệnh DSL `ext rung {"px": …, "ms": …}` → rung màn hình.
fn ext_rung(args) {
    // `ext rung` không kèm JSON thì args là unit — dùng mặc định.
    if type_of(args) != "map" {
        shake(10, 300);
        return;
    }
    let px = if type_of(args.px) == "()" { 10 } else { args.px };
    let ms = if type_of(args.ms) == "()" { 300 } else { args.ms };
    shake(px, ms);
}
