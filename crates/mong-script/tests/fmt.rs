//! Test bước 3 M2 — formatter và các bất biến round-trip còn lại của
//! spec-mongscript mục 9: số 1 (parse∘print = id trên IR chuẩn tắc),
//! số 2 (fmt idempotent), số 5 (comment bảo toàn).
//!
//! Property test dùng PRNG xorshift tự viết cùng phong cách fuzz-lite M1
//! (xác định theo seed, không thêm dependency).

use mong_core::{
    BinOp, ChoiceArm, Cond, CondOp, Effect, Expr, Instr, Node, SayOpts, SetOp, StagePos, Story,
    Value, FORMAT_VERSION,
};
use mong_script::dsl::{canonicalize_story, format_dsl, load_story_dsl, print_story};
use std::collections::BTreeMap;

const DEMO_DSL: &str = include_str!("data/demo-story.mongscript");
const DEMO_JSON: &str = include_str!("data/demo-story.json");
const DEMO_STRINGS_VI: &str = include_str!("data/demo-strings.vi.json");

/// Fixture demo là điểm bất động của formatter — vừa chốt canonical form
/// bằng golden, vừa là bất biến 2 trên input thật.
#[test]
fn golden_demo_la_diem_bat_dong_cua_fmt() {
    let out = format_dsl(DEMO_DSL).expect("demo format duoc");
    assert_eq!(out.generated_keys, 0);
    assert_eq!(out.text, DEMO_DSL, "fixture phai o dang canonical");
}

/// Bất biến 1 trên demo: JSON → IR → in DSL → parse → đúng IR + đúng chuỗi.
#[test]
fn golden_in_demo_tu_ir_roi_parse_lai() {
    let mut story: Story = serde_json::from_str(DEMO_JSON).unwrap();
    story.format_version = FORMAT_VERSION;
    let strings: BTreeMap<String, String> = serde_json::from_str(DEMO_STRINGS_VI).unwrap();

    let text = print_story(&story, &strings).expect("in duoc");
    let back = load_story_dsl(&text).expect("parse lai duoc");
    assert_eq!(back.story, story);
    assert_eq!(back.strings, strings);

    // Text in ra đã canonical → fmt là no-op (bất biến 2 trên đường IR).
    assert_eq!(format_dsl(&text).unwrap().text, text);
}

/// Bất biến 5: comment nguyên dòng + comment đuôi sống sót qua fmt,
/// và fmt idempotent trên file có trivia lộn xộn.
#[test]
fn comment_bao_toan_va_fmt_on_dinh() {
    let src = "# dau file\n\n\n@locale vi\n@node a\n\n\n  # trong node\n  *   hi   #~ a.l1   # duoi dong\n\n\n  jump a # tren jump\n";
    let out1 = format_dsl(src).unwrap();
    assert!(out1.text.contains("# dau file"));
    assert!(out1.text.contains("  # trong node"));
    assert!(out1.text.contains("* hi  #~ a.l1  # duoi dong"));
    assert!(out1.text.contains("jump a  # tren jump"));
    assert!(!out1.text.contains("\n\n\n"), "khong con dong trong kep");
    let out2 = format_dsl(&out1.text).unwrap();
    assert_eq!(out1.text, out2.text, "fmt phai idempotent");
}

/// fmt sinh key cho dòng thiếu và ghi `#~` ra text; lần hai ổn định.
#[test]
fn fmt_sinh_key_va_on_dinh_lan_hai() {
    let src = "@locale vi\n@node a\n  * mot  #~ a.l2\n  * hai\n  > chon -> a\n";
    let out = format_dsl(src).unwrap();
    assert_eq!(out.generated_keys, 2);
    assert!(out.text.contains("* hai  #~ a.l3"));
    assert!(out.text.contains("> chon -> a  #~ a.c1"));
    let out2 = format_dsl(&out.text).unwrap();
    assert_eq!(out2.generated_keys, 0);
    assert_eq!(out2.text, out.text);
}

