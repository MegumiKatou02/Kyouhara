//! Fuzz-lite (DoD M1: "fuzz input không panic") — driver giả-ngẫu-nhiên có seed,
//! chạy trên stable, vào thẳng CI. Nuốt mọi `Result` (lỗi là hành vi đúng),
//! chỉ canh panic. Mỗi ca fail in seed để tái lập chính xác.
//!
//! Chạy dài hơn cục bộ:
//!   MONG_FUZZ_ITERS=20000 MONG_FUZZ_SEED=123 cargo test -p mong-core --test fuzz_lite

use mong_core::*;
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

const DEMO: &str = include_str!("../../mong-script/tests/data/demo-story.json");

/// SplitMix64 — nhân bản có chủ đích từ vm.rs: PRNG của VM là private, và
/// fuzz driver không nên phụ thuộc nội bộ của chính thứ nó đang kiểm.
struct TestRng(u64);

impl TestRng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1) // bias của modulo không quan trọng với fuzz
    }
    fn i64_in(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn run_case(seed: u64, f: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(f)).is_err() {
        panic!("FUZZ FAIL — tai lap: MONG_FUZZ_SEED={seed} MONG_FUZZ_ITERS=1");
    }
}

// ---- sinh cốt truyện ngẫu nhiên (cố ý gồm cả cấu trúc mà lint sẽ chặn) ----

fn pick_node(r: &mut TestRng) -> String {
    ["a", "b", "c", "ma"][r.below(4) as usize].into()
}
fn pick_var(r: &mut TestRng) -> String {
    ["x", "y", "chua_khai_bao"][r.below(3) as usize].into()
}
fn pick_label(r: &mut TestRng) -> String {
    ["l1", "l2", "l_treo"][r.below(3) as usize].into()
}

fn gen_value(r: &mut TestRng) -> Value {
    match r.below(3) {
        0 => Value::Int(r.i64_in(-5, 5)),
        1 => Value::Bool(r.below(2) == 0),
        _ => Value::Str("s".into()),
    }
}

fn gen_cond(r: &mut TestRng) -> Cond {
    Cond {
        var: pick_var(r),
        op: [CondOp::Ge, CondOp::Le, CondOp::Eq, CondOp::Ne][r.below(4) as usize],
        value: gen_value(r),
    }
}

fn gen_expr(r: &mut TestRng, depth: u32) -> Expr {
    if depth == 0 || r.below(3) == 0 {
        return if r.below(2) == 0 {
            Expr::Lit(gen_value(r))
        } else {
            Expr::Var(pick_var(r))
        };
    }
    if r.below(4) == 0 {
        Expr::Neg(Box::new(gen_expr(r, depth - 1)))
    } else {
        Expr::Bin {
            op: [BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div, BinOp::Rem][r.below(5) as usize],
            lhs: Box::new(gen_expr(r, depth - 1)),
            rhs: Box::new(gen_expr(r, depth - 1)),
        }
    }
}

fn gen_arm(r: &mut TestRng) -> ChoiceArm {
    ChoiceArm {
        text: "c".into(),
        target: if r.below(4) == 0 {
            None
        } else {
            Some(pick_node(r))
        },
        cond: if r.below(2) == 0 {
            Some(gen_cond(r))
        } else {
            None
        },
        effects: vec![Effect {
            var: pick_var(r),
            op: [SetOp::Assign, SetOp::Add, SetOp::Sub, SetOp::Toggle][r.below(4) as usize],
            value: gen_value(r),
        }],
    }
}

fn gen_instr(r: &mut TestRng, depth: u32) -> Instr {
    match r.below(16) {
        0 => Instr::Say {
            speaker: None,
            text: "t".into(),
            opts: SayOpts::default(),
        },
        1 => Instr::Show {
            character: "c".into(),
            pose: None,
            pos: StagePos::Center,
        },
        2 => Instr::Hide {
            character: "c".into(),
        },
        3 => Instr::Scene {
            scene: "s".into(),
            transition: None,
        },
        4 => Instr::Choice {
            arms: (0..r.below(3)).map(|_| gen_arm(r)).collect(),
        },
        5 => Instr::Jump {
            target: pick_node(r),
        },
        6 => Instr::Call {
            target: pick_node(r),
        },
        7 => Instr::Return,
        8 => Instr::Set {
            effect: Effect {
                var: pick_var(r),
                op: [SetOp::Assign, SetOp::Add, SetOp::Sub, SetOp::Toggle][r.below(4) as usize],
                value: gen_value(r),
            },
        },
        9 => Instr::Wait {
            ms: r.below(50) as u32,
        },
        10 => Instr::Rand {
            var: pick_var(r),
            min: r.i64_in(-3, 3),
            max: r.i64_in(-3, 3),
        },
        11 => Instr::Label {
            name: pick_label(r),
        },
        12 => Instr::Goto {
            label: pick_label(r),
        },
        13 => Instr::SetExpr {
            var: pick_var(r),
            expr: gen_expr(r, 3),
        },
        14 if depth > 0 => Instr::If {
            cond: gen_cond(r),
            then_branch: gen_body(r, depth - 1),
            else_branch: gen_body(r, depth - 1),
        },
        _ => Instr::End,
    }
}

fn gen_body(r: &mut TestRng, depth: u32) -> Vec<Instr> {
    (0..r.below(7)).map(|_| gen_instr(r, depth)).collect()
}

