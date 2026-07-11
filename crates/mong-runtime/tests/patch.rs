//! Hot reload (M6, spec-devlink): patch_strings không đụng VM;
//! patch_node full-session-replay giữ vị trí, tái hiện hậu quả plugin,
//! và dừng-tại-chỗ khi log vấp vào nội dung đã đổi.
//!
//! Helper dựng story/runtime chép theo mẫu tests/plugin.rs.

use mong_assets::Manifest;
use mong_core::{ChoiceArm, Instr, Node, SayOpts, Story, Value, VmError, VmStatus, FORMAT_VERSION};
use mong_i18n::{Catalog, Table};
use mong_runtime::{Input, PatchOutcome, Runtime};
use std::collections::BTreeMap;

fn story(body_a: Vec<Instr>, extra: Vec<Node>) -> Story {
    let mut nodes = vec![Node {
        id: "a".into(),
        title: String::new(),
        scene: None,
        body: body_a,
    }];
    nodes.extend(extra);
    Story {
        format_version: FORMAT_VERSION,
        title: String::new(),
        default_locale: "vi".into(),
        locales: vec![],
        variables: BTreeMap::from([("ten".to_string(), Value::Str("Minh".into()))]),
        start: "a".into(),
        nodes,
    }
}

fn say(key: &str) -> Instr {
    Instr::Say {
        speaker: None,
        text: key.into(),
        opts: SayOpts::default(),
    }
}

fn node(id: &str, body: Vec<Instr>) -> Node {
    Node {
        id: id.into(),
        title: String::new(),
        scene: None,
        body,
    }
}

fn arm(text: &str, target: &str) -> ChoiceArm {
    ChoiceArm {
        text: text.into(),
        target: Some(target.into()),
        cond: None,
        effects: vec![],
    }
}