/// Dòng trống giữa hai nhóm `>` phải sống sót (nó mang ngữ nghĩa cắt nhóm).
#[test]
fn fmt_giu_dong_trong_giua_hai_nhom_arm() {
    let src = "@locale vi\n@node a\n  > mot -> a  #~ k1\n\n  > hai -> a  #~ k2\n";
    let text = format_dsl(src).unwrap().text;
    let story1 = load_story_dsl(src).unwrap().story;
    let story2 = load_story_dsl(&text).unwrap().story;
    assert_eq!(story1, story2);
    assert_eq!(story1.nodes[0].body.len(), 2, "van la hai choice");
}

/// Dạng IR thoái hoá: in ra sẽ parse về dạng chuẩn tắc — đúng bằng
/// `canonicalize_story`, và canonicalize là idempotent.
#[test]
fn ir_thoai_hoa_ve_dang_chuan_tac() {
    let story = Story {
        format_version: FORMAT_VERSION,
        title: "  t  ".into(),
        default_locale: "vi".into(),
        locales: vec![],
        variables: BTreeMap::new(),
        start: "a".into(),
        nodes: vec![Node {
            id: "a".into(),
            title: String::new(),
            scene: None,
            body: vec![
                Instr::SetExpr {
                    var: "x".into(),
                    expr: Expr::Lit(Value::Int(5)),
                },
                Instr::Set {
                    effect: Effect {
                        var: "b".into(),
                        op: SetOp::Toggle,
                        value: Value::Int(7),
                    },
                },
                Instr::End,
            ],
        }],
    };
    let canon = canonicalize_story(&story);
    assert_ne!(story, canon);
    assert_eq!(canonicalize_story(&canon), canon, "canonicalize idempotent");

    let text = print_story(&story, &BTreeMap::new()).unwrap();
    let back = load_story_dsl(&text).unwrap().story;
    assert_eq!(back, canon, "parse(print(ir)) == canonicalize(ir)");
}

/// `set_expr {x, x + <Lit Int>}` KHÔNG thoái hoá: phải in dạng dài
/// `~ x = x + n` để không bị parse nhầm thành `set {add}`.
#[test]
fn set_expr_cong_literal_in_dang_dai() {
    let story = story_voi_body(vec![Instr::SetExpr {
        var: "x".into(),
        expr: Expr::Bin {
            op: BinOp::Add,
            lhs: Box::new(Expr::Var("x".into())),
            rhs: Box::new(Expr::Lit(Value::Int(2))),
        },
    }]);
    let text = print_story(&story, &BTreeMap::new()).unwrap();
    assert!(text.contains("~ x = x + 2"), "text:\n{text}");
    assert_eq!(load_story_dsl(&text).unwrap().story, story);
}

/// Ngoặc tối thiểu nhưng đủ: cây lệch phải và ưu tiên thấp bên trong cao.
#[test]
fn in_bieu_thuc_giu_dung_cay() {
    let e = |s: Expr| {
        story_voi_body(vec![Instr::SetExpr {
            var: "x".into(),
            expr: s,
        }])
    };
    let cases = vec![
        // x = a - (b - c)
        Expr::Bin {
            op: BinOp::Sub,
            lhs: Box::new(Expr::Var("a".into())),
            rhs: Box::new(Expr::Bin {
                op: BinOp::Sub,
                lhs: Box::new(Expr::Var("b".into())),
                rhs: Box::new(Expr::Var("c".into())),
            }),
        },
        // x = (a + b) * c
        Expr::Bin {
            op: BinOp::Mul,
            lhs: Box::new(Expr::Bin {
                op: BinOp::Add,
                lhs: Box::new(Expr::Var("a".into())),
                rhs: Box::new(Expr::Var("b".into())),
            }),
            rhs: Box::new(Expr::Var("c".into())),
        },
        // x = -(a % 2)
        Expr::Neg(Box::new(Expr::Bin {
            op: BinOp::Rem,
            lhs: Box::new(Expr::Var("a".into())),
            rhs: Box::new(Expr::Lit(Value::Int(2))),
        })),
    ];
    for expr in cases {
        let story = e(expr);
        let text = print_story(&story, &BTreeMap::new()).unwrap();
        assert_eq!(load_story_dsl(&text).unwrap().story, story, "text:\n{text}");
    }
}

