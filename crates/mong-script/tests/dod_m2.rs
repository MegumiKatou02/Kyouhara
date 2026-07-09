//! Kiểm chứng DoD M2 (tài liệu thiết kế mục 13):
//! "Demo viết lại bằng DSL, diff ổn định; lint bắt đủ các lỗi mà prototype v4 bắt".
//!
//! Ba khẳng định:
//! 1. Demo DSL và demo JSON là **cùng một cốt truyện** — không chỉ `Story ==`,
//!    mà cùng chuỗi `VmEvent` trên cả hai nhánh kết thúc (chạy VM thật).
//! 2. Demo DSL sạch lint (kể cả các luật chuỗi mới L022–L024).
//! 3. Diff ổn định: demo là điểm bất động của formatter (khẳng định lại ở đây
//!    để DoD tự chứa; chi tiết trong tests/fmt.rs).
//!
//! Checklist luật đầy đủ: docs/lint-rules.md. Golden tuyệt đối của chuỗi event
//! vẫn do tests/golden.rs (M1) khoá.

use mong_core::{Story, Vm, VmEvent};
use mong_script::dsl::{format_dsl, load_story_dsl};
use mong_script::{validate, validate_strings, Severity};

const DEMO_DSL: &str = include_str!("data/demo-story.mongscript");
const DEMO_JSON: &str = include_str!("data/demo-story.json");

#[derive(Debug, Clone, Copy)]
enum Input {
    Advance,
    Choose(usize),
}

fn chay(story: Story, inputs: &[Input]) -> Vec<VmEvent> {
    let mut vm = Vm::new(story).expect("vm dung duoc");
    let mut log = vm.start().expect("start");
    for (i, inp) in inputs.iter().enumerate() {
        let evs = match inp {
            Input::Advance => vm.advance(),
            Input::Choose(n) => vm.choose(*n),
        }
        .unwrap_or_else(|e| panic!("buoc {i}: {e}"));
        log.extend(evs);
    }
    log
}

fn story_dsl() -> Story {
    load_story_dsl(DEMO_DSL).expect("demo DSL hop le").story
}

fn story_json() -> Story {
    let mut s: Story = mong_script::load_story_json(DEMO_JSON).expect("demo JSON hop le");
    s.format_version = mong_core::FORMAT_VERSION;
    s
}

use Input::{Advance, Choose};

/// Kịch bản "kết đẹp": chào trước (+1 thiện cảm) → nhận lời.
const KET_DEP: &[Input] = &[
    Advance,
    Advance,
    Choose(0),
    Advance,
    Advance,
    Advance,
    Advance,
    Choose(0),
    Advance,
    Advance,
    Advance,
];

/// Kịch bản "kết thường": lảng tránh → arm có điều kiện bị ẩn.
const KET_THUONG: &[Input] = &[
    Advance,
    Advance,
    Choose(1),
    Advance,
    Advance,
    Advance,
    Advance,
    Choose(0),
    Advance,
];

/// DoD 1 — hai frontend, cùng một cốt truyện chạy được, từng event trùng khớp.
#[test]
fn dod_demo_dsl_chay_giong_het_demo_json() {
    for (ten, inputs) in [("ket_dep", KET_DEP), ("ket_thuong", KET_THUONG)] {
        let tu_dsl = chay(story_dsl(), inputs);
        let tu_json = chay(story_json(), inputs);
        assert_eq!(tu_dsl, tu_json, "kich ban '{ten}': chuoi event lech");
        assert!(
            matches!(tu_dsl.last(), Some(VmEvent::Ended)),
            "kich ban '{ten}' phai ket thuc"
        );
    }
}

/// DoD 1 (dạng tĩnh) — `Story` bằng nhau, không chỉ hành vi.
#[test]
fn dod_story_dsl_bang_story_json() {
    assert_eq!(story_dsl(), story_json());
}

