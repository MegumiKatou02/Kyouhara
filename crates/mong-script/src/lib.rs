//! mong-script — nạp cốt truyện và kiểm tra tính toàn vẹn (lint).
//!
//! M0: nạp JSON (định dạng dự án `.mong`) thành [`Story`] + `validate`.
//! M2 sẽ thêm parser DSL MộngScript; cả hai đường đều ra cùng một IR.
// "M2 sẽ thêm parser DSL" → "M2: parser DSL trong `dsl` — cả hai đường ra cùng một IR."
use mong_core::{Expr, Instr, Node, Story, FORMAT_VERSION};
use std::collections::BTreeSet;
pub mod dsl;
mod lint;
pub use lint::validate_strings;

/// Mức độ của một phát hiện lint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Một phát hiện của bộ lint.
#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    pub node: Option<String>,
    pub message: String,
}

/// Nạp cốt truyện từ JSON của dự án.
pub fn load_story_json(json: &str) -> Result<Story, serde_json::Error> {
    serde_json::from_str(json)
}

pub(crate) fn err(node: Option<&str>, msg: String) -> Issue {
    Issue {
        severity: Severity::Error,
        node: node.map(String::from),
        message: msg,
    }
}
pub(crate) fn warn(node: Option<&str>, msg: String) -> Issue {
    Issue {
        severity: Severity::Warning,
        node: node.map(String::from),
        message: msg,
    }
}

/// Gom mọi đích nhảy (jump/call/choice) trong một block, đệ quy qua `if`.
fn collect_targets<'a>(body: &'a [Instr], out: &mut Vec<&'a str>) {
    for i in body {
        match i {
            Instr::Jump { target } | Instr::Call { target } => out.push(target),
            Instr::Choice { arms } => {
                for a in arms {
                    if let Some(t) = &a.target {
                        out.push(t);
                    }
                }
            }
            Instr::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_targets(then_branch, out);
                collect_targets(else_branch, out);
            }
            _ => {}
        }
    }
}

/// Gom mọi biến được dùng (cond + effect), đệ quy qua `if`.
fn collect_vars<'a>(body: &'a [Instr], out: &mut Vec<&'a str>) {
    for i in body {
        match i {
            Instr::Rand { var, .. } => out.push(var),
            Instr::SetExpr { var, expr } => {
                out.push(var);
                collect_expr_vars(expr, out);
            }
            Instr::Set { effect } => out.push(&effect.var),
            Instr::Choice { arms } => {
                for a in arms {
                    if let Some(c) = &a.cond {
                        out.push(&c.var);
                    }
                    for e in &a.effects {
                        out.push(&e.var);
                    }
                }
            }
            Instr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                out.push(&cond.var);
                collect_vars(then_branch, out);
                collect_vars(else_branch, out);
            }
            _ => {}
        }
    }
}

/// Gom biến được đọc trong một biểu thức.
fn collect_expr_vars<'a>(e: &'a Expr, out: &mut Vec<&'a str>) {
    match e {
        Expr::Var(v) => out.push(v),
        Expr::Neg(x) => collect_expr_vars(x, out),
        Expr::Bin { lhs, rhs, .. } => {
            collect_expr_vars(lhs, out);
            collect_expr_vars(rhs, out);
        }
        Expr::Lit(_) => {}
    }
}

/// Luật label/goto (v1): label trùng tên, label trong nhánh if,
/// goto tới label không tồn tại — đều là lỗi.
fn lint_labels(node: &Node, issues: &mut Vec<Issue>) {
    let mut labels: BTreeSet<&str> = BTreeSet::new();
    for i in &node.body {
        if let Instr::Label { name } = i {
            if !labels.insert(name) {
                issues.push(err(
                    Some(&node.id),
                    format!("label '{name}' bi trung trong node"),
                ));
            }
        }
    }
    fn walk(id: &str, body: &[Instr], top: bool, labels: &BTreeSet<&str>, out: &mut Vec<Issue>) {
        for i in body {
            match i {
                Instr::Label { name } if !top => out.push(err(
                    Some(id),
                    format!("label '{name}' nam trong nhanh if — chi dat o cap cao nhat cua node"),
                )),
                Instr::Goto { label } if !labels.contains(label.as_str()) => out.push(err(
                    Some(id),
                    format!("goto toi label '{label}' khong ton tai trong node"),
                )),
                Instr::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    walk(id, then_branch, false, labels, out);
                    walk(id, else_branch, false, labels, out);
                }
                _ => {}
            }
        }
    }
    walk(&node.id, &node.body, true, &labels, issues);
}

