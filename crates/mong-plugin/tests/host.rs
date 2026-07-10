//! Hợp đồng host theo docs/spec-plugin.md: gom action, xâu chuỗi filter,
//! cô lập lỗi, ranh giới lớp quyền, dispatch ext, ngân sách phép tính.

use mong_plugin::{Action, Hook, Host};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn host(srcs: &[(&str, &str)]) -> Host {
    let m: BTreeMap<String, String> = srcs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    Host::new(&m)
}

fn vars(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn hook_gom_action_set_var() {
    let mut h = host(&[("a", "fn on_game_start() { set_var(\"diem\", 1); }")]);
    let acts = h.fire(Hook::GameStart, &json!({}), &vars(&[]));
    assert_eq!(
        acts,
        vec![Action::SetVar {
            name: "diem".into(),
            value: json!(1)
        }]
    );
    assert!(h.take_log().is_empty());
}

#[test]
fn filter_chen_bien_va_thay_the() {
    let src = r#"
        fn filter_text(m) {
            let t = m.text;
            t.replace("{ten}", get_var("ten"));
            t
        }
    "#;
    let mut h = host(&[("chen_bien", src)]);
    let out = h.filter_text(
        Some("lan"),
        "k",
        "Chào {ten}!",
        &vars(&[("ten", json!("Minh"))]),
    );
    assert_eq!(out, "Chào Minh!");
}

#[test]
fn filter_xau_chuoi_theo_thu_tu_id() {
    let a = r#"fn filter_text(m) { m.text + "a" }"#;
    let b = r#"fn filter_text(m) { m.text + "b" }"#;
    // Nạp theo BTreeMap nên "a" chạy trước "b" bất kể thứ tự khai báo ở đây.
    let mut h = host(&[("b", b), ("a", a)]);
    assert_eq!(h.filter_text(None, "k", "x", &vars(&[])), "xab");
}

#[test]
fn loi_runtime_khong_lay_lan_sang_plugin_khac() {
    let hong = "fn on_game_start() { ham_khong_ton_tai(); }";
    let lanh = "fn on_game_start() { set_var(\"ok\", true); }";
    let mut h = host(&[("a_hong", hong), ("b_lanh", lanh)]);
    let acts = h.fire(Hook::GameStart, &json!({}), &vars(&[]));
    assert_eq!(
        acts,
        vec![Action::SetVar {
            name: "ok".into(),
            value: json!(true)
        }]
    );
    let log = h.take_log();
    assert_eq!(log.len(), 1);
    assert!(
        log[0].contains("a_hong"),
        "log phai chi dung thu pham: {log:?}"
    );
}

#[test]
fn loi_bien_dich_chi_vo_hieu_plugin_do() {
    let mut h = host(&[
        ("a_sai_cu_phap", "fn on_game_start( {"),
        ("b_lanh", "fn on_game_start() { play_sfx(\"ting\"); }"),
    ]);
    let log = h.take_log();
    assert!(log.iter().any(|l| l.contains("a_sai_cu_phap")));
    let acts = h.fire(Hook::GameStart, &json!({}), &vars(&[]));
    assert_eq!(
        acts,
        vec![Action::PlaySfx {
            asset: "ting".into()
        }]
    );
}

#[test]
fn on_type_bi_chan_set_var_nhung_duoc_sfx() {
    let src = r#"
        fn on_type(m) {
            set_var("lau", 1);
            play_sfx("go");
        }
    "#;
    let mut h = host(&[("go_chu", src)]);
    let acts = h.fire(
        Hook::Type,
        &json!({"grapheme": "ế", "index": 3, "total": 10}),
        &vars(&[]),
    );
    assert_eq!(acts, vec![Action::PlaySfx { asset: "go".into() }]);
    let log = h.take_log();
    assert!(log.iter().any(|l| l.contains("set_var")), "{log:?}");
}

#[test]
fn filter_khong_duoc_play_sfx() {
    let src = r#"fn filter_text(m) { play_sfx("x"); m.text }"#;
    let mut h = host(&[("a", src)]);
    // filter_text không trả action; kiểm bằng log + text vẫn về đúng.
    assert_eq!(h.filter_text(None, "k", "t", &vars(&[])), "t");
    assert!(h.take_log().iter().any(|l| l.contains("play_sfx")));
}

#[test]
fn ext_dispatch_va_khong_ai_nhan() {
    let src = r#"fn ext_rung(args) { shake(args.px, args.ms); }"#;
    let mut h = host(&[("rung", src)]);
    let acts = h.ext("rung", &json!({"px": 6, "ms": 300}), &vars(&[]));
    assert_eq!(
        acts,
        Some(vec![Action::Shake {
            amplitude: 6.0,
            ms: 300
        }])
    );
    assert_eq!(h.ext("khong_co", &json!(null), &vars(&[])), None);
}

#[test]
fn vong_lap_vo_han_bi_ngan_sach_chan() {
    let mut h = host(&[("treo", "fn on_game_start() { loop {} }")]);
    let acts = h.fire(Hook::GameStart, &json!({}), &vars(&[])); // phải trả về, không treo
    assert!(acts.is_empty());
    assert!(!h.take_log().is_empty());
}

#[test]
fn set_var_thay_duoc_ngay_trong_cung_hook() {
    let src = r#"
        fn on_node_enter(m) {
            set_var("x", 2);
            set_var("y", get_var("x") + 1);
        }
    "#;
    let mut h = host(&[("a", src)]);
    let acts = h.fire(Hook::NodeEnter, &json!({"node": "n"}), &vars(&[]));
    assert!(acts.contains(&Action::SetVar {
        name: "y".into(),
        value: json!(3)
    }));
}
