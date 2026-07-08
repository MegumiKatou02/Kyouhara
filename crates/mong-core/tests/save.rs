//! Test save slot có version, hash cốt truyện và fallback theo node id (DoD M1).

use mong_core::*;
use std::collections::BTreeMap;

fn say(text: &str) -> Instr {
    Instr::Say {
        speaker: None,
        text: text.into(),
        opts: SayOpts::default(),
    }
}

/// Story 2 node: a → (choice, effect x+=1) → b. `b1_text` để giả lập
/// "tác giả sửa thoại" giữa lúc save và lúc load.
fn story_v(b1_text: &str) -> Story {
    let mut vars = BTreeMap::new();
    vars.insert("x".to_string(), Value::Int(0));
    Story {
        format_version: 0,
        title: "save-test".into(),
        default_locale: "vi".into(),
        locales: vec![],
        variables: vars,
        start: "a".into(),
        nodes: vec![
            Node {
                id: "a".into(),
                title: String::new(),
                scene: None,
                body: vec![
                    say("a1"),
                    Instr::Choice {
                        arms: vec![ChoiceArm {
                            text: "di".into(),
                            target: Some("b".into()),
                            cond: None,
                            effects: vec![Effect {
                                var: "x".into(),
                                op: SetOp::Add,
                                value: Value::Int(1),
                            }],
                        }],
                    },
                ],
            },
            Node {
                id: "b".into(),
                title: String::new(),
                scene: None,
                body: vec![say(b1_text), Instr::End],
            },
        ],
    }
}

/// Chơi tới điểm chờ đầu tiên trong node b rồi lưu.
fn play_and_save(story: Story) -> SaveSlot {
    let mut vm = Vm::new(story).unwrap();
    vm.start().unwrap();
    vm.advance().unwrap();
    vm.choose(0).unwrap(); // vào b, x = 1, dừng ở Say b1
    let slot = vm
        .save("slot-1", Some("2026-07-08T00:00:00Z".into()))
        .unwrap();
    assert_eq!(slot.node, "b");
    assert_eq!(slot.save_version, SAVE_VERSION);
    slot
}

#[test]
fn hash_on_dinh_va_nhay_voi_thay_doi() {
    assert_eq!(story_v("b1").hash64(), story_v("b1").hash64());
    assert_ne!(story_v("b1").hash64(), story_v("b1_moi").hash64());
}

#[test]
fn load_exact_khi_cot_truyen_y_nguyen() {
    let slot = play_and_save(story_v("b1"));
    let mut vm = Vm::new(story_v("b1")).unwrap();
    match vm.load(&slot).unwrap() {
        LoadOutcome::Exact(replay) => {
            assert!(matches!(replay.last(), Some(VmEvent::Say { text, .. }) if text == "b1"));
        }
        other => panic!("phai la Exact, nhan {other:?}"),
    }
    assert_eq!(vm.status(), VmStatus::AwaitAdvance);
    assert_eq!(vm.vars().get("x"), Some(&Value::Int(1)));
    let e = vm.advance().unwrap();
    assert!(matches!(e.last(), Some(VmEvent::Ended)));
}

#[test]
fn load_fallback_khi_cot_truyen_da_doi() {
    let slot = play_and_save(story_v("b1"));
    let mut vm = Vm::new(story_v("b1_moi")).unwrap(); // tác giả đã sửa thoại
    match vm.load(&slot).unwrap() {
        LoadOutcome::NodeFallback { node, events } => {
            assert_eq!(node, "b");
            assert!(matches!(&events[0], VmEvent::NodeEntered { node } if node == "b"));
            assert!(
                matches!(events.last(), Some(VmEvent::Say { text, .. }) if text == "b1_moi"),
                "phai chay lai node b theo body MOI"
            );
        }
        other => panic!("phai la NodeFallback, nhan {other:?}"),
    }
    // Biến từ save được giữ nguyên (x ghi ở node a, node b không đụng tới).
    assert_eq!(vm.vars().get("x"), Some(&Value::Int(1)));
    assert_eq!(vm.status(), VmStatus::AwaitAdvance);
}

#[test]
fn load_loi_khi_node_bien_mat() {
    let slot = play_and_save(story_v("b1"));
    let mut story = story_v("b1");
    story.nodes.retain(|n| n.id != "b"); // tác giả xoá hẳn node b
    let mut vm = Vm::new(story).unwrap();
    assert_eq!(vm.load(&slot), Err(VmError::UnknownNode("b".into())));
}

#[test]
fn load_tu_choi_save_version_la() {
    let mut slot = play_and_save(story_v("b1"));
    slot.save_version = 999;
    let mut vm = Vm::new(story_v("b1")).unwrap();
    assert_eq!(vm.load(&slot), Err(VmError::BadSaveVersion(999)));
}

#[test]
fn save_slot_serde_round_trip() {
    let slot = play_and_save(story_v("b1"));
    let json = serde_json::to_string(&slot).unwrap();
    let back: SaveSlot = serde_json::from_str(&json).unwrap();
    assert_eq!(slot, back);
}

#[test]
fn save_truoc_start_la_none() {
    let vm = Vm::new(story_v("b1")).unwrap();
    assert!(vm.save("s", None).is_none());
}
