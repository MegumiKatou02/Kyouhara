//! Test tích hợp máy ảo: chuỗi sự kiện, rollback, tính xác định.

use mong_core::*;
use std::collections::BTreeMap;

fn say(text: &str) -> Instr {
    Instr::Say {
        speaker: None,
        text: text.into(),
        opts: SayOpts::default(),
    }
}

fn mini_story() -> Story {
    let mut vars = BTreeMap::new();
    vars.insert("tc".to_string(), Value::Int(0));
    Story {
        format_version: 0,
        title: "mini".into(),
        default_locale: "vi".into(),
        locales: vec![],
        variables: vars,
        start: "a".into(),
        nodes: vec![
            Node {
                id: "a".into(),
                title: "A".into(),
                scene: None,
                body: vec![
                    say("a1"),
                    Instr::Choice {
                        arms: vec![
                            ChoiceArm {
                                text: "chao".into(),
                                target: Some("b".into()),
                                cond: None,
                                effects: vec![Effect {
                                    var: "tc".into(),
                                    op: SetOp::Add,
                                    value: Value::Int(1),
                                }],
                            },
                            ChoiceArm {
                                text: "lang".into(),
                                target: Some("c".into()),
                                cond: None,
                                effects: vec![],
                            },
                        ],
                    },
                ],
            },
            Node {
                id: "b".into(),
                title: "B".into(),
                scene: None,
                body: vec![
                    Instr::If {
                        cond: Cond {
                            var: "tc".into(),
                            op: CondOp::Ge,
                            value: Value::Int(1),
                        },
                        then_branch: vec![say("b_than")],
                        else_branch: vec![say("b_thuong")],
                    },
                    Instr::End,
                ],
            },
            Node {
                id: "c".into(),
                title: "C".into(),
                scene: None,
                body: vec![say("c1")],
            },
        ],
    }
}

fn sig(evs: &[VmEvent]) -> String {
    serde_json::to_string(evs).unwrap()
}

#[test]
fn chuoi_su_kien_nhanh_chao() {
    let mut vm = Vm::new(mini_story()).unwrap();
    let e1 = vm.start().unwrap();
    assert!(matches!(e1.last(), Some(VmEvent::Say { text, .. }) if text == "a1"));
    assert_eq!(vm.status(), VmStatus::AwaitAdvance);

    let e2 = vm.advance().unwrap();
    assert!(matches!(e2.last(), Some(VmEvent::Choices { arms }) if arms.len() == 2));

    let e3 = vm.choose(0).unwrap();
    assert_eq!(vm.vars().get("tc"), Some(&Value::Int(1)));
    // Nhánh if phải rẽ vào then vì tc == 1.
    assert!(matches!(e3.last(), Some(VmEvent::Say { text, .. }) if text == "b_than"));

    let e4 = vm.advance().unwrap();
    assert!(matches!(e4.last(), Some(VmEvent::Ended)));
    assert_eq!(vm.status(), VmStatus::Ended);
}

#[test]
fn nhanh_lang_di_qua_else_khi_quay_lai() {
    let mut vm = Vm::new(mini_story()).unwrap();
    vm.start().unwrap();
    vm.advance().unwrap();
    let e = vm.choose(1).unwrap();
    assert!(matches!(e.last(), Some(VmEvent::Say { text, .. }) if text == "c1"));
    // Node c hết body, không End tường minh -> tự Ended.
    let e2 = vm.advance().unwrap();
    assert!(matches!(e2.last(), Some(VmEvent::Ended)));
}

#[test]
fn rollback_khoi_phuc_ca_bien() {
    let mut vm = Vm::new(mini_story()).unwrap();
    vm.start().unwrap();
    vm.advance().unwrap();
    vm.choose(0).unwrap();
    assert_eq!(vm.vars().get("tc"), Some(&Value::Int(1)));

    // Lùi một bước: về màn hình lựa chọn, tc phải trở lại 0.
    let replay = vm.rollback().expect("phai lui duoc");
    assert_eq!(vm.status(), VmStatus::AwaitChoice);
    assert_eq!(vm.vars().get("tc"), Some(&Value::Int(0)));
    assert!(matches!(replay.last(), Some(VmEvent::Choices { .. })));

    // Chọn lại nhánh kia — thế giới rẽ hướng khác hoàn toàn hợp lệ.
    let e = vm.choose(1).unwrap();
    assert!(matches!(e.last(), Some(VmEvent::Say { text, .. }) if text == "c1"));
}

#[test]
fn xac_dinh_sau_restore() {
    // Chạy lần 1, lưu snapshot tại màn lựa chọn, ghi lại "đuôi" sự kiện.
    let mut vm = Vm::new(mini_story()).unwrap();
    vm.start().unwrap();
    vm.advance().unwrap();
    let snap = vm.snapshot().expect("co snapshot");
    let tail1 = {
        let mut t = vm.choose(0).unwrap();
        t.extend(vm.advance().unwrap());
        sig(&t)
    };
    // Khôi phục và chạy lại cùng input — đuôi sự kiện phải giống hệt.
    let replay = vm.restore(&snap);
    assert!(matches!(replay.last(), Some(VmEvent::Choices { .. })));
    let tail2 = {
        let mut t = vm.choose(0).unwrap();
        t.extend(vm.advance().unwrap());
        sig(&t)
    };
    assert_eq!(tail1, tail2, "thuc thi phai xac dinh");
}

#[test]
fn call_return_quay_ve_dung_cho() {
    let story = Story {
        format_version: 0,
        title: "call".into(),
        default_locale: "vi".into(),
        locales: vec![],
        variables: BTreeMap::new(),
        start: "main".into(),
        nodes: vec![
            Node {
                id: "main".into(),
                title: String::new(),
                scene: None,
                body: vec![
                    Instr::Call {
                        target: "sub".into(),
                    },
                    say("sau_call"),
                    Instr::End,
                ],
            },
            Node {
                id: "sub".into(),
                title: String::new(),
                scene: None,
                body: vec![say("trong_sub"), Instr::Return],
            },
        ],
    };
    let mut vm = Vm::new(story).unwrap();
    let e1 = vm.start().unwrap();
    assert!(matches!(e1.last(), Some(VmEvent::Say { text, .. }) if text == "trong_sub"));
    let e2 = vm.advance().unwrap();
    assert!(matches!(e2.last(), Some(VmEvent::Say { text, .. }) if text == "sau_call"));
}

#[test]
fn ir_serde_round_trip() {
    let story = mini_story();
    let json = serde_json::to_string(&story).unwrap();
    let back: Story = serde_json::from_str(&json).unwrap();
    assert_eq!(story, back);
}
