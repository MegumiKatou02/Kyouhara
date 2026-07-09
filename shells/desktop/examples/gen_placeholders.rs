//! Sinh PNG placeholder cho examples/quan-ca-phe — để DoD M3 kiểm chứng được
//! trước khi có art thật. Artist thay file, không đụng code.
//!
//! `cargo run -p mong-desktop --example gen_placeholders`

use std::fs;
use std::io::BufWriter;
use std::path::Path;

/// Sprite nhân vật dùng chung khung: các lớp chồng khít lên nhau, chân sprite
/// nằm ở đáy ảnh (runtime neo theo chân — xem `Fit::Anchor`).
const CHAR_W: u32 = 520;
const CHAR_H: u32 = 900;

fn write(path: &str, w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 4]) {
    let p = Path::new(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&px(x, y));
        }
    }
    let mut enc = png::Encoder::new(BufWriter::new(fs::File::create(p).unwrap()), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
    println!("{path}");
}

/// Nền: dốc màu dọc, đủ để thấy transition fade đổi cả bức.
fn bg(path: &str, top: [u8; 3], bottom: [u8; 3]) {
    write(path, 1920, 1080, |_, y| {
        let t = y as f32 / 1079.0;
        let c = |i: usize| (top[i] as f32 * (1.0 - t) + bottom[i] as f32 * t) as u8;
        [c(0), c(1), c(2), 255]
    });
}

/// Thân: hình viên thuốc, chân chạm đáy khung.
fn base(path: &str, rgb: [u8; 3]) {
    let (cx, r) = (CHAR_W as f32 / 2.0, 150.0);
    write(path, CHAR_W, CHAR_H, |x, y| {
        let (dx, dy) = (x as f32 - cx, y as f32);
        let inside = dx.abs() < r && dy > 200.0 || (dx * dx + (dy - 200.0).powi(2)) < r * r;
        if inside {
            [rgb[0], rgb[1], rgb[2], 255]
        } else {
            [0, 0, 0, 0]
        }
    });
}

/// Mặt: hai chấm mắt + một vạch miệng, cong theo biểu cảm.
/// `mouth` > 0 là cười, < 0 là buồn.
fn face(path: &str, mouth: f32) {
    let cx = CHAR_W as f32 / 2.0;
    write(path, CHAR_W, CHAR_H, |x, y| {
        let (fx, fy) = (x as f32 - cx, y as f32 - 190.0);
        let eye = ((fx.abs() - 45.0).powi(2) + (fy + 20.0).powi(2)) < 130.0;
        let curve = mouth * (fx / 55.0).powi(2) * 22.0;
        let lip = fx.abs() < 55.0 && (fy - 55.0 + curve).abs() < 5.0;
        if eye || lip {
            [30, 24, 28, 255]
        } else {
            [0, 0, 0, 0]
        }
    });
}

/// Trang phục: dải màu vắt qua thân, chứng minh thứ tự chồng lớp đúng.
fn outfit(path: &str, rgb: [u8; 3]) {
    write(path, CHAR_W, CHAR_H, |x, y| {
        let dx = x as f32 - CHAR_W as f32 / 2.0;
        if dx.abs() < 150.0 && (400..520).contains(&y) {
            [rgb[0], rgb[1], rgb[2], 255]
        } else {
            [0, 0, 0, 0]
        }
    });
}

fn main() {
    let d = "examples/quan-ca-phe/assets";
    bg(
        &format!("{d}/backgrounds/quan_ca_phe.png"),
        [96, 74, 58],
        [40, 30, 26],
    );
    bg(
        &format!("{d}/backgrounds/san_thuong.png"),
        [244, 148, 92],
        [70, 52, 96],
    );

    base(
        &format!("{d}/characters/lan/base/than.png"),
        [224, 108, 159],
    );
    face(&format!("{d}/characters/lan/face/thuong.png"), 0.0);
    face(&format!("{d}/characters/lan/face/vui.png"), 1.0);
    outfit(
        &format!("{d}/characters/lan/outfit/ao_dai.png"),
        [250, 240, 235],
    );

    base(
        &format!("{d}/characters/minh/base/than.png"),
        [108, 159, 224],
    );
    face(&format!("{d}/characters/minh/face/thuong.png"), 0.0);
    face(&format!("{d}/characters/minh/face/cuoi.png"), 1.0);

    println!("\nCon thieu:");
    println!("  {d}/fonts/BeVietnamPro-Regular.ttf  (chep tu crates/mong-render/tests/fonts/)");
    println!("  {d}/audio/*.ogg                     (bo trong duoc, game chay cam)");
}
