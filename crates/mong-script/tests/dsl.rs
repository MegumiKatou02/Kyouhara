//! Test parser DSL (bước 2 M2): golden đối chiếu demo DSL ↔ demo JSON
//! (bất biến round-trip số 3 của spec-mongscript mục 9), sinh key, và
//! báo lỗi cú pháp có vị trí, không panic.

use mong_core::{BinOp, CondOp, Expr, Instr, SetOp, StagePos, Story, Value, FORMAT_VERSION};
use mong_script::dsl::{load_story_dsl, StmtKind};
use std::collections::BTreeMap;

const DEMO_DSL: &str = include_str!("data/demo-story.mongscript");
const DEMO_JSON: &str = include_str!("data/demo-story.json");
const DEMO_STRINGS_VI: &str = include_str!("data/demo-strings.vi.json");

/// Bất biến 3: hai frontend, một IR — demo nạp qua DSL phải ra đúng Story
/// như nạp qua JSON (chỉ khác format_version: JSON fixture còn ghi v0).
#[test]
fn golden_demo_dsl_ra_dung_story_nhu_json() {
    let out = load_story_dsl(DEMO_DSL).expect("demo DSL hop le");
    let mut json_story: Story = serde_json::from_str(DEMO_JSON).unwrap();
    json_story.format_version = FORMAT_VERSION; // DSL luon phat ban hien hanh

    assert_eq!(out.story, json_story, "DSL va JSON phai ra cung mot Story");
    assert_eq!(out.generated_keys, 0, "demo da co san key, khong sinh moi");
}

/// Văn bản trong file DSL phải khớp từng chuỗi với bảng chuỗi vi hiện có.
#[test]
fn golden_demo_dsl_ra_dung_bang_chuoi_vi() {
    let out = load_story_dsl(DEMO_DSL).unwrap();
    let expect: BTreeMap<String, String> = serde_json::from_str(DEMO_STRINGS_VI).unwrap();
    assert_eq!(out.strings, expect);
}

/// Dòng thiếu `#~` được sinh key bền vững: đếm tiếp max hiện có, không đè
/// key cũ; key ghi ngược vào AST (bất biến 4, kịch bản "thêm dòng mới").
#[test]
fn sinh_key_dem_tiep_khong_de_key_cu() {
    let src = "@locale vi\n@node a\n  * mot  #~ a.l3\n  * hai\n  > chon -> a\n";
    let mut file = mong_script::dsl::parse_dsl(src).unwrap();
    let out = mong_script::dsl::compile(&mut file).unwrap();
    assert_eq!(out.generated_keys, 2);
    assert_eq!(out.strings.get("a.l3").map(String::as_str), Some("mot"));
    assert_eq!(out.strings.get("a.l4").map(String::as_str), Some("hai")); // 3+1
    assert_eq!(out.strings.get("a.c1").map(String::as_str), Some("chon"));
    // Key mới nằm lại trong AST để formatter ghi ra file.
    let keys: Vec<_> = file.nodes[0]
        .body
        .iter()
        .filter_map(|s| s.key.clone())
        .collect();
    assert_eq!(keys, vec!["a.l3", "a.l4", "a.c1"]);
}

/// Đảo thứ tự dòng/node không đổi key — bất biến 4, kịch bản "đảo thứ tự".
#[test]
fn dao_thu_tu_khong_doi_key() {
    let truoc = "@locale vi\n@node a\n  * mot  #~ a.l1\n  * hai  #~ a.l2\n";
    let sau = "@locale vi\n@node a\n  * hai  #~ a.l2\n  * mot  #~ a.l1\n";
    let s1 = load_story_dsl(truoc).unwrap().strings;
    let s2 = load_story_dsl(sau).unwrap().strings;
    assert_eq!(s1, s2, "van ban van giu dung key sau khi dao dong");
}

#[test]
fn hai_nhom_arm_tach_boi_dong_trong_la_hai_choice() {
    let src = "@locale vi\n@node a\n  > mot -> a\n  > hai -> a\n\n  > ba -> a\n";
    let out = load_story_dsl(src).unwrap();
    let body = &out.story.nodes[0].body;
    assert_eq!(body.len(), 2);
    assert!(matches!(&body[0], Instr::Choice { arms } if arms.len() == 2));
    assert!(matches!(&body[1], Instr::Choice { arms } if arms.len() == 1));
}

/// Arm vắng `->` = target None (kết thúc truyện) — quyết định 4.
#[test]
fn arm_vang_muc_tieu_la_end() {
    let src = "@locale vi\n@node a\n  > thoi\n";
    let out = load_story_dsl(src).unwrap();
    match &out.story.nodes[0].body[0] {
        Instr::Choice { arms } => assert_eq!(arms[0].target, None),
        i => panic!("mong choice, gap {i:?}"),
    }
}

