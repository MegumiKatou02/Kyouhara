//! Test tích hợp M0 (DoD): JSON dự án → Story → mongpack → đọc lại → Story y hệt,
//! rồi máy ảo chạy truyện demo tới cả hai kết thúc.

use mong_assets::{read_pack, write_pack, EntryKind, PackEntry};
use mong_core::{Value, Vm, VmEvent, VmStatus};

const DEMO: &str = include_str!("../../mong-script/tests/data/demo-story.json");

#[test]
fn dod_m0_mongpack_round_trip() {
    let story = mong_script::load_story_json(DEMO).expect("json demo hop le");
    let issues = mong_script::validate(&story);
    assert!(
        issues.iter().all(|i| i.severity != mong_script::Severity::Error),
        "demo khong duoc co loi lint: {issues:?}"
    );

    let ir = serde_json::to_vec(&story).unwrap();
    let entries = vec![PackEntry {
        name: "story.ir".into(),
        kind: EntryKind::StoryIr,
        data: ir,
    }];
    let mut buf = Vec::new();
    write_pack(&mut buf, &entries).unwrap();

    let back = read_pack(&mut &buf[..]).unwrap();
    assert_eq!(back.len(), 1);
    let story2: mong_core::Story = serde_json::from_slice(&back[0].data).unwrap();
    assert_eq!(story, story2, "round-trip phai giu nguyen tung bit ngu nghia");
}

fn advance_to_choices(vm: &mut Vm) -> Vec<mong_core::PresentedChoice> {
    for _ in 0..20 {
        let evs = vm.advance().expect("advance hop le");
        if let Some(VmEvent::Choices { arms }) = evs.last() {
            return arms.clone();
        }
    }
    panic!("khong gap Choices sau 20 buoc");
}

fn last_say(evs: &[VmEvent]) -> Option<&str> {
    evs.iter().rev().find_map(|e| match e {
        VmEvent::Say { text, .. } => Some(text.as_str()),
        _ => None,
    })
}

#[test]
fn demo_chay_toi_ket_dep_khi_chao_truoc() {
    let story = mong_script::load_story_json(DEMO).unwrap();
    let mut vm = Vm::new(story).unwrap();
    let e = vm.start().unwrap();
    assert_eq!(last_say(&e), Some("mo_dau.l1"));
    let arms = advance_to_choices(&mut vm);
    assert_eq!(arms.len(), 2);
    vm.choose(0).unwrap();
    assert_eq!(vm.vars().get("thien_cam"), Some(&Value::Int(1)));
    // Ca hai lua chon deu hien vi thien_cam >= 1.
    let arms = advance_to_choices(&mut vm);
    assert_eq!(arms.len(), 2);
    let e = vm.choose(0).unwrap();
    assert!(e.iter().any(|x| matches!(x, VmEvent::SceneChanged { scene, .. } if scene == "san_thuong")));
    vm.advance().unwrap();
    vm.advance().unwrap();
    let e = vm.advance().unwrap();
    assert!(matches!(e.last(), Some(VmEvent::Ended)));
    assert_eq!(vm.status(), VmStatus::Ended);
}

#[test]
fn demo_an_lua_chon_khi_thieu_thien_cam() {
    let story = mong_script::load_story_json(DEMO).unwrap();
    let mut vm = Vm::new(story).unwrap();
    vm.start().unwrap();
    advance_to_choices(&mut vm);
    vm.choose(1).unwrap(); // lang tranh — khong cong thien cam
    let arms = advance_to_choices(&mut vm);
    assert_eq!(arms.len(), 1, "lua chon co dieu kien phai bi an");
    assert_eq!(arms[0].text, "loi_moi.c2");
}
