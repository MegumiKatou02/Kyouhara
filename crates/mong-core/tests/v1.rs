//! Test ba lệnh v1 (rand / label+goto / set_expr) + ngân sách bước + tương thích save.

use mong_core::*;
use std::collections::BTreeMap;

fn say(t: &str) -> Instr {
    Instr::Say {
        speaker: None,
        text: t.into(),
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
fn story(nodes: Vec<Node>, vars: &[(&str, i64)]) -> Story {
    let mut m = BTreeMap::new();
    for (k, v) in vars {
        m.insert(k.to_string(), Value::Int(*v));
    }
    Story {
        format_version: 1,
        title: String::new(),
        default_locale: "vi".into(),
        locales: vec![],
        variables: m,
        start: "a".into(),
        nodes,
    }
}
fn set_expr(var: &str, expr: Expr) -> Instr {
    Instr::SetExpr {
        var: var.into(),
        expr,
    }
}
fn bin(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    Expr::Bin {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}
fn lit(n: i64) -> Expr {
    Expr::Lit(Value::Int(n))
}
fn var(n: &str) -> Expr {
    Expr::Var(n.into())
}
fn int(vm: &Vm, name: &str) -> i64 {
    match vm.vars().get(name) {
        Some(Value::Int(n)) => *n,
        other => panic!("bien '{name}' phai la Int, nhan {other:?}"),
    }
}

/// say s1; rand d; say s2 — dừng hai nhịp để quan sát d giữa chừng.
fn story_rand(min: i64, max: i64) -> Story {
    story(
        vec![node(
            "a",
            vec![
                say("s1"),
                Instr::Rand {
                    var: "d".into(),
                    min,
                    max,
                },
                say("s2"),
                Instr::End,
            ],
        )],
        &[("d", 0)],
    )
}

#[test]
fn rand_xac_dinh_cung_seed_va_doi_theo_seed() {
    let draw = |seed: Option<u64>| {
        let mut vm = Vm::new(story_rand(1, 1_000_000_000)).unwrap();
        if let Some(s) = seed {
            vm.set_seed(s);
        }
        vm.start().unwrap();
        vm.advance().unwrap();
        int(&vm, "d")
    };
    let d = draw(None);
    assert!((1..=1_000_000_000).contains(&d));
    assert_eq!(d, draw(None), "cung seed mac dinh phai ra cung gia tri");
    assert_ne!(
        d,
        draw(Some(42)),
        "seed khac phai ra chuoi khac (cap seed nay da kiem)"
    );
}

#[test]
fn rollback_tai_lap_cung_gia_tri_rand() {
    let mut vm = Vm::new(story_rand(1, 1_000_000_000)).unwrap();
    vm.start().unwrap(); // dừng ở s1, PRNG chưa rút
    vm.advance().unwrap(); // rand chạy, dừng ở s2
    let d1 = int(&vm, "d");
    vm.rollback().expect("lui ve diem s1");
    vm.advance().unwrap(); // rand chạy lại từ cùng trạng thái PRNG
    assert_eq!(
        int(&vm, "d"),
        d1,
        "PRNG nam trong snapshot — rollback phai tai lap"
    );
}

#[test]
fn rand_khoang_mot_phan_tu_va_khoang_cuc_dai() {
    let mut vm = Vm::new(story_rand(5, 5)).unwrap();
    vm.start().unwrap();
    vm.advance().unwrap();
    assert_eq!(int(&vm, "d"), 5);
    // Toàn dải i64: không panic, không tràn phép map 128-bit.
    let mut vm = Vm::new(story_rand(i64::MIN, i64::MAX)).unwrap();
    vm.start().unwrap();
    vm.advance().unwrap();
}

#[test]
fn rand_khoang_nguoc_la_loi() {
    let mut vm = Vm::new(story_rand(3, 1)).unwrap();
    vm.start().unwrap();
    assert_eq!(vm.advance(), Err(VmError::BadRandRange { min: 3, max: 1 }));
}

#[test]
fn goto_vong_lap_va_rand_luon_trong_khoang() {
    // Tự kiểm trong truyện: 50 vòng goto (từ trong nhánh if ra label top-level),
    // mỗi vòng rút d ∈ [-3,3]; lọt ra ngoài khoảng thì jump sang node "hong".
    let ge = |v: &str, n: i64| Cond {
        var: v.into(),
        op: CondOp::Ge,
        value: Value::Int(n),
    };
    let le = |v: &str, n: i64| Cond {
        var: v.into(),
        op: CondOp::Le,
        value: Value::Int(n),
    };
    let iff = |cond: Cond, then: Vec<Instr>| Instr::If {
        cond,
        then_branch: then,
        else_branch: vec![],
    };
    let s = story(
        vec![
            node(
                "a",
                vec![
                    Instr::Label {
                        name: "vong".into(),
                    },
                    Instr::Rand {
                        var: "d".into(),
                        min: -3,
                        max: 3,
                    },
                    iff(
                        ge("d", 4),
                        vec![Instr::Jump {
                            target: "hong".into(),
                        }],
                    ),
                    iff(
                        le("d", -4),
                        vec![Instr::Jump {
                            target: "hong".into(),
                        }],
                    ),
                    set_expr("i", bin(BinOp::Add, var("i"), lit(1))),
                    iff(
                        le("i", 50),
                        vec![Instr::Goto {
                            label: "vong".into(),
                        }],
                    ),
                    say("a.ok"),
                    Instr::End,
                ],
            ),
            node("hong", vec![say("hong.bug"), Instr::End]),
        ],
        &[("d", 0), ("i", 0)],
    );
    let mut vm = Vm::new(s).unwrap();
    let ev = vm.start().unwrap();
    assert!(
        matches!(ev.last(), Some(VmEvent::Say { text, .. }) if text == "a.ok"),
        "co gia tri rand lot ngoai khoang: {ev:?}"
    );
    assert_eq!(int(&vm, "i"), 51);
}

#[test]
fn goto_vo_han_cham_ngan_sach_buoc() {
    let s = story(
        vec![node(
            "a",
            vec![
                Instr::Label { name: "l".into() },
                Instr::Goto { label: "l".into() },
            ],
        )],
        &[],
    );
    let mut vm = Vm::new(s).unwrap();
    vm.set_step_budget(100);
    assert_eq!(vm.start(), Err(VmError::StepBudgetExceeded(100)));
}

#[test]
fn goto_toi_label_la_la_loi() {
    let s = story(
        vec![node("a", vec![Instr::Goto { label: "ma".into() }])],
        &[],
    );
    let mut vm = Vm::new(s).unwrap();
    assert_eq!(
        vm.start(),
        Err(VmError::UnknownLabel {
            node: "a".into(),
            label: "ma".into()
        })
    );
}

#[test]
fn set_expr_so_hoc_bao_hoa_va_bien_chua_co() {
    let s = story(
        vec![node(
            "a",
            vec![
                set_expr(
                    "x",
                    bin(BinOp::Mul, bin(BinOp::Add, lit(2), lit(3)), lit(4)),
                ),
                set_expr(
                    "y",
                    bin(BinOp::Add, Expr::Lit(Value::Int(i64::MAX)), lit(1)),
                ),
                set_expr("z", bin(BinOp::Add, var("chua_khai_bao"), lit(1))),
                set_expr("n", Expr::Neg(Box::new(lit(7)))),
                say("s"),
            ],
        )],
        &[("x", 0), ("y", 0), ("z", 0), ("n", 0)],
    );
    let mut vm = Vm::new(s).unwrap();
    vm.start().unwrap();
    assert_eq!(int(&vm, "x"), 20); // AST tường minh, không có chuyện ưu tiên toán tử
    assert_eq!(int(&vm, "y"), i64::MAX); // bão hoà
    assert_eq!(int(&vm, "z"), 1); // biến chưa có đọc ra 0
    assert_eq!(int(&vm, "n"), -7);
}

#[test]
fn set_expr_chia_cho_khong_va_sai_kieu() {
    let div0 = story(
        vec![node(
            "a",
            vec![set_expr("x", bin(BinOp::Div, lit(1), lit(0)))],
        )],
        &[("x", 0)],
    );
    assert_eq!(
        Vm::new(div0).unwrap().start(),
        Err(VmError::DivByZero { var: "x".into() })
    );
    let badty = story(
        vec![node(
            "a",
            vec![set_expr(
                "x",
                bin(BinOp::Add, Expr::Lit(Value::Bool(true)), lit(1)),
            )],
        )],
        &[("x", 0)],
    );
    assert_eq!(
        Vm::new(badty).unwrap().start(),
        Err(VmError::TypeMismatch { var: "x".into() })
    );
}

#[test]
fn story_v2_bi_tu_choi_story_v0_van_chay() {
    let mut s = story(vec![node("a", vec![say("s")])], &[]);
    s.format_version = FORMAT_VERSION + 1;
    assert!(matches!(
        Vm::new(s),
        Err(VmError::UnsupportedFormatVersion(_))
    ));

    let mut s0 = story(vec![node("a", vec![say("s"), Instr::End])], &[]);
    s0.format_version = 0;
    let mut vm = Vm::new(s0).unwrap();
    assert!(vm.start().is_ok());
}

#[test]
fn save_ghi_truoc_khi_co_rng_van_doc_duoc() {
    // Lý do giữ SAVE_VERSION = 1 với serde(default): save không có field rng
    // vẫn nạp được, nhận seed mặc định.
    let s = || story(vec![node("a", vec![say("s"), Instr::End])], &[]);
    let mut vm = Vm::new(s()).unwrap();
    vm.start().unwrap();
    let slot = vm.save("cu", None).unwrap();
    let mut j = serde_json::to_value(&slot).unwrap();
    j["snapshot"].as_object_mut().unwrap().remove("rng");
    let back: SaveSlot = serde_json::from_value(j).unwrap();
    let mut vm2 = Vm::new(s()).unwrap();
    assert!(matches!(vm2.load(&back), Ok(LoadOutcome::Exact(_))));
}