/// Parser khoan dung thứ tự phần đuôi arm; ngữ nghĩa không đổi.
#[test]
fn arm_chap_nhan_thu_tu_linh_hoat() {
    let a = "@locale vi\n@node a\n  > x [ v >= 1 ] -> a { v += 1 }  #~ k\n";
    let b = "@locale vi\n@node a\n  > x -> a [ v >= 1 ] { v += 1 }  #~ k\n";
    assert_eq!(
        load_story_dsl(a).unwrap().story,
        load_story_dsl(b).unwrap().story
    );
}

/// Quy tắc set/set_expr của spec 3.3 — từng dạng ra đúng lệnh IR.
#[test]
fn set_va_set_expr_theo_dung_quy_tac() {
    let src = "@locale vi\n@node a\n  ~ x = 1\n  ~ x += 2\n  ~ x -= 3\n  ~ !x\n  ~ x = y * 2\n  ~ x += y\n  ~ x = -5\n  ~ x = -(y)\n";
    let out = load_story_dsl(src).unwrap();
    let b = &out.story.nodes[0].body;
    assert!(
        matches!(&b[0], Instr::Set { effect } if effect.op == SetOp::Assign && effect.value == Value::Int(1))
    );
    assert!(
        matches!(&b[1], Instr::Set { effect } if effect.op == SetOp::Add && effect.value == Value::Int(2))
    );
    assert!(
        matches!(&b[2], Instr::Set { effect } if effect.op == SetOp::Sub && effect.value == Value::Int(3))
    );
    assert!(matches!(&b[3], Instr::Set { effect } if effect.op == SetOp::Toggle));
    assert!(matches!(&b[4], Instr::SetExpr { .. }));
    // `~ x += y` khai trien thanh x = x + y
    match &b[5] {
        Instr::SetExpr { var, expr } => {
            assert_eq!(var, "x");
            assert_eq!(
                *expr,
                Expr::Bin {
                    op: BinOp::Add,
                    lhs: Box::new(Expr::Var("x".into())),
                    rhs: Box::new(Expr::Var("y".into())),
                }
            );
        }
        i => panic!("mong set_expr, gap {i:?}"),
    }
    // `-5` la literal am (khong phai Neg) — spec muc 4.
    assert!(matches!(&b[6], Instr::Set { effect } if effect.value == Value::Int(-5)));
    // `-( … )` moi la Neg.
    assert!(matches!(
        &b[7],
        Instr::SetExpr {
            expr: Expr::Neg(_),
            ..
        }
    ));
}

#[test]
fn uu_tien_toan_tu_nhan_truoc_cong() {
    let src = "@locale vi\n@node a\n  ~ x = 1 + 2 * 3\n";
    let out = load_story_dsl(src).unwrap();
    match &out.story.nodes[0].body[0] {
        Instr::SetExpr { expr, .. } => match expr {
            Expr::Bin {
                op: BinOp::Add,
                rhs,
                ..
            } => {
                assert!(matches!(**rhs, Expr::Bin { op: BinOp::Mul, .. }))
            }
            e => panic!("mong Add o ngoai cung, gap {e:?}"),
        },
        i => panic!("mong set_expr, gap {i:?}"),
    }
}

#[test]
fn if_else_long_nhau_va_cond_du_bon_op() {
    let src = "@locale vi\n@node a\n  ? v >= 1 {\n    ? s == \"ok\" {\n      * trong  #~ a.l1\n    }\n  } : {\n    ? b != true {\n      end\n    } : {\n      ? v <= 0 {\n        return\n      }\n    }\n  }\n";
    let out = load_story_dsl(src).unwrap();
    match &out.story.nodes[0].body[0] {
        Instr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            assert_eq!(cond.op, CondOp::Ge);
            assert!(matches!(&then_branch[0], Instr::If { cond, .. }
                if cond.op == CondOp::Eq && cond.value == Value::Str("ok".into())));
            assert!(matches!(&else_branch[0], Instr::If { cond, .. }
                if cond.op == CondOp::Ne && cond.value == Value::Bool(true)));
        }
        i => panic!("mong if, gap {i:?}"),
    }
}

/// Các lệnh từ khoá còn lại map 1:1.
#[test]
fn lenh_tu_khoa_map_mot_mot() {
    let src = "@locale vi\n@node a\n  show lan vui left\n  show lan center\n  hide lan\n  wait 500\n  label lap\n  rand may 1 6\n  goto lap\n  call a\n  ext rung {\"manh\": 3}\n  ext im\n  bgm\n";
    let out = load_story_dsl(src).unwrap();
    let b = &out.story.nodes[0].body;
    assert!(matches!(&b[0], Instr::Show { pose: Some(p), pos: StagePos::Left, .. } if p == "vui"));
    assert!(matches!(
        &b[1],
        Instr::Show {
            pose: None,
            pos: StagePos::Center,
            ..
        }
    ));
    assert!(matches!(&b[2], Instr::Hide { .. }));
    assert!(matches!(&b[3], Instr::Wait { ms: 500 }));
    assert!(matches!(&b[4], Instr::Label { name } if name == "lap"));
    assert!(matches!(&b[5], Instr::Rand { min: 1, max: 6, .. }));
    assert!(matches!(&b[6], Instr::Goto { label } if label == "lap"));
    assert!(matches!(&b[7], Instr::Call { target } if target == "a"));
    assert!(
        matches!(&b[8], Instr::Ext { command, args } if command == "rung" && args["manh"] == 3)
    );
    assert!(matches!(&b[9], Instr::Ext { args, .. } if args.is_null()));
    assert!(matches!(&b[10], Instr::Bgm { asset: None }));
}