/// Key thiếu trong bảng chuỗi → in phải lỗi, không in file thiếu thoại.
#[test]
fn in_loi_khi_thieu_chuoi() {
    let story = story_voi_body(vec![Instr::Say {
        speaker: None,
        text: "vang.l1".into(),
        opts: SayOpts::default(),
    }]);
    let e = print_story(&story, &BTreeMap::new()).expect_err("phai loi");
    assert!(e.message.contains("vang.l1"));
}

fn story_voi_body(body: Vec<Instr>) -> Story {
    Story {
        format_version: FORMAT_VERSION,
        title: String::new(),
        default_locale: "vi".into(),
        locales: vec![],
        variables: BTreeMap::new(),
        start: "a".into(),
        nodes: vec![Node {
            id: "a".into(),
            title: String::new(),
            scene: None,
            body,
        }],
    }
}

// ================= property test: sinh Story chuẩn tắc ngẫu nhiên =========

struct TestRng(u64);

impl TestRng {
    fn next(&mut self) -> u64 {
        // xorshift64* — đủ tốt cho sinh dữ liệu test, xác định theo seed.
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

const IDENTS: &[&str] = &["a1", "b_2", "x", "thien_cam", "v", "wait", "endx"];
const POSES: &[&str] = &["vui", "thuong", "cuoi"];
// Văn bản cố tình chứa ký tự cần escape và unicode.
const TEXTS: &[&str] = &[
    "Xin chào!",
    "kênh #1 nhé",
    "chọn [A] {x} -> hết",
    "dấu > đơn lẻ, và -",
    "…Ừ nhỉ: \"ok\"?",
    "(vui) mở đầu bằng ngoặc",
];

struct Gen<'a> {
    r: &'a mut TestRng,
    node_ids: Vec<String>,
    strings: BTreeMap<String, String>,
    key_counter: u64,
}

impl<'a> Gen<'a> {
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.r.below(xs.len() as u64) as usize]
    }

    fn text_key(&mut self) -> String {
        self.key_counter += 1;
        let key = format!("k.t{}", self.key_counter);
        let text = self.pick(TEXTS).to_string();
        self.strings.insert(key.clone(), text);
        key
    }

    fn value(&mut self) -> Value {
        match self.r.below(3) {
            0 => Value::Int(self.r.next() as i64 % 1000),
            1 => Value::Bool(self.r.chance(50)),
            _ => Value::Str(
                self.pick(&["a\"b", "x\\y", "nhiều\ndòng", "thường"])
                    .to_string(),
            ),
        }
    }

    fn cond(&mut self) -> Cond {
        Cond {
            var: self.pick(IDENTS).to_string(),
            op: self.pick(&[CondOp::Ge, CondOp::Le, CondOp::Eq, CondOp::Ne]),
            value: self.value(),
        }
    }

    /// Effect chuẩn tắc: add/sub là Int, toggle là Bool(true).
    fn effect(&mut self) -> Effect {
        let var = self.pick(IDENTS).to_string();
        match self.r.below(4) {
            0 => Effect {
                var,
                op: SetOp::Assign,
                value: self.value(),
            },
            1 => Effect {
                var,
                op: SetOp::Add,
                value: Value::Int(self.r.next() as i64 % 100),
            },
            2 => Effect {
                var,
                op: SetOp::Sub,
                value: Value::Int(self.r.next() as i64 % 100),
            },
            _ => Effect {
                var,
                op: SetOp::Toggle,
                value: Value::Bool(true),
            },
        }
    }

    /// Biểu thức tuỳ ý, gốc không phải Lit (tránh dạng thoái hoá của set_expr).
    fn expr_khong_lit(&mut self, depth: u32) -> Expr {
        match self.r.below(3) {
            0 => Expr::Var(self.pick(IDENTS).to_string()),
            1 => Expr::Neg(Box::new(self.expr(depth))),
            _ => Expr::Bin {
                op: self.pick(&[BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div, BinOp::Rem]),
                lhs: Box::new(self.expr(depth)),
                rhs: Box::new(self.expr(depth)),
            },
        }
    }

    fn expr(&mut self, depth: u32) -> Expr {
        if depth == 0 || self.r.chance(40) {
            if self.r.chance(50) {
                Expr::Lit(Value::Int(self.r.next() as i64 % 100))
            } else {
                Expr::Var(self.pick(IDENTS).to_string())
            }
        } else {
            self.expr_khong_lit(depth - 1)
        }
    }

    fn say_opts(&mut self) -> SayOpts {
        let pos = if self.r.chance(50) {
            Some(self.pick(&[StagePos::Left, StagePos::Center, StagePos::Right]))
        } else {
            None
        };
        SayOpts {
            // POSES không trùng left/center/right nên mọi tổ hợp in được.
            pose: self.r.chance(50).then(|| self.pick(POSES).to_string()),
            pos,
            sfx: self.r.chance(25).then(|| self.pick(IDENTS).to_string()),
            exit: self.r.chance(20),
        }
    }

    fn instr(&mut self, depth: u32) -> Instr {
        match self.r.below(if depth > 0 { 17 } else { 16 }) {
            0 => Instr::Say {
                speaker: self.r.chance(60).then(|| self.pick(IDENTS).to_string()),
                text: self.text_key(),
                opts: self.say_opts(),
            },
            1 => {
                let n = 1 + self.r.below(3);
                let arms = (0..n)
                    .map(|_| ChoiceArm {
                        text: self.text_key(),
                        target: self.r.chance(80).then(|| self.pick_node()),
                        cond: self.r.chance(40).then(|| self.cond()),
                        effects: (0..self.r.below(3)).map(|_| self.effect()).collect(),
                    })
                    .collect();
                Instr::Choice { arms }
            }
            2 => Instr::Set {
                effect: self.effect(),
            },
            3 => Instr::SetExpr {
                var: self.pick(IDENTS).to_string(),
                expr: self.expr_khong_lit(2),
            },
            4 => Instr::Jump {
                target: self.pick_node(),
            },
            5 => Instr::Call {
                target: self.pick_node(),
            },
            6 => Instr::Return,
            7 => Instr::Label {
                name: self.pick(IDENTS).to_string(),
            },
            8 => Instr::Goto {
                label: self.pick(IDENTS).to_string(),
            },
            9 => Instr::End,
            10 => Instr::Scene {
                scene: self.pick(IDENTS).to_string(),
                transition: self
                    .r
                    .chance(50)
                    .then(|| self.pick(&["fade", "cut"]).to_string()),
            },
            11 => Instr::Show {
                character: self.pick(IDENTS).to_string(),
                pose: self.r.chance(50).then(|| self.pick(POSES).to_string()),
                pos: self.pick(&[StagePos::Left, StagePos::Center, StagePos::Right]),
            },
            12 => Instr::Hide {
                character: self.pick(IDENTS).to_string(),
            },
            13 => Instr::Wait {
                ms: self.r.next() as u32 % 10_000,
            },
            14 => match self.r.below(3) {
                0 => Instr::Sfx {
                    asset: self.pick(IDENTS).to_string(),
                },
                1 => Instr::Bgm {
                    asset: Some(self.pick(IDENTS).to_string()),
                },
                _ => Instr::Bgm { asset: None },
            },
            15 => match self.r.below(3) {
                0 => Instr::Rand {
                    var: self.pick(IDENTS).to_string(),
                    min: -(self.r.next() as i64 % 10),
                    max: self.r.next() as i64 % 100,
                },
                1 => Instr::Ext {
                    command: self.pick(IDENTS).to_string(),
                    args: serde_json::json!({"n": self.r.below(9), "s": "a#b"}),
                },
                _ => Instr::Ext {
                    command: self.pick(IDENTS).to_string(),
                    args: serde_json::Value::Null,
                },
            },
            _ => Instr::If {
                cond: self.cond(),
                then_branch: self.body(depth - 1, 3),
                else_branch: if self.r.chance(60) {
                    self.body(depth - 1, 3)
                } else {
                    Vec::new()
                },
            },
        }
    }

    fn pick_node(&mut self) -> String {
        self.node_ids[self.r.below(self.node_ids.len() as u64) as usize].clone()
    }

    fn body(&mut self, depth: u32, max_len: u64) -> Vec<Instr> {
        (0..self.r.below(max_len + 1))
            .map(|_| self.instr(depth))
            .collect()
    }
}

