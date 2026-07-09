//! Sân khấu phản ứng đúng theo VmEvent, và rollback trả sân khấu về đúng
//! thời điểm VM dừng (RFC-001: core không lưu stage).

use mong_assets::Manifest;
use mong_core::{ChoiceArm, Instr, Node, SayOpts, StagePos, Story, FORMAT_VERSION};
use mong_i18n::{Catalog, Table};
use mong_runtime::{AudioCmd, Input, Runtime};

const MANIFEST: &str = r#"{
  "format_version": 1,
  "scenes": {
    "quan": { "bg": "bg_quan", "bgm": "nhac_quan" },
    "san_thuong": { "bg": "bg_st" }
  },
  "characters": {
    "lan": { "layers": [
      { "kind": "base", "default": "than", "variants": { "than": "lan_than" } },
      { "kind": "face", "default": "thuong",
        "variants": { "thuong": "lan_thuong", "vui": "lan_vui" } }
    ] }
  },
  "assets": {
    "bg_quan": { "path": "a.png", "kind": "image" },
    "bg_st": { "path": "b.png", "kind": "image" },
    "lan_than": { "path": "c.png", "kind": "image" },
    "lan_thuong": { "path": "d.png", "kind": "image" },
    "lan_vui": { "path": "e.png", "kind": "image" },
    "nhac_quan": { "path": "f.ogg", "kind": "audio" }
  }
}"#;

fn say(speaker: Option<&str>, key: &str, opts: SayOpts) -> Instr {
    Instr::Say {
        speaker: speaker.map(String::from),
        text: key.into(),
        opts,
    }
}

fn story() -> Story {
    Story {
        format_version: FORMAT_VERSION,
        title: String::new(),
        default_locale: "vi".into(),
        locales: vec![],
        variables: Default::default(),
        start: "a".into(),
        nodes: vec![
            Node {
                id: "a".into(),
                title: String::new(),
                scene: None,
                body: vec![
                    Instr::Scene {
                        scene: "quan".into(),
                        transition: Some("fade".into()),
                    },
                    say(
                        Some("lan"),
                        "l1",
                        SayOpts {
                            pose: Some("vui".into()),
                            pos: Some(StagePos::Left),
                            ..Default::default()
                        },
                    ),
                    say(
                        Some("lan"),
                        "l2",
                        SayOpts {
                            exit: true,
                            ..Default::default()
                        },
                    ),
                    Instr::Choice {
                        arms: vec![ChoiceArm {
                            text: "c1".into(),
                            target: Some("b".into()),
                            cond: None,
                            effects: vec![],
                        }],
                    },
                ],
            },
            Node {
                id: "b".into(),
                title: String::new(),
                scene: None,
                body: vec![
                    Instr::Scene {
                        scene: "san_thuong".into(),
                        transition: None,
                    },
                    Instr::End,
                ],
            },
        ],
    }
}

fn runtime() -> Runtime {
    let mut cat = Catalog::new("vi");
    cat.set_table(
        "vi",
        Table::from([
            ("l1".into(), "Ơ… Minh?".into()),
            ("l2".into(), "Chào nhé.".into()),
            ("c1".into(), "Đi thôi".into()),
        ]),
    );
    Runtime::new(story(), cat, Manifest::parse(MANIFEST).unwrap(), "vi").unwrap()
}

#[test]
fn scene_dat_nen_va_phat_bgm_khai_bao() {
    let mut rt = runtime();
    rt.start().unwrap();
    assert_eq!(rt.stage().bg.as_deref(), Some("bg_quan"));
    assert!(rt.stage().transition.is_some(), "fade phai dang chay");
    assert_eq!(rt.take_audio(), vec![AudioCmd::Bgm(Some("nhac_quan".into()))]);
}

#[test]
fn say_opts_dua_nhan_vat_len_san_khau() {
    let mut rt = runtime();
    rt.start().unwrap();
    let c = &rt.stage().chars[0];
    assert_eq!((c.id.as_str(), c.pose.as_deref(), c.pos), ("lan", Some("vui"), StagePos::Left));
    assert!(!c.dim, "nguoi dang noi khong bi lam toi");
}

#[test]
fn typewriter_chay_theo_thoi_gian_shell_cap() {
    let mut rt = runtime();
    rt.set_cps(4.0);
    rt.start().unwrap();
    rt.tick(0.5).unwrap(); // 2 grapheme
    assert_eq!(rt.line().unwrap().visible(), "Ơ…");
    rt.input(Input::Advance).unwrap(); // click thu nhat: hien het dong
    assert_eq!(rt.line().unwrap().visible(), "Ơ… Minh?");
    assert_eq!(rt.stage().chars.len(), 1, "chua sang dong moi");
}

#[test]
fn exit_giau_nguoi_noi_khi_bo_qua_dong() {
    let mut rt = runtime();
    rt.start().unwrap();
    rt.input(Input::Advance).unwrap(); // reveal l1
    rt.input(Input::Advance).unwrap(); // sang l2 (exit)
    rt.input(Input::Advance).unwrap(); // reveal l2
    rt.input(Input::Advance).unwrap(); // bo qua l2 -> lan roi san khau
    assert!(rt.stage().chars.is_empty());
    assert_eq!(rt.choices().len(), 1);
}

#[test]
fn rollback_tra_san_khau_ve_dung_thoi_diem() {
    let mut rt = runtime();
    rt.start().unwrap();
    rt.input(Input::Advance).unwrap();
    rt.input(Input::Advance).unwrap(); // dang o l2, lan con tren san khau
    rt.input(Input::Advance).unwrap();
    rt.input(Input::Advance).unwrap(); // l2 bi bo qua -> lan bien mat
    assert!(rt.stage().chars.is_empty());

    rt.input(Input::Rollback).unwrap();
    assert_eq!(rt.stage().chars.len(), 1, "lui lai thi Lan phai co mat");
    assert_eq!(rt.line().unwrap().text, "Chào nhé.");
}

#[test]
fn scene_moi_don_sach_san_khau() {
    let mut rt = runtime();
    rt.start().unwrap();
    for _ in 0..4 {
        rt.input(Input::Advance).unwrap();
    }
    rt.take_audio();
    rt.input(Input::Choose(0)).unwrap();
    assert_eq!(rt.stage().bg.as_deref(), Some("bg_st"));
    assert!(rt.stage().chars.is_empty());
    assert!(rt.stage().transition.is_none(), "khong khai bao = cut");
    assert!(rt.take_audio().is_empty(), "san_thuong khong khai bao bgm");
}
