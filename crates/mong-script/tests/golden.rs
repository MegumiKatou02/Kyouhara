//! Golden test M1: cùng cốt truyện + cùng chuỗi input → cùng chuỗi event,
//! so khớp từng byte (sau khi normalize newline) với file trong tests/golden/.
//!
//! Cập nhật file golden khi ngữ nghĩa VM đổi CÓ CHỦ ĐÍCH:
//!   UPDATE_GOLDEN=1 cargo test -p mong-script --test golden
//! rồi xem diff bằng Git trước khi commit — diff chính là "ngữ nghĩa đã đổi gì".

use mong_core::{Story, Vm, VmEvent};
use std::fs;
use std::path::PathBuf;

const DEMO: &str = include_str!("data/demo-story.json");

/// Input của người chơi, đơn vị của kịch bản golden.
#[derive(Debug, Clone, Copy)]
enum Input {
    Advance,
    Choose(usize),
    Rollback,
}

fn demo() -> Story {
    mong_script::load_story_json(DEMO).expect("json demo hop le")
}

/// Chạy một kịch bản input, ghi lại từng bước (nhãn input + batch event trả về).
fn run_script(story: Story, inputs: &[Input]) -> serde_json::Value {
    let mut vm = Vm::new(story).unwrap();
    let mut steps = vec![step("start", vm.start().unwrap())];
    for (i, input) in inputs.iter().enumerate() {
        let (label, evs) = match input {
            Input::Advance => ("advance".to_string(), vm.advance()),
            Input::Choose(n) => (format!("choose {n}"), vm.choose(*n)),
            Input::Rollback => (
                "rollback".to_string(),
                Ok(vm
                    .rollback()
                    .unwrap_or_else(|| panic!("buoc {i}: khong lui duoc"))),
            ),
        };
        let evs = evs.unwrap_or_else(|e| panic!("buoc {i} ({label}): {e}"));
        steps.push(step(&label, evs));
    }
    serde_json::Value::Array(steps)
}

fn step(input: &str, events: Vec<VmEvent>) -> serde_json::Value {
    serde_json::json!({ "input": input, "events": events })
}

fn check_golden(name: &str, actual: &serde_json::Value) {
    let rendered = serde_json::to_string_pretty(actual).unwrap() + "\n";
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &rendered).unwrap();
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "thieu file golden '{}'.\nSinh file: UPDATE_GOLDEN=1 cargo test -p mong-script --test golden",
            path.display()
        )
    });
    assert_eq!(
        rendered,
        expected.replace("\r\n", "\n"),
        "chuoi event lech khoi golden '{name}'.\n\
         Neu day la thay doi ngu nghia CO CHU DICH: UPDATE_GOLDEN=1 roi soi diff Git.\n\
         Neu khong: VM da mat tinh xac dinh hoac co regression."
    );
}

use Input::{Advance, Choose, Rollback};

#[test]
fn golden_demo_ket_dep() {
    // Chào trước (+1 thiện cảm) → nhận lời → kết đẹp trên sân thượng.
    let log = run_script(
        demo(),
        &[
            Advance,   // mo_dau.l2
            Advance,   // choices (2 arm)
            Choose(0), // bat_chuyen.l1
            Advance,   // bat_chuyen.l2
            Advance,   // jump -> loi_moi.l1
            Advance,   // loi_moi.l2
            Advance,   // choices (2 arm: thien_cam >= 1)
            Choose(0), // scene san_thuong + ket_dep.l1
            Advance,   // ket_dep.l2
            Advance,   // ket_dep.l3
            Advance,   // Ended
        ],
    );
    check_golden("demo_ket_dep.golden.json", &log);
}

#[test]
fn golden_demo_ket_thuong() {
    // Lảng tránh → lựa chọn có điều kiện bị ẩn (chỉ 1 arm) → kết thường.
    let log = run_script(
        demo(),
        &[
            Advance,   // mo_dau.l2
            Advance,   // choices
            Choose(1), // lang_tranh.l1
            Advance,   // lang_tranh.l2
            Advance,   // jump -> loi_moi.l1
            Advance,   // loi_moi.l2
            Advance,   // choices (1 arm)
            Choose(0), // ket_thuong.l1
            Advance,   // Ended
        ],
    );
    check_golden("demo_ket_thuong.golden.json", &log);
}

#[test]
fn golden_demo_rollback_re_nhanh_khac() {
    // Chọn "chào" (+1 thiện cảm), lùi lại, chọn "lảng tránh".
    // Batch cuối phải là Choices 1 arm — chứng minh rollback khôi phục cả biến.
    let log = run_script(
        demo(),
        &[
            Advance,   // mo_dau.l2
            Advance,   // choices
            Choose(0), // bat_chuyen.l1 (thien_cam = 1)
            Rollback,  // replay Choices, thien_cam ve 0
            Choose(1), // lang_tranh.l1
            Advance,   // lang_tranh.l2
            Advance,   // loi_moi.l1
            Advance,   // loi_moi.l2
            Advance,   // choices — PHAI chi con 1 arm
            Choose(0), // ket_thuong.l1
            Advance,   // Ended
        ],
    );
    check_golden("demo_rollback_re_nhanh_khac.golden.json", &log);
}

/// Save/restore trên demo thật: đuôi sự kiện sau restore phải trùng từng byte
/// với lần chạy gốc (bổ trợ cho `xac_dinh_sau_restore` vốn chỉ chạy mini story).
#[test]
fn save_restore_xac_dinh_tren_demo() {
    let mut vm = Vm::new(demo()).unwrap();
    vm.start().unwrap();
    vm.advance().unwrap();
    vm.advance().unwrap(); // đứng ở màn lựa chọn đầu
    let snap = vm.snapshot().expect("co snapshot tai diem cho");

    let tail = |vm: &mut Vm| {
        let mut evs = vm.choose(0).unwrap();
        for _ in 0..3 {
            evs.extend(vm.advance().unwrap());
        }
        serde_json::to_string(&evs).unwrap()
    };

    let tail1 = tail(&mut vm);
    let replay = vm.restore(&snap);
    assert!(matches!(replay.last(), Some(VmEvent::Choices { .. })));
    let tail2 = tail(&mut vm);
    assert_eq!(
        tail1, tail2,
        "restore roi chay lai cung input phai ra cung event"
    );
}