fn gen_story(r: &mut TestRng) -> (Story, BTreeMap<String, String>) {
    let n_nodes = 1 + r.below(4) as usize;
    let node_ids: Vec<String> = (0..n_nodes).map(|i| format!("n{i}")).collect();
    let mut g = Gen {
        r,
        node_ids: node_ids.clone(),
        strings: BTreeMap::new(),
        key_counter: 0,
    };

    let mut variables = BTreeMap::new();
    for _ in 0..g.r.below(3) {
        let name = g.pick(IDENTS).to_string();
        let val = g.value();
        variables.insert(name, val);
    }

    let nodes: Vec<Node> = node_ids
        .iter()
        .map(|id| Node {
            id: id.clone(),
            title: if g.r.chance(50) {
                g.pick(TEXTS).to_string()
            } else {
                String::new()
            },
            scene: g.r.chance(50).then(|| g.pick(IDENTS).to_string()),
            body: g.body(2, 6),
        })
        .collect();

    let start = g.pick_node();
    let story = Story {
        format_version: FORMAT_VERSION,
        title: if g.r.chance(70) {
            "Truyện #test".into()
        } else {
            String::new()
        },
        default_locale: "vi".into(),
        locales: if g.r.chance(40) {
            vec!["en".into(), "pt-BR".into()]
        } else {
            vec![]
        },
        variables,
        start,
        nodes,
    };
    let strings = g.strings;
    (story, strings)
}