/// Luật rand (v1): khoảng rỗng là lỗi từ lúc soạn, khỏi đợi runtime.
fn lint_rand(id: &str, body: &[Instr], issues: &mut Vec<Issue>) {
    for i in body {
        match i {
            Instr::Rand { var, min, max } if min > max => issues.push(err(
                Some(id),
                format!("rand vao '{var}': min {min} > max {max}"),
            )),
            Instr::If {
                then_branch,
                else_branch,
                ..
            } => {
                lint_rand(id, then_branch, issues);
                lint_rand(id, else_branch, issues);
            }
            _ => {}
        }
    }
}

/// Bộ lint cốt truyện — chuyển thẳng các quy tắc đã kiểm chứng ở prototype v4
/// sang chạy trên IR (chính xác tuyệt đối, dùng được trong CI).
pub fn validate(story: &Story) -> Vec<Issue> {
    let mut issues = Vec::new();

    if story.format_version > FORMAT_VERSION {
        issues.push(err(
            None,
            format!(
                "formatVersion {} moi hon phien ban ho tro ({FORMAT_VERSION})",
                story.format_version
            ),
        ));
    }

    let ids: BTreeSet<&str> = story.nodes.iter().map(|n| n.id.as_str()).collect();

    if ids.len() != story.nodes.len() {
        issues.push(err(None, "co node trung id".into()));
    }
    if !ids.contains(story.start.as_str()) {
        issues.push(err(
            None,
            format!("diem bat dau '{}' khong ton tai", story.start),
        ));
        return issues;
    }

    // Dich den phai ton tai + bien phai duoc khai bao.
    for n in &story.nodes {
        let mut targets = Vec::new();
        collect_targets(&n.body, &mut targets);
        for t in targets {
            if !ids.contains(t) {
                issues.push(err(
                    Some(&n.id),
                    format!("tro toi node '{t}' khong ton tai"),
                ));
            }
        }
        let mut vars = Vec::new();
        collect_vars(&n.body, &mut vars);
        for v in vars {
            if !story.variables.contains_key(v) {
                issues.push(err(Some(&n.id), format!("dung bien '{v}' chua khai bao")));
            }
        }
        // Soft-lock: choice ma moi arm deu co dieu kien.
        for i in &n.body {
            if let Instr::Choice { arms } = i {
                if !arms.is_empty() && arms.iter().all(|a| a.cond.is_some()) {
                    issues.push(warn(
                        Some(&n.id),
                        "moi lua chon deu co dieu kien — co the ket thuc dot ngot".into(),
                    ));
                }
                if arms.is_empty() {
                    issues.push(err(Some(&n.id), "lenh choice khong co lua chon nao".into()));
                }
            }
        }

        lint_labels(n, &mut issues);
        lint_rand(&n.id, &n.body, &mut issues);
    }

    // Kha nang tiep can tu diem bat dau (BFS).
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut queue: Vec<&str> = vec![story.start.as_str()];
    while let Some(id) = queue.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(n) = story.nodes.iter().find(|n| n.id == id) {
            let mut targets = Vec::new();
            collect_targets(&n.body, &mut targets);
            for t in targets {
                if ids.contains(t) {
                    queue.push(t);
                }
            }
        }
    }
    for n in &story.nodes {
        if !seen.contains(n.id.as_str()) {
            issues.push(err(
                Some(&n.id),
                "nhanh mo coi: khong den duoc tu diem bat dau".into(),
            ));
        }
    }

    issues.extend(lint::lint_m2(story));
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use mong_core::{ChoiceArm, Cond, CondOp, Node, Value};
    use std::collections::BTreeMap;

    fn story_with(nodes: Vec<Node>, vars: &[(&str, i64)]) -> Story {
        let mut m = BTreeMap::new();
        for (k, v) in vars {
            m.insert(k.to_string(), Value::Int(*v));
        }
        Story {
            format_version: 0,
            title: String::new(),
            default_locale: "vi".into(),
            locales: vec![],
            variables: m,
            start: "a".into(),
            nodes,
        }
    }

    #[test]
    fn bat_dich_den_khong_ton_tai() {
        let s = story_with(
            vec![Node {
                id: "a".into(),
                title: String::new(),
                scene: None,
                body: vec![Instr::Jump {
                    target: "ma".into(),
                }],
            }],
            &[],
        );
        let iss = validate(&s);
        assert!(iss
            .iter()
            .any(|i| i.severity == Severity::Error && i.message.contains("'ma'")));
    }

    #[test]
    fn bat_nhanh_mo_coi_va_bien_chua_khai_bao() {
        let s = story_with(
            vec![
                Node {
                    id: "a".into(),
                    title: String::new(),
                    scene: None,
                    body: vec![Instr::End],
                },
                Node {
                    id: "orphan".into(),
                    title: String::new(),
                    scene: None,
                    body: vec![Instr::If {
                        cond: Cond {
                            var: "x".into(),
                            op: CondOp::Ge,
                            value: Value::Int(1),
                        },
                        then_branch: vec![],
                        else_branch: vec![],
                    }],
                },
            ],
            &[],
        );
        let iss = validate(&s);
        assert!(iss.iter().any(|i| i.message.contains("mo coi")));
        assert!(iss.iter().any(|i| i.message.contains("'x'")));
    }

    #[test]
    fn canh_bao_soft_lock() {
        let s = story_with(
            vec![Node {
                id: "a".into(),
                title: String::new(),
                scene: None,
                body: vec![Instr::Choice {
                    arms: vec![ChoiceArm {
                        text: "t".into(),
                        target: None,
                        cond: Some(Cond {
                            var: "tc".into(),
                            op: CondOp::Ge,
                            value: Value::Int(1),
                        }),
                        effects: vec![],
                    }],
                }],
            }],
            &[("tc", 0)],
        );
        let iss = validate(&s);
        assert!(iss.iter().any(|i| i.severity == Severity::Warning));
    }

    #[test]
    fn bat_loi_label_goto_va_rand_v1() {
        use mong_core::Expr;
        let s = story_with(
            vec![Node {
                id: "a".into(),
                title: String::new(),
                scene: None,
                body: vec![
                    Instr::Label { name: "l".into() },
                    Instr::Label { name: "l".into() }, // trùng
                    Instr::If {
                        cond: Cond {
                            var: "x".into(),
                            op: CondOp::Ge,
                            value: Value::Int(1),
                        },
                        then_branch: vec![
                            Instr::Label {
                                name: "trong_if".into(),
                            }, // label trong nhánh
                            Instr::Goto { label: "ma".into() }, // label lạ
                        ],
                        else_branch: vec![],
                    },
                    Instr::Rand {
                        var: "x".into(),
                        min: 3,
                        max: 1,
                    }, // khoảng ngược
                    Instr::SetExpr {
                        var: "x".into(),
                        expr: Expr::Var("y".into()),
                    }, // y chưa khai báo
                ],
            }],
            &[("x", 0)],
        );
        let msgs: Vec<String> = validate(&s).into_iter().map(|i| i.message).collect();
        assert!(msgs.iter().any(|m| m.contains("bi trung")));
        assert!(msgs.iter().any(|m| m.contains("nam trong nhanh if")));
        assert!(msgs.iter().any(|m| m.contains("'ma'")));
        assert!(msgs.iter().any(|m| m.contains("min 3 > max 1")));
        assert!(msgs.iter().any(|m| m.contains("'y'")));
    }
}
