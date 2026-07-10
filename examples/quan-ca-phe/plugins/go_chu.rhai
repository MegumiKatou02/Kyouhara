// go_chu — tiếng gõ phím theo nhịp typewriter. Hook trình diễn:
// host chặn set_var/goto ở đây, chỉ sfx/shake/cps được qua.
fn on_type(m) {
    if m.grapheme == " " {
        return; // khoảng trắng im lặng
    }
    if m.index % 2 == 0 {
        play_sfx("go_phim"); // mỗi 2 grapheme một tiếng, đỡ dày tai
    }
}