fn gen_story(r: &mut TestRng) -> Story {
    let n = 1 + r.below(3) as usize;
    let nodes = ["a", "b", "c"][..n]
        .iter()
        .map(|id| Node {
            id: (*id).into(),
            title: String::new(),
            scene: None,
            body: gen_body(r, 3),
        })
        .collect();
    let mut vars = BTreeMap::new();
    vars.insert("x".to_string(), Value::Int(0));
    vars.insert("y".to_string(), Value::Bool(false));
    Story {
        format_version: 1,
        title: String::new(),
        default_locale: "vi".into(),
        locales: vec![],
        variables: vars,
        start: if r.below(8) == 0 {
            "ma".into()
        } else {
            "a".into()
        },
        nodes,
    }
}

// ---- driver input ----

/// Gõ input bừa vào VM: sai trạng thái, chỉ số ngoài khoảng, rollback cạn,
/// save/load giữa chừng, gõ tiếp cả sau khi Ended hoặc sau khi VM báo lỗi.
fn drive(story: Story, r: &mut TestRng) {
    let Ok(mut vm) = Vm::new(story) else { return }; // từ chối story là hành vi đúng
    if r.below(2) == 0 {
        vm.set_seed(r.next());
    }
    vm.set_step_budget(2_000); // vòng goto vô hạn sinh ngẫu nhiên phải chết nhanh
    let mut stash: Option<Snapshot> = None;
    let _ = vm.start();
    for _ in 0..150 {
        match r.below(12) {
            0..=4 => {
                let _ = vm.advance();
            }
            5..=7 => {
                let _ = vm.choose(r.below(6) as usize);
            }
            8 => {
                let _ = vm.rollback();
            }
            9 => stash = vm.snapshot(),
            10 => {
                if let Some(s) = &stash {
                    let _ = vm.restore(s);
                }
            }
            _ => {
                if let Some(slot) = vm.save("f", None) {
                    let _ = vm.load(&slot);
                }
            }
        }
        if vm.status() == VmStatus::Ended && r.below(4) != 0 {
            break; // thỉnh thoảng vẫn ở lại gõ tiếp sau Ended
        }
    }
}

// ---- làm hỏng save slot ----

/// Xáo trộn cây JSON: đổi số bừa (chỉ số con trỏ, version, hash...),
/// lật bool, cắt cụt mảng. Không đụng chuỗi — hỏng chuỗi đa phần chỉ
/// làm serde từ chối, không tới được đường chạy.
fn mutate_json(v: &mut serde_json::Value, r: &mut TestRng) {
    use serde_json::Value as J;
    match v {
        J::Number(_) if r.below(3) == 0 => *v = J::from(r.below(10_000)),
        J::Bool(b) if r.below(4) == 0 => *b = !*b,
        J::Array(a) => {
            if r.below(5) == 0 && !a.is_empty() {
                let keep = r.below(a.len() as u64) as usize;
                a.truncate(keep);
            }
            for x in a.iter_mut() {
                mutate_json(x, r);
            }
        }
        J::Object(o) => {
            for (_, x) in o.iter_mut() {
                mutate_json(x, r);
            }
        }
        _ => {}
    }
}

// ---- ba bài fuzz ----

#[test]
fn fuzz_input_tren_truyen_hop_le() {
    let base = env_u64("MONG_FUZZ_SEED", 0xC0FF_EE01);
    let cases = env_u64("MONG_FUZZ_ITERS", 300);
    let demo: Story = serde_json::from_str(DEMO).expect("json demo hop le");
    for i in 0..cases {
        let seed = base.wrapping_add(i.wrapping_mul(0x9e37_79b9));
        let demo = demo.clone();
        run_case(seed, move || drive(demo, &mut TestRng(seed)));
    }
}

#[test]
fn fuzz_story_ngau_nhien_khong_panic() {
    let base = env_u64("MONG_FUZZ_SEED", 0xBAD5_EED0);
    let cases = env_u64("MONG_FUZZ_ITERS", 500);
    for i in 0..cases {
        let seed = base.wrapping_add(i.wrapping_mul(0x9e37_79b9));
        run_case(seed, move || {
            let mut r = TestRng(seed);
            let story = gen_story(&mut r);
            drive(story, &mut r);
        });
    }
}

#[test]
fn fuzz_save_hong_khong_panic() {
    let base = env_u64("MONG_FUZZ_SEED", 0x5A0E_F11E);
    let cases = env_u64("MONG_FUZZ_ITERS", 300);
    let demo: Story = serde_json::from_str(DEMO).expect("json demo hop le");
    let mut vm = Vm::new(demo.clone()).unwrap();
    vm.start().unwrap();
    let _ = vm.advance();
    let _ = vm.advance();
    let json = serde_json::to_value(vm.save("goc", None).unwrap()).unwrap();
    for i in 0..cases {
        let seed = base.wrapping_add(i.wrapping_mul(0x9e37_79b9));
        let (demo, json) = (demo.clone(), json.clone());
        run_case(seed, move || {
            let mut r = TestRng(seed);
            let mut j = json;
            mutate_json(&mut j, &mut r);
            // serde từ chối là hành vi đúng; parse được thì load + chơi tiếp
            // cũng không được panic — đây là bài ép ra CorruptSnapshot.
            let Ok(bad) = serde_json::from_value::<SaveSlot>(j) else {
                return;
            };
            let mut vm = Vm::new(demo).unwrap();
            let _ = vm.load(&bad);
            for _ in 0..20 {
                match r.below(3) {
                    0 => {
                        let _ = vm.advance();
                    }
                    1 => {
                        let _ = vm.choose(r.below(4) as usize);
                    }
                    _ => {
                        let _ = vm.rollback();
                    }
                }
            }
        });
    }
}