/// Nhân vật trùng tên từ khoá vẫn nói được nhờ PEG backtrack (dialogue cần `:`).
#[test]
fn nhan_vat_trung_ten_tu_khoa_van_thoai_duoc() {
    let src = "@locale vi\n@node a\n  wait: Chờ chút đã…  #~ a.l1\n";
    let out = load_story_dsl(src).unwrap();
    assert!(matches!(&out.story.nodes[0].body[0],
        Instr::Say { speaker: Some(s), .. } if s == "wait"));
}

#[test]
fn escape_thang_trong_van_ban() {
    let src =
        "@locale vi\n@node a\n  * kênh \\#1 nhé  #~ a.l1\n  > chọn \\[A] \\> hết -> a  #~ a.c1\n";
    let out = load_story_dsl(src).unwrap();
    assert_eq!(out.strings["a.l1"], "kênh #1 nhé");
    assert_eq!(out.strings["a.c1"], "chọn [A] > hết");
}

/// Comment giữ nguyên trong AST (nguyên liệu cho formatter bước 3).
#[test]
fn comment_duoc_giu_trong_ast() {
    let src = "@locale vi\n# dau file\n@node a\n  # trong node\n  * hi  #~ a.l1  # duoi dong\n";
    let file = mong_script::dsl::parse_dsl(src).unwrap();
    assert!(matches!(&file.leading[0].kind, StmtKind::Comment(c) if c == "dau file"));
    assert!(file.nodes[0]
        .body
        .iter()
        .any(|s| matches!(&s.kind, StmtKind::Comment(c) if c == "trong node")));
    let say = file.nodes[0]
        .body
        .iter()
        .find(|s| matches!(s.kind, StmtKind::Say { .. }))
        .unwrap();
    assert_eq!(say.comment.as_deref(), Some("duoi dong"));
    assert_eq!(say.key.as_deref(), Some("a.l1"));
}

// ---- lỗi phải có vị trí, không panic ----

fn loi(src: &str) -> mong_script::dsl::DslError {
    load_story_dsl(src).expect_err("nguon nay phai loi")
}

#[test]
fn loi_bao_dung_dong() {
    let e = loi("@locale vi\n@node a\n  * ok  #~ a.l1\n  ??? sai\n");
    assert_eq!(e.pos.line, 4, "loi phai chi vao dong 4: {e}");
}

#[test]
fn cac_loi_ngu_nghia_cuc_bo() {
    // key trùng
    assert!(loi("@locale vi\n@node a\n  * x  #~ k\n  * y  #~ k\n")
        .message
        .contains("trung"));
    // thiếu @locale
    assert!(loi("@node a\n  end\n").message.contains("@locale"));
    // directive cấp file sau @node
    assert!(loi("@locale vi\n@node a\n  end\n@var x = 1\n")
        .message
        .contains("truoc @node"));
    // pos không hợp lệ
    assert!(loi("@locale vi\n@node a\n  show lan vui giua\n")
        .message
        .contains("left/center/right"));
    // key gắn nhầm lên lệnh không mang văn bản
    assert!(loi("@locale vi\n@node a\n  jump a  #~ k\n")
        .message
        .contains("#~"));
    // hai đích trong một arm
    assert!(loi("@locale vi\n@node a\n  > x -> a -> a\n")
        .message
        .contains("hai dich"));
    // wait vượt u32
    assert!(loi("@locale vi\n@node a\n  wait 99999999999\n")
        .message
        .contains("u32"));
    // ext args không phải JSON
    assert!(loi("@locale vi\n@node a\n  ext rung {hong\n")
        .message
        .contains("JSON"));
}

/// Fuzz-lite mini: nguồn cắt cụt/đột biến không được panic (bất biến 6).
#[test]
fn nguon_hong_khong_panic() {
    let base = DEMO_DSL;
    for cut in (0..base.len()).step_by(7) {
        // cắt tại ranh giới char để có &str hợp lệ
        if !base.is_char_boundary(cut) {
            continue;
        }
        let _ = load_story_dsl(&base[..cut]); // Ok hoặc Err đều được, miễn không panic
    }
    for junk in [
        "\u{0}", "@node", "@node 1", "* \n", "> \n", "~\n", "? {\n", "#~ k\n",
    ] {
        let _ = load_story_dsl(junk);
    }
}