/// DoD 2 — demo sạch lint: không Error, và không Warning nào ngoài ý muốn.
#[test]
fn dod_demo_sach_lint() {
    let story = story_dsl();
    let iss = validate(&story);
    let loi: Vec<&str> = iss
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.message.as_str())
        .collect();
    assert!(loi.is_empty(), "demo khong duoc co loi lint: {loi:?}");

    let canh_bao: Vec<&str> = iss.iter().map(|i| i.message.as_str()).collect();
    assert!(
        canh_bao.is_empty(),
        "demo khong nen co canh bao nao: {canh_bao:?}"
    );
}

/// DoD 2 — luật chuỗi (L022 key trùng, L023 mồ côi, L024 thiếu) sạch trên demo.
#[test]
fn dod_demo_sach_lint_chuoi() {
    let out = load_story_dsl(DEMO_DSL).unwrap();
    let iss = validate_strings(&out.story, &out.strings);
    let msgs: Vec<&str> = iss.iter().map(|i| i.message.as_str()).collect();
    assert!(msgs.is_empty(), "bang chuoi demo phai sach: {msgs:?}");
}

/// DoD 3 — diff ổn định: demo đã ở dạng chuẩn, format lại không đổi byte nào,
/// và không sinh key mới (mọi dòng đã có `#~`).
#[test]
fn dod_diff_on_dinh() {
    let out = format_dsl(DEMO_DSL).expect("format duoc");
    assert_eq!(out.generated_keys, 0);

    let normalized_output = out.text.replace("\r\n", "\n").replace("\r", "\n");
    let normalized_expected = DEMO_DSL.replace("\r\n", "\n").replace("\r", "\n");

    assert_eq!(normalized_output, normalized_expected);
}

/// Lưới an toàn cho checklist: mỗi luật M2 trong docs/lint-rules.md phải
/// thực sự phát ra trên một ca dựng sẵn. Nếu ai xoá luật, test này đỏ.
#[test]
fn checklist_luat_m2_deu_con_song() {
    use mong_core::{BinOp, ChoiceArm, Expr, Instr, Node, SayOpts, Value, FORMAT_VERSION};
    use std::collections::BTreeMap;

    let story = Story {
        format_version: FORMAT_VERSION,
        title: String::new(),
        default_locale: "vi".into(),
        locales: vec![],
        variables: BTreeMap::from([("x".to_string(), Value::Int(0))]),
        start: "a".into(),
        nodes: vec![Node {
            id: "a".into(),
            title: String::new(),
            scene: None,
            body: vec![
                // L025
                Instr::SetExpr {
                    var: "x".into(),
                    expr: Expr::Bin {
                        op: BinOp::Rem,
                        lhs: Box::new(Expr::Var("x".into())),
                        rhs: Box::new(Expr::Lit(Value::Int(0))),
                    },
                },
                // L026
                Instr::Return,
                // L020 (arm vang target) + L021 (co lenh dung sau)
                Instr::Choice {
                    arms: vec![ChoiceArm {
                        text: "k1".into(),
                        target: None,
                        cond: None,
                        effects: vec![],
                    }],
                },
                Instr::Say {
                    speaker: None,
                    // L022: trung key voi arm tren
                    text: "k1".into(),
                    opts: SayOpts::default(),
                },
            ],
        }],
    };

    let iss = validate(&story);
    let co = |s: &str| iss.iter().any(|i| i.message.contains(s));
    assert!(co("khong co dich"), "L020");
    assert!(co("bat kha dat"), "L021");
    assert!(co("chia hoac lay du cho 0"), "L025");
    assert!(co("CallStackUnderflow"), "L026");

    // L022 + L023 + L024 qua bang chuoi.
    let strings = BTreeMap::from([("thua".to_string(), "x".to_string())]);
    let iss = validate_strings(&story, &strings);
    let co = |s: &str| iss.iter().any(|i| i.message.contains(s));
    assert!(co("dung chung"), "L022");
    assert!(co("mo coi"), "L023");
    assert!(co("khong co trong bang chuoi"), "L024");
}
