//! Dựng Loaded trong bộ nhớ → pack → load_pack: bằng nhau. Không cần fs,
//! nên chạy được cả trên wasm.

use mong_assets::Manifest;
use mong_core::{Instr, Node, SayOpts, Story, FORMAT_VERSION};
use mong_i18n::Table;
use mong_project::{load_pack, to_pack_bytes, Loaded};
use std::collections::BTreeMap;

const MANIFEST: &str = r#"{
  "format_version": 2,
  "scenes": { "quan": { "bg": "bg_quan" } },
  "characters": {},
  "assets": {
    "bg_quan": { "path": "bg/quan.png", "kind": "image" },
    "font_vi": { "path": "fonts/x.ttf", "kind": "font" }
  },
  "fonts": { "vi": ["font_vi"] },
  "strings": { "vi": { "quan.name": "Quán cà phê" } }
}"#;

fn loaded() -> Loaded {
    let story = Story {
        format_version: FORMAT_VERSION,
        title: "Quán cà phê".into(),
        default_locale: "vi".into(),
        locales: vec!["en".into()],
        variables: Default::default(),
        start: "a".into(),
        nodes: vec![Node {
            id: "a".into(),
            title: String::new(),
            scene: None,
            body: vec![
                Instr::Scene {
                    scene: "quan".into(),
                    transition: Some("fade".into()),
                },
                Instr::Say {
                    speaker: None,
                    text: "a.l1".into(),
                    opts: SayOpts::default(),
                },
                Instr::End,
            ],
        }],
    };
    let mut strings = BTreeMap::new();
    strings.insert(
        "vi".to_string(),
        Table::from([("a.l1".into(), "Nắng chiều.".into())]),
    );
    strings.insert(
        "en".to_string(),
        Table::from([("a.l1".into(), "Afternoon sun.".into())]),
    );

    let mut assets = BTreeMap::new();
    assets.insert("bg_quan".to_string(), vec![0x89, b'P', b'N', b'G']);
    assets.insert("font_vi".to_string(), vec![0x00, 0x01, 0x00, 0x00]);

    Loaded {
        story,
        strings,
        manifest: Manifest::parse(MANIFEST).unwrap(),
        locale: "vi".into(),
        assets,
        plugins: BTreeMap::new(),
    }
}

/// Plugin đi qua gói nguyên vẹn, id tách đúng từ tên entry.
#[test]
fn plugin_round_trip_qua_goi() {
    let mut l = loaded();
    l.plugins
        .insert("rung".into(), "fn on_game_start() {}".into());
    let back = load_pack(&to_pack_bytes(&l).unwrap(), Some("vi")).unwrap();
    assert_eq!(l, back);
}

#[test]
fn pack_roi_load_ra_dung_thu_cu() {
    let l = loaded();
    let bytes = to_pack_bytes(&l).unwrap();
    let back = load_pack(&bytes, Some("vi")).unwrap();
    assert_eq!(l, back);
}

/// Cùng dự án → cùng byte. Không có nó thì CI không so hash được.
#[test]
fn pack_xac_dinh() {
    let l = loaded();
    assert_eq!(to_pack_bytes(&l).unwrap(), to_pack_bytes(&l).unwrap());
}

/// `manifest.strings` không lọt vào entry Strings (miền key tách bạch),
/// nhưng vẫn tra được qua Catalog sau khi load.
#[test]
fn mien_key_metadata_khong_tron_vao_bang_noi_dung() {
    let back = load_pack(&to_pack_bytes(&loaded()).unwrap(), None).unwrap();
    assert!(!back.strings["vi"].contains_key("quan.name"));
    assert_eq!(back.catalog().text_or_key("vi", "quan.name"), "Quán cà phê");
}

#[test]
fn thieu_asset_thi_tu_choi_pack() {
    let mut l = loaded();
    l.assets.remove("font_vi");
    assert!(to_pack_bytes(&l).is_err());
}

#[test]
fn locale_la_bi_tu_choi() {
    let bytes = to_pack_bytes(&loaded()).unwrap();
    assert!(load_pack(&bytes, Some("ja")).is_err());
}