/// Bất biến 1 + 2 trên Story chuẩn tắc ngẫu nhiên:
/// parse(print(ir)) == ir, chuỗi giữ nguyên, và text in ra là điểm bất động
/// của fmt. Chỉnh seed/số vòng qua MONG_FUZZ_SEED / MONG_FUZZ_ITERS.
#[test]
fn property_round_trip_ir_dsl_ir() {
    let base = env_u64("MONG_FUZZ_SEED", 0xD51_5EED);
    let cases = env_u64("MONG_FUZZ_ITERS", 200);
    for i in 0..cases {
        let mut r = TestRng(base.wrapping_add(i.wrapping_mul(0x9e37_79b9)).max(1));
        let (story, strings) = gen_story(&mut r);
        // Story sinh ra đã chuẩn tắc — khẳng định để generator không trôi.
        assert_eq!(
            canonicalize_story(&story),
            story,
            "seed {i}: gen phai chuan tac"
        );

        let text = print_story(&story, &strings)
            .unwrap_or_else(|e| panic!("seed {i}: khong in duoc: {e}"));
        let back = load_story_dsl(&text)
            .unwrap_or_else(|e| panic!("seed {i}: khong parse lai duoc: {e}\n---\n{text}"));
        assert_eq!(back.story, story, "seed {i}: IR lech\n---\n{text}");
        assert_eq!(
            back.strings, strings,
            "seed {i}: bang chuoi lech\n---\n{text}"
        );

        let fmt1 = format_dsl(&text).unwrap_or_else(|e| panic!("seed {i}: fmt loi: {e}"));
        assert_eq!(
            fmt1.text, text,
            "seed {i}: print_story phai ra dang canonical"
        );
    }
}