fn runtime(story: Story, texts: &[(&str, &str)], plugins: &[(&str, &str)]) -> Runtime {
    let mut c = Catalog::new("vi".to_string());
    c.set_table(
        "vi".to_string(),
        Table::from_iter(texts.iter().map(|(k, v)| (k.to_string(), v.to_string()))),
    );
    let manifest =
        Manifest::parse(r#"{ "format_version": 2, "scenes": {}, "characters": {}, "assets": {} }"#)
            .unwrap();
    let mut rt = Runtime::new(story, c, manifest, "vi").unwrap();
    // Test không gọi tick() nên typewriter không bao giờ tự chạy xong; để
    // cps mặc định thì Advance đầu tiên trên MỖI dòng chỉ là skip (reveal),
    // không đẩy VM. cps ≤ 0 = hiện tức thì (hành vi đã chốt M5) — mỗi
    // Advance trong test là một bước VM thật.
    rt.set_cps(0.0);
    rt.set_plugins(
        &plugins
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
    );
    rt.start().unwrap();
    rt
}

fn entries(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn patch_strings_doi_dong_dang_hien_khong_reset_vm() {
    // Đường nóng của DoD "sửa thoại < 1s": chỉ catalog + dòng đang hiện đổi,
    // VM đứng nguyên chỗ cũ.
    let mut rt = runtime(
        story(vec![say("a.l1"), say("a.l2"), Instr::End], vec![]),
        &[("a.l1", "mot"), ("a.l2", "hai")],
        &[],
    );
    assert_eq!(rt.line().unwrap().text, "mot");

    rt.patch_strings("vi", &entries(&[("a.l1", "mot moi")]));
    assert_eq!(rt.line().unwrap().text, "mot moi");
    assert_eq!(rt.status(), VmStatus::AwaitAdvance, "VM khong duoc reset");

    // Vị trí giữ nguyên: advance đi tiếp sang dòng 2, không chạy lại từ đầu.
    rt.input(Input::Advance).unwrap();
    assert_eq!(rt.line().unwrap().text, "hai");
}

#[test]
fn patch_strings_di_qua_filter_plugin_nhu_begin_line() {
    // Text sau patch phải đi cùng đường filter với lúc dựng dòng — nếu không
    // patch cho ra text "sống" khác text begin_line dựng.
    let mut rt = runtime(
        story(vec![say("a.l1"), Instr::End], vec![]),
        &[("a.l1", "Chào {ten}!")],
        &[(
            "chen_bien",
            r#"fn filter_text(m) { let t = m.text; t.replace("{ten}", get_var("ten")); t }"#,
        )],
    );
    assert_eq!(rt.line().unwrap().text, "Chào Minh!");

    rt.patch_strings("vi", &entries(&[("a.l1", "Tạm biệt {ten}!")]));
    assert_eq!(rt.line().unwrap().text, "Tạm biệt Minh!");
}

#[test]
fn patch_node_giu_nguyen_vi_tri_khi_log_van_hop_le() {
    // Đứng ở dòng 2, thêm một câu vào CUỐI node: replay áp trọn log, người
    // viết vẫn đứng ở dòng 2, câu mới hiện khi advance.
    let mut rt = runtime(
        story(vec![say("a.l1"), say("a.l2"), Instr::End], vec![]),
        &[("a.l1", "mot"), ("a.l2", "hai")],
        &[],
    );
    rt.input(Input::Advance).unwrap();
    assert_eq!(rt.line().unwrap().text, "hai");

    let out = rt
        .patch_node(node(
            "a",
            vec![say("a.l1"), say("a.l2"), say("a.l3"), Instr::End],
        ))
        .unwrap();
    assert_eq!(out, PatchOutcome::Full);
    assert_eq!(rt.line().unwrap().text, "hai", "van dung dong 2 sau patch");

    // Nội dung mới có mặt: a.l3 chưa có trong catalog nên hiện chính key
    // (text_or_key — thoại không bao giờ "biến mất").
    rt.input(Input::Advance).unwrap();
    assert_eq!(rt.line().unwrap().text, "a.l3");
    rt.input(Input::Advance).unwrap();
    assert_eq!(rt.status(), VmStatus::Ended);
}

#[test]
fn patch_node_xoa_arm_lam_replay_vap_thi_dung_tai_cho() {
    // Phiên gốc chọn arm 1; patch xoá arm đó → replay vấp ở entry Choose(1),
    // dừng ở trạng thái hợp lệ ngay trước điểm vấp (đang chờ chọn, 1 arm).
    let mut rt = runtime(
        story(
            vec![
                say("a.l1"),
                Instr::Choice {
                    arms: vec![arm("c.b", "b"), arm("c.c", "c")],
                },
            ],
            vec![
                node("b", vec![say("b.l1"), Instr::End]),
                node("c", vec![say("c.l1"), Instr::End]),
            ],
        ),
        &[("a.l1", "mot"), ("b.l1", "ben_b"), ("c.l1", "ben_c")],
        &[],
    );
    rt.input(Input::Advance).unwrap(); // tới choices
    rt.input(Input::Choose(1)).unwrap(); // sang node c
    assert_eq!(rt.line().unwrap().text, "ben_c");

    let out = rt
        .patch_node(node(
            "a",
            vec![
                say("a.l1"),
                Instr::Choice {
                    arms: vec![arm("c.b", "b")], // arm 1 không còn
                },
            ],
        ))
        .unwrap();
    // Log: [Advance, Choose(1)] — Advance áp được (applied trước điểm vấp
    // là 1 entry), Choose(1) vấp vì chỉ còn 1 arm.
    assert!(
        matches!(out, PatchOutcome::Stopped { applied: 1, .. }),
        "phai dung tai Choose(1): {out:?}"
    );
    assert_eq!(
        rt.status(),
        VmStatus::AwaitChoice,
        "dung o trang thai hop le"
    );
    assert_eq!(rt.choices().len(), 1);
}

#[test]
fn set_var_cua_plugin_tai_hien_dung_sau_replay() {
    // Hook on_line_show ghi bien=7 ở dòng 1. Patch một node KHÁC (chưa đi
    // qua) → replay bắn lại hook y hệt phiên gốc, biến giữ đúng giá trị.
    let mut rt = runtime(
        story(
            vec![say("a.l1"), say("a.l2"), Instr::End],
            vec![node("b", vec![say("b.l1"), Instr::End])],
        ),
        &[("a.l1", "mot"), ("a.l2", "hai"), ("b.l1", "ben_b")],
        &[(
            "ghi",
            r#"fn on_line_show(m) { if m.key == "a.l1" { set_var("bien", 7); } }"#,
        )],
    );
    rt.input(Input::Advance).unwrap();
    assert_eq!(rt.vars().get("bien"), Some(&Value::Int(7)));

    let out = rt
        .patch_node(node("b", vec![say("b.l2"), Instr::End]))
        .unwrap();
    assert_eq!(out, PatchOutcome::Full);
    assert_eq!(rt.line().unwrap().text, "hai", "vi tri giu nguyen");
    assert_eq!(
        rt.vars().get("bien"),
        Some(&Value::Int(7)),
        "hook tai hien cung hau qua nhu phien goc"
    );
}

#[test]
fn advance_skip_khong_vao_replay_log() {
    // cps mặc định + không tick: dòng chưa gõ xong, Advance đầu chỉ là skip
    // (reveal), KHÔNG đẩy VM — và vì vậy không được vào log. Patch sau đó
    // replay log rỗng: vẫn đứng nguyên dòng 1.
    let mut c = Catalog::new("vi".to_string());
    c.set_table(
        "vi".to_string(),
        Table::from_iter([("a.l1".to_string(), "mot".to_string())]),
    );
    let manifest =
        Manifest::parse(r#"{ "format_version": 2, "scenes": {}, "characters": {}, "assets": {} }"#)
            .unwrap();
    let mut rt = Runtime::new(
        story(vec![say("a.l1"), say("a.l2"), Instr::End], vec![]),
        c,
        manifest,
        "vi",
    )
    .unwrap();
    // Cố ý KHÔNG set_cps(0.0) — cần dòng đang-gõ-dở.
    rt.start().unwrap();

    rt.input(Input::Advance).unwrap(); // skip: chỉ reveal, VM đứng yên
    assert_eq!(rt.line().unwrap().text, "mot");
    assert_eq!(rt.status(), VmStatus::AwaitAdvance);

    let out = rt
        .patch_node(node("a", vec![say("a.l1"), say("a.l2"), Instr::End]))
        .unwrap();
    assert_eq!(out, PatchOutcome::Full, "log rong: khong co gi de vap");
    assert_eq!(
        rt.line().unwrap().text,
        "mot",
        "skip khong duoc replay thanh advance that"
    );
}

#[test]
fn patch_node_id_chua_ton_tai_la_loi() {
    // Thêm node mới không đi đường patch_node — đó là việc của patch_story.
    let mut rt = runtime(
        story(vec![say("a.l1"), Instr::End], vec![]),
        &[("a.l1", "mot")],
        &[],
    );
    let err = rt
        .patch_node(node("khong_co", vec![Instr::End]))
        .unwrap_err();
    assert!(matches!(err, VmError::UnknownNode(id) if id == "khong_co"));
    // Runtime còn nguyên, chơi tiếp bình thường.
    assert_eq!(rt.line().unwrap().text, "mot");
    rt.input(Input::Advance).unwrap();
    assert_eq!(rt.status(), VmStatus::Ended);
}
