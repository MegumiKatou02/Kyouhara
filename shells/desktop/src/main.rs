//! Shell desktop: đọc dự án rồi giao cho `mong-shell`.
//!
//! Chạy: `cargo run -p mong-desktop -- <thu_muc | file.mongpack> [locale]`
//! Trong lúc chơi: click / Space / Enter = tiếp | 1-9 = chọn | Z = lùi.

fn main() {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("cach dung: mong-desktop <thu_muc_du_an | file.mongpack> [locale]");
        std::process::exit(2);
    };
    let locale = args.next();

    // Nhận cả hai: thư mục (dev) và gói (thứ người chơi nhận).
    let loaded = if std::path::Path::new(&path).is_dir() {
        mong_project::load_dir(&path, locale.as_deref())
    } else {
        match std::fs::read(&path) {
            Ok(b) => mong_project::load_pack(&b, locale.as_deref()),
            Err(e) => {
                eprintln!("loi: {path}: {e}");
                std::process::exit(1);
            }
        }
    };

    match loaded {
        Ok(l) => mong_shell::run(l),
        Err(e) => {
            eprintln!("loi: {e}");
            std::process::exit(1);
        }
    }
}
