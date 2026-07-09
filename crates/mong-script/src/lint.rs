//! Luật lint bổ sung ở M2 (docs/lint-rules.md mục C).
//!
//! `validate()` trong `lib.rs` gọi [`lint_m2`]; các luật cần bảng chuỗi
//! (L022–L024) nằm ở [`validate_strings`] vì `Story` không mang văn bản.

use crate::{err, warn, Issue};
use mong_core::{BinOp, Expr, Instr, Node, Story, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Các luật M2 chỉ cần IR: L020, L021, L025, L026.
pub(crate) fn lint_m2(story: &Story) -> Vec<Issue> {
    let mut out = Vec::new();
    let called = nodes_duoc_call(story);
    for n in &story.nodes {
        lint_body(&n.id, &n.body, &mut out);
        if !called.contains(n.id.as_str()) {
            lint_return_mo_coi(&n.id, &n.body, &mut out);
        }
    }
    out
}

/// Node nào là đích của một `call` — `return` ở node khác sẽ underflow
/// (trừ khi node được call gián tiếp qua jump từ node được call; ta chỉ
/// cảnh báo ca chắc chắn: không node nào call tới id này).
fn nodes_duoc_call(story: &Story) -> BTreeSet<&str> {
    fn walk<'a>(body: &'a [Instr], out: &mut BTreeSet<&'a str>) {
        for i in body {
            match i {
                Instr::Call { target } => {
                    out.insert(target.as_str());
                }
                Instr::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    walk(then_branch, out);
                    walk(else_branch, out);
                }
                _ => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    for n in &story.nodes {
        walk(&n.body, &mut out);
    }
    out
}

fn lint_body(id: &str, body: &[Instr], out: &mut Vec<Issue>) {
    for (idx, i) in body.iter().enumerate() {
        match i {
            Instr::Choice { arms } => {
                // L020 — arm vắng target: hợp lệ nhưng thường là gõ thiếu.
                for (k, a) in arms.iter().enumerate() {
                    if a.target.is_none() {
                        out.push(warn(
                            Some(id),
                            format!(
                                "lua chon thu {} khong co dich '->' — chon xong truyen ket thuc",
                                k + 1
                            ),
                        ));
                    }
                }
                // L021 — mọi lệnh sau choice trong cùng block không bao giờ chạy.
                if idx + 1 < body.len() {
                    out.push(warn(
                        Some(id),
                        format!(
                            "{} lenh sau 'choice' trong cung block la bat kha dat",
                            body.len() - idx - 1
                        ),
                    ));
                }
            }
            // L025 — chia/lấy dư cho literal 0.
            Instr::SetExpr { var, expr } => lint_chia_khong(id, var, expr, out),
            Instr::If {
                then_branch,
                else_branch,
                ..
            } => {
                lint_body(id, then_branch, out);
                lint_body(id, else_branch, out);
            }
            _ => {}
        }
    }
}

fn lint_chia_khong(id: &str, var: &str, e: &Expr, out: &mut Vec<Issue>) {
    match e {
        Expr::Bin { op, lhs, rhs } => {
            if matches!(op, BinOp::Div | BinOp::Rem) && matches!(**rhs, Expr::Lit(Value::Int(0))) {
                out.push(warn(
                    Some(id),
                    format!("gan '{var}': chia hoac lay du cho 0 — runtime se bao DivByZero"),
                ));
            }
            lint_chia_khong(id, var, lhs, out);
            lint_chia_khong(id, var, rhs, out);
        }
        Expr::Neg(inner) => lint_chia_khong(id, var, inner, out),
        _ => {}
    }
}

/// L026 — `return` ở node không ai `call`.
fn lint_return_mo_coi(id: &str, body: &[Instr], out: &mut Vec<Issue>) {
    for i in body {
        match i {
            Instr::Return => out.push(warn(
                Some(id),
                "'return' o node khong node nao 'call' toi — runtime se bao CallStackUnderflow"
                    .into(),
            )),
            Instr::If {
                then_branch,
                else_branch,
                ..
            } => {
                lint_return_mo_coi(id, then_branch, out);
                lint_return_mo_coi(id, else_branch, out);
            }
            _ => {}
        }
    }
}

/// Luật cần bảng chuỗi defaultLocale: L022 (key trùng), L023 (key mồ côi),
/// L024 (key thiếu). Gọi riêng vì `validate()` chỉ nhận `Story`.
///
/// Lưu ý L022: hai lệnh **khác nhau** dùng chung key là lỗi; cùng một key
/// xuất hiện đúng một lần thì không. Frontend DSL đã chặn từ lúc parse,
/// nhưng JSON dự án viết tay thì chưa — nên luật này vẫn cần ở tầng IR.
pub fn validate_strings(story: &Story, strings: &BTreeMap<String, String>) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut dem: BTreeMap<&str, usize> = BTreeMap::new();
    for n in &story.nodes {
        gom_key(n, &mut |k| *dem.entry(k).or_insert(0) += 1);
    }

    for (k, c) in &dem {
        if *c > 1 {
            out.push(err(None, format!("key '{k}' duoc {c} dong dung chung")));
        }
        if !strings.contains_key(*k) {
            out.push(err(
                None,
                format!("key '{k}' khong co trong bang chuoi defaultLocale"),
            ));
        }
    }
    for k in strings.keys() {
        if !dem.contains_key(k.as_str()) {
            out.push(warn(
                None,
                format!("key '{k}' mo coi trong bang chuoi — khong dong nao dung"),
            ));
        }
    }
    out
}

/// Duyệt mọi `string_key` của một node (say + arm của choice), kể cả trong
/// nhánh `if`.
fn gom_key<'a>(node: &'a Node, f: &mut impl FnMut(&'a str)) {
    fn walk<'a>(body: &'a [Instr], f: &mut impl FnMut(&'a str)) {
        for i in body {
            match i {
                Instr::Say { text, .. } => f(text.as_str()),
                Instr::Choice { arms } => {
                    for a in arms {
                        f(a.text.as_str());
                    }
                }
                Instr::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    walk(then_branch, f);
                    walk(else_branch, f);
                }
                _ => {}
            }
        }
    }
    walk(&node.body, f);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{validate, Severity};
    use mong_core::{ChoiceArm, Cond, CondOp, SayOpts, FORMAT_VERSION};

    fn story(body: Vec<Instr>) -> Story {
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

    fn arm(text: &str, target: Option<&str>) -> ChoiceArm {
        ChoiceArm {
            text: text.into(),
            target: target.map(Into::into),
            cond: None,
            effects: vec![],
        }
    }

    fn co_canh_bao(iss: &[Issue], chua: &str) -> bool {
        iss.iter()
            .any(|i| i.severity == Severity::Warning && i.message.contains(chua))
    }
    fn co_loi(iss: &[Issue], chua: &str) -> bool {
        iss.iter()
            .any(|i| i.severity == Severity::Error && i.message.contains(chua))
    }

    #[test]
    fn l020_arm_vang_target() {
        let s = story(vec![Instr::Choice {
            arms: vec![arm("k1", Some("a")), arm("k2", None)],
        }]);
        let iss = validate(&s);
        assert!(co_canh_bao(&iss, "khong co dich"));
        // Arm có target thì không cảnh báo — đúng một cảnh báo L020.
        assert_eq!(
            iss.iter()
                .filter(|i| i.message.contains("khong co dich"))
                .count(),
            1
        );
    }

    #[test]
    fn l021_lenh_sau_choice() {
        let s = story(vec![
            Instr::Choice {
                arms: vec![arm("k1", Some("a"))],
            },
            Instr::End,
        ]);
        assert!(co_canh_bao(&validate(&s), "bat kha dat"));

        // choice là lệnh cuối block → không cảnh báo.
        let ok = story(vec![Instr::Choice {
            arms: vec![arm("k1", Some("a"))],
        }]);
        assert!(!co_canh_bao(&validate(&ok), "bat kha dat"));
    }

    #[test]
    fn l021_bat_ca_trong_nhanh_if() {
        let s = story(vec![Instr::If {
            cond: Cond {
                var: "v".into(),
                op: CondOp::Ge,
                value: Value::Int(1),
            },
            then_branch: vec![
                Instr::Choice {
                    arms: vec![arm("k1", Some("a"))],
                },
                Instr::End,
            ],
            else_branch: vec![],
        }]);
        let mut s = s;
        s.variables.insert("v".into(), Value::Int(0));
        assert!(co_canh_bao(&validate(&s), "bat kha dat"));
    }

    #[test]
    fn l025_chia_cho_khong() {
        let s = story(vec![Instr::SetExpr {
            var: "x".into(),
            expr: Expr::Bin {
                op: BinOp::Div,
                lhs: Box::new(Expr::Var("x".into())),
                rhs: Box::new(Expr::Lit(Value::Int(0))),
            },
        }]);
        let mut s = s;
        s.variables.insert("x".into(), Value::Int(1));
        assert!(co_canh_bao(&validate(&s), "chia hoac lay du cho 0"));
    }

    #[test]
    fn l026_return_khong_ai_call() {
        let s = story(vec![Instr::Return]);
        assert!(co_canh_bao(&validate(&s), "CallStackUnderflow"));

        // Có node call tới thì im lặng.
        let mut s2 = story(vec![Instr::Call { target: "b".into() }]);
        s2.nodes.push(Node {
            id: "b".into(),
            title: String::new(),
            scene: None,
            body: vec![Instr::Return],
        });
        assert!(!co_canh_bao(&validate(&s2), "CallStackUnderflow"));
    }

    #[test]
    fn l022_key_trung() {
        let s = story(vec![
            Instr::Say {
                speaker: None,
                text: "k".into(),
                opts: SayOpts::default(),
            },
            Instr::Say {
                speaker: None,
                text: "k".into(),
                opts: SayOpts::default(),
            },
        ]);
        let strings = BTreeMap::from([("k".to_string(), "x".to_string())]);
        assert!(co_loi(&validate_strings(&s, &strings), "dung chung"));
    }

    #[test]
    fn l023_key_mo_coi() {
        let s = story(vec![Instr::End]);
        let strings = BTreeMap::from([("thua".to_string(), "x".to_string())]);
        assert!(co_canh_bao(&validate_strings(&s, &strings), "mo coi"));
    }

    #[test]
    fn l024_key_thieu_o_default() {
        let s = story(vec![Instr::Say {
            speaker: None,
            text: "vang".into(),
            opts: SayOpts::default(),
        }]);
        let iss = validate_strings(&s, &BTreeMap::new());
        assert!(co_loi(&iss, "khong co trong bang chuoi"));
    }

    #[test]
    fn key_trong_nhanh_if_van_duoc_dem() {
        let s = story(vec![Instr::If {
            cond: Cond {
                var: "v".into(),
                op: CondOp::Ge,
                value: Value::Int(1),
            },
            then_branch: vec![Instr::Say {
                speaker: None,
                text: "k".into(),
                opts: SayOpts::default(),
            }],
            else_branch: vec![],
        }]);
        let strings = BTreeMap::from([("k".to_string(), "x".to_string())]);
        // Không mồ côi, không thiếu → im lặng.
        assert!(validate_strings(&s, &strings).is_empty());
    }
}
