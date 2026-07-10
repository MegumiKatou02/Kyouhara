//! Plugin ↔ runtime theo docs/spec-plugin.md: filter chèn biến, ext→shake,
//! set_var vào snapshot (rollback thấy đúng), goto có ngân sách.

use mong_assets::Manifest;
use mong_core::{Instr, Node, SayOpts, Story, Value, FORMAT_VERSION};
use mong_i18n::{Catalog, Table};
use mong_runtime::{Input, Runtime};
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
    rt.set_plugins(
        &plugins
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
    );
    rt.start().unwrap();
    rt
}

#[test]
fn filter_chen_bien_vao_thoai() {
    let src = r#"fn filter_text(m) { m.text.replace("{ten}", get_var("ten")); m.text }"#;
    // rhai: replace trả String mới hay sửa tại chỗ tuỳ version — viết an toàn:
    let src = r#"fn filter_text(m) { let t = m.text; t.replace("{ten}", get_var("ten")); t }"#;
    let _ = src;
    let rt = runtime(
        story(vec![say("a.l1"), Instr::End], vec![]),
        &[("a.l1", "Chào {ten}!")],
        &[(
            "chen_bien",
            r#"fn filter_text(m) { let t = m.text; t.replace("{ten}", get_var("ten")); t }"#,
        )],
    );
    assert_eq!(rt.line().unwrap().text, "Chào Minh!");
}

#[test]
fn ext_rung_tao_shake_va_tu_tat() {
    let mut rt = runtime(
        story(
            vec![
                Instr::Ext {
                    command: "rung".into(),
                    args: serde_json::json!({"px": 8, "ms": 200}),
                },
                say("a.l1"),
                Instr::End,
            ],
            vec![],
        ),
        &[("a.l1", "x")],
        &[("rung", "fn ext_rung(args) { shake(args.px, args.ms); }")],
    );
    assert_ne!(rt.shake_offset(), (0.0, 0.0));
    rt.tick(0.5).unwrap(); // 500ms > 200ms: rung phải tắt
    assert_eq!(rt.shake_offset(), (0.0, 0.0));
}

#[test]
fn set_var_cua_plugin_song_sot_qua_rollback() {
    // on_line_show dòng 1 ghi bien=7. Advance sang dòng 2 rồi rollback về
    // dòng 1: hook KHÔNG bắn lại, nhưng snapshot dòng 1 đã chứa bien=7.
    let mut rt = runtime(
        story(vec![say("a.l1"), say("a.l2"), Instr::End], vec![]),
        &[("a.l1", "mot"), ("a.l2", "hai")],
        &[(
            "ghi",
            r#"fn on_line_show(m) { if m.key == "a.l1" { set_var("bien", 7); } }"#,
        )],
    );
    assert_eq!(rt.vars().get("bien"), Some(&Value::Int(7)));
    rt.input(Input::Advance).unwrap(); // sang "hai"
    rt.input(Input::Rollback).unwrap(); // về "mot"
    assert_eq!(rt.line().unwrap().text, "mot");
    assert_eq!(
        rt.vars().get("bien"),
        Some(&Value::Int(7)),
        "snapshot phai chua hau qua hook lan dau"
    );
}

#[test]
fn goto_trong_on_node_enter_khong_de_lai_event_oi() {
    // Node "a" có Say ngay sau NodeEntered; plugin goto("b") lúc vào "a".
    // Nếu phần đuôi batch cũ vẫn được áp, line sẽ là "x" (của a) thay vì "y".
    let mut rt = runtime(
        story(
            vec![say("a.l1"), Instr::End],
            vec![Node {
                id: "b".into(),
                title: String::new(),
                scene: None,
                body: vec![say("b.l1"), Instr::End],
            }],
        ),
        &[("a.l1", "x"), ("b.l1", "y")],
        &[(
            "nhay",
            r#"fn on_node_enter(m) { if m.node == "a" { goto("b"); } }"#,
        )],
    );
    assert_eq!(rt.line().unwrap().text, "y");
}

#[test]
fn goto_day_chuyen_bi_ngan_sach_chan() {
    // on_node_enter của "b" goto chính nó — không có ngân sách thì treo/tràn stack.
    let mut rt = runtime(
        story(
            vec![say("a.l1"), Instr::End],
            vec![Node {
                id: "b".into(),
                title: String::new(),
                scene: None,
                body: vec![say("b.l1"), Instr::End],
            }],
        ),
        &[("a.l1", "x"), ("b.l1", "y")],
        &[(
            "lap",
            r#"fn on_node_enter(m) { if m.node == "b" { goto("b"); } }
               fn on_line_show(m) { if m.key == "a.l1" { goto("b"); } }"#,
        )],
    );
    // Nếu tới được đây là ngân sách đã chặn; VM phải đang đứng ở "b".
    assert_eq!(rt.line().unwrap().text, "y");
}
